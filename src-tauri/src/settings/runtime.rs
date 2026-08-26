//! Where the app points: which Codex home and which Codex CLI.
//!
//! The home can come from four places, in descending precedence: a
//! `--codex-home` argument, `CODEX_HOME`, a saved override, or the `~/.codex`
//! default. Only the first two count as *explicit* — anything else means the
//! launch picker is shown before booting.

use serde::Serialize;
use std::path::{Path, PathBuf};

use super::prefs::{self, RuntimeOverrides};
use crate::codex::binary;
use crate::{HomeContext, RuntimeConfig};

/// The runtime identity as the settings dialog sees it: what is active now,
/// what is saved as an override, and whether the two have diverged.
#[derive(Debug, Serialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuntimeSettings {
    codex_home: String,
    codex_binary: String,
    override_codex_home: Option<String>,
    override_codex_binary: Option<String>,
    settings_path: String,
    restart_required: bool,
}

/// A saved home the launch picker offers.
#[derive(Debug, Serialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecentHomeInfo {
    path: String,
    last_used: i64,
    /// Whether the folder still exists on disk (stale entries are dimmed, not hidden).
    exists: bool,
}

/// State the frontend reads once at startup to decide whether to show the home
/// picker (non-explicit launch) or boot straight into the active home.
#[derive(Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LaunchState {
    codex_home: String,
    /// Canonical form of `codex_home` — the key the backend tags events with,
    /// so the frontend filters by exact equality.
    home_key: String,
    codex_binary: String,
    default_home: String,
    /// The home came from `--codex-home`/`CODEX_HOME`; boot without a picker.
    explicit: bool,
    /// Show the picker before booting (inverse of `explicit`).
    needs_picker: bool,
    recent_homes: Vec<RecentHomeInfo>,
    /// Whether the configured Codex CLI can be spawned. A home cannot be opened
    /// until it can, so the picker offers a path input instead of booting into
    /// a raw spawn error.
    codex_binary_status: binary::BinaryStatus,
}

pub(crate) fn runtime_settings(
    runtime: &RuntimeConfig,
    overrides: &RuntimeOverrides,
) -> RuntimeSettings {
    let restart_required = overrides
        .codex_home
        .as_ref()
        .is_some_and(|home| Path::new(home) != runtime.codex_home)
        || overrides
            .codex_binary
            .as_ref()
            .is_some_and(|binary| Path::new(binary) != runtime.codex_binary);
    RuntimeSettings {
        codex_home: runtime.codex_home.display().to_string(),
        codex_binary: runtime.codex_binary.display().to_string(),
        override_codex_home: overrides.codex_home.clone(),
        override_codex_binary: overrides.codex_binary.clone(),
        settings_path: prefs::settings_path().display().to_string(),
        restart_required,
    }
}

/// The launch state one window sees: the home its context points at, and
/// whether it still needs to pick one (`explicit` means "bound already" for
/// windows beyond the first).
pub(crate) fn launch_state(ctx: &HomeContext, explicit: bool) -> LaunchState {
    let runtime = ctx.runtime();
    let overrides = prefs::read_overrides(&prefs::settings_path());
    let recent_homes = overrides
        .recent_homes
        .into_iter()
        .map(|home| RecentHomeInfo {
            exists: Path::new(&home.path).is_dir(),
            path: home.path,
            last_used: home.last_used,
        })
        .collect();
    LaunchState {
        codex_home: runtime.codex_home.display().to_string(),
        home_key: ctx.home_key.clone(),
        codex_binary: runtime.codex_binary.display().to_string(),
        default_home: default_codex_home().display().to_string(),
        explicit,
        needs_picker: !explicit,
        recent_homes,
        codex_binary_status: binary::status(&runtime.codex_binary),
    }
}

/// Trim a user-supplied override, treating whitespace-only input as "unset".
pub(crate) fn normalize_override(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// The `~/.codex` fallback used when no home is configured anywhere.
pub(crate) fn default_codex_home() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".codex")
}

