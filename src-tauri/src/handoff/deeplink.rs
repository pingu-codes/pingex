//! Receiving a `codex://` link and resolving it against the running home.
//!
//! A handoff opens the *same thread in the same state root*, not merely a
//! matching folder — so the resolved payload tells the frontend whether the
//! requested home is the running one, letting it navigate, offer a deliberate
//! switch, or show an actionable error rather than silently falling back.

use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager};

use super::url::{parse_deep_link, DeepLink, DeepLinkKind};
use crate::AppState;

/// Expand a leading `~` to the user's home directory.
fn expand_tilde(path: &str) -> PathBuf {
    if path == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from(path));
    }
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}

/// Compare two homes by canonical path, falling back to a lexical compare when
/// a path cannot be canonicalized (e.g. it does not exist yet).
fn homes_match(a: &Path, b: &Path) -> bool {
    let canonical =
        |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    canonical(a) == canonical(b)
}

/// Payload emitted to the frontend when a `codex://` link arrives. The frontend
/// uses `home_matches`/`home_exists` to decide between navigating, offering a
/// deliberate home switch, or showing an actionable error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HandoffOpen {
    /// `thread` or `new`.
    pub(crate) kind: String,
    pub(crate) thread_id: Option<String>,
    pub(crate) path: Option<String>,
    /// The resolved (tilde-expanded) requested home, if the link carried one.
    pub(crate) requested_home: Option<String>,
    pub(crate) label: Option<String>,
    /// The requested home equals the running home (or the link carried none).
    pub(crate) home_matches: bool,
    /// The requested home exists on disk (true when the link carried none).
    pub(crate) home_exists: bool,
}

fn resolve_open(link: &DeepLink, running_home: &Path) -> HandoffOpen {
    let (kind, thread_id) = match &link.kind {
        DeepLinkKind::Thread(id) => ("thread".to_string(), Some(id.clone())),
        DeepLinkKind::New => ("new".to_string(), None),
    };
    let (requested_home, home_matches, home_exists) = match link.codex_home.as_deref() {
        Some(raw) => {
            let expanded = expand_tilde(raw);
            let matches = homes_match(&expanded, running_home);
            let exists = expanded.is_dir();
            (Some(expanded.display().to_string()), matches, exists)
        }
        // No home param: treat as the running home.
        None => (None, true, true),
    };
    HandoffOpen {
        kind,
        thread_id,
        path: link.path.clone(),
        requested_home,
        label: link.label.clone(),
        home_matches,
        home_exists,
    }
}

/// Parse a received `codex://` URL, route it to the window whose home matches
/// (focusing it), else hand it to the main window resolved against its own
/// home so the frontend can offer a deliberate switch. Unknown/garbage URLs
/// are ignored (a stray click should not disturb the app).
pub(crate) fn handle_deep_link_url(app: &AppHandle, url: &str) {
    let Ok(link) = parse_deep_link(url) else {
        return;
    };
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    // A window already on the requested home takes the link directly.
    if let Some(raw) = link.codex_home.as_deref() {
        let requested = expand_tilde(raw);
        let matching = state
            .window_bindings()
            .into_iter()
            .find(|(label, key)| label != "quick" && homes_match(Path::new(key), &requested));
        if let Some((label, key)) = matching {
            if let Some(window) = app.get_webview_window(&label) {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
            let payload = resolve_open(&link, Path::new(&key));
            let _ = app.emit_to(&label, "handoff://open", payload);
            return;
        }
    }
    // No open window has this home: let the main window resolve it against
    // its own context and offer the switch flow.
    let running_home = state.ctx_for_label("main").runtime().codex_home;
    let payload = resolve_open(&link, &running_home);
    let _ = app.emit_to("main", "handoff://open", payload);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_reports_home_match_and_existence() {
        let link = DeepLink {
            kind: DeepLinkKind::Thread("t1".into()),
            path: Some("/repo".into()),
            codex_home: Some("/nope/.codex".into()),
            label: None,
        };
        let running = PathBuf::from("/other/.codex");
        let open = resolve_open(&link, &running);
        assert_eq!(open.kind, "thread");
        assert_eq!(open.thread_id.as_deref(), Some("t1"));
        assert!(!open.home_matches);
        // The nonexistent path resolves to no directory.
        assert!(!open.home_exists);

        // Same home path matches lexically even when it does not exist.
        let same = DeepLink {
            codex_home: Some("/other/.codex".into()),
            ..link.clone()
        };
        assert!(resolve_open(&same, &running).home_matches);
    }
    #[test]
    fn resolve_without_home_defaults_to_running() {
        let link = DeepLink {
            kind: DeepLinkKind::New,
            path: Some("/repo".into()),
            codex_home: None,
            label: None,
        };
        let open = resolve_open(&link, &PathBuf::from("/home/.codex"));
        assert!(open.home_matches);
        assert!(open.home_exists);
        assert_eq!(open.requested_home, None);
    }
}