/// Resolve the runtime and report whether the home was *explicitly* requested
/// via a CLI arg or environment variable (as opposed to a saved override or the
/// default). An explicit home boots straight away; otherwise the launch picker
/// is shown first.
fn resolve_runtime(
    cli_home: Option<PathBuf>,
    env_home: Option<PathBuf>,
    override_home: Option<&str>,
    env_binary: Option<PathBuf>,
    override_binary: Option<&str>,
) -> (RuntimeConfig, bool) {
    let explicit_home = cli_home.is_some() || env_home.is_some();
    let codex_home = cli_home
        .or(env_home)
        .or_else(|| override_home.map(PathBuf::from))
        .unwrap_or_else(default_codex_home);
    let codex_binary = env_binary
        .or_else(|| override_binary.map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("codex"));
    (
        RuntimeConfig {
            codex_home,
            codex_binary,
        },
        explicit_home,
    )
}

/// Read the runtime out of the process environment at startup.
pub(crate) fn parse_runtime() -> (RuntimeConfig, bool) {
    // Best-effort: an unreadable legacy preference file must not stop startup.
    let _ = prefs::migrate_legacy_settings();
    let mut args = std::env::args().skip(1);
    let mut cli_home = None;
    while let Some(arg) = args.next() {
        if arg == "--codex-home" {
            cli_home = args.next().map(PathBuf::from);
        }
    }
    let overrides = prefs::read_overrides(&prefs::settings_path());
    let env_binary = std::env::var_os("PINGEX_CODEX_CLI_PATH")
        .or_else(|| std::env::var_os("PINGU_CODEX_CLI_PATH"))
        .or_else(|| std::env::var_os("CODEX_CLI_PATH"))
        .map(PathBuf::from);
    resolve_runtime(
        cli_home,
        std::env::var_os("CODEX_HOME").map(PathBuf::from),
        overrides.codex_home.as_deref(),
        env_binary,
        overrides.codex_binary.as_deref(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_sources_follow_documented_precedence() {
        let (runtime, explicit) = resolve_runtime(
            Some("/cli-home".into()),
            Some("/env-home".into()),
            Some("/override-home"),
            Some("/env-codex".into()),
            Some("/override-codex"),
        );
        assert_eq!(runtime.codex_home, PathBuf::from("/cli-home"));
        assert_eq!(runtime.codex_binary, PathBuf::from("/env-codex"));
        assert!(explicit);

        let (environment, explicit) = resolve_runtime(
            None,
            Some("/env-home".into()),
            Some("/override-home"),
            None,
            Some("/override-codex"),
        );
        assert_eq!(environment.codex_home, PathBuf::from("/env-home"));
        assert_eq!(environment.codex_binary, PathBuf::from("/override-codex"));
        assert!(explicit);

        let (overrides, explicit) = resolve_runtime(
            None,
            None,
            Some("/override-home"),
            None,
            Some("/override-codex"),
        );
        assert_eq!(overrides.codex_home, PathBuf::from("/override-home"));
        assert_eq!(overrides.codex_binary, PathBuf::from("/override-codex"));
        // A saved override is not "explicit" — the picker should still appear.
        assert!(!explicit);
    }

    #[test]
    fn restart_is_required_only_for_changed_overrides() {
        let runtime = RuntimeConfig {
            codex_home: "/home".into(),
            codex_binary: "/bin/codex".into(),
        };
        let unchanged = RuntimeOverrides {
            codex_home: Some("/home".into()),
            codex_binary: Some("/bin/codex".into()),
            ..Default::default()
        };
        assert!(!runtime_settings(&runtime, &unchanged).restart_required);

        let changed = RuntimeOverrides {
            codex_home: Some("/other".into()),
            codex_binary: None,
            ..Default::default()
        };
        assert!(runtime_settings(&runtime, &changed).restart_required);
    }

    #[test]
    fn empty_runtime_overrides_are_removed() {
        assert_eq!(normalize_override(Some("  ".into())), None);
        assert_eq!(
            normalize_override(Some(" /bin/codex ".into())),
            Some("/bin/codex".into())
        );
    }
}
