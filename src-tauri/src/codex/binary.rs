//! Locating the `codex` CLI.
//!
//! A bundled `.app` launched from Finder inherits a bare PATH
//! (`/usr/bin:/bin:/usr/sbin:/sbin`), so a Homebrew/npm/cargo install of Codex
//! is invisible to it and spawning bare `codex` fails with a raw
//! "No such file or directory (os error 2)". Everything that needs the binary
//! resolves it through here instead: PATH first, then the install locations a
//! GUI launch cannot see, so the app works without the user editing anything.

use serde::Serialize;
use std::path::{Path, PathBuf};

/// Install locations checked after PATH, because a Finder launch usually has
/// none of them. Home-relative entries start with `~/`.
const FALLBACK_DIRS: [&str; 8] = [
    "/opt/homebrew/bin",
    "/usr/local/bin",
    "/usr/bin",
    "~/.claude/local",
    "~/.local/bin",
    "~/.bun/bin",
    "~/.cargo/bin",
    "~/.volta/bin",
];

/// Expand a leading `~` so typed paths like `~/bin/codex` resolve.
pub(crate) fn expand_tilde(value: &str) -> PathBuf {
    let home = || dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    if value == "~" {
        home()
    } else if let Some(rest) = value.strip_prefix("~/") {
        home().join(rest)
    } else {
        PathBuf::from(value)
    }
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

/// The directories searched for a bare name: `$PATH` (when present) followed by
/// the fallbacks, deduped and tilde-expanded.
fn search_dirs(path_env: Option<&str>) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut push = |dir: PathBuf| {
        if !dir.as_os_str().is_empty() && !dirs.contains(&dir) {
            dirs.push(dir);
        }
    };
    for dir in path_env.unwrap_or_default().split(':') {
        push(expand_tilde(dir));
    }
    for dir in FALLBACK_DIRS {
        push(expand_tilde(dir));
    }
    dirs
}

/// Resolve `binary` to an executable file, or `None` when it cannot be found.
/// A value containing a separator (or a `~`) is treated as a path and checked
/// directly; a bare name is looked up in [`search_dirs`].
pub(crate) fn resolve_in(binary: &Path, path_env: Option<&str>) -> Option<PathBuf> {
    let raw = binary.to_string_lossy();
    if raw.is_empty() {
        return None;
    }
    if raw.contains('/') || raw.starts_with('~') {
        let candidate = expand_tilde(&raw);
        return is_executable(&candidate).then_some(candidate);
    }
    search_dirs(path_env)
        .into_iter()
        .map(|dir| dir.join(binary))
        .find(|candidate| is_executable(candidate))
}

pub fn resolve(binary: &Path) -> Option<PathBuf> {
    resolve_in(binary, std::env::var("PATH").ok().as_deref())
}

/// Why a binary could not be used, phrased for the picker and settings form.
pub fn missing_message(binary: &Path) -> String {
    let raw = binary.display().to_string();
    if raw.contains('/') || raw.starts_with('~') {
        format!("No executable Codex CLI at {raw}. Enter the full path to your codex binary.")
    } else {
        format!(
            "Could not find the Codex CLI ({raw}) on PATH or in the usual install locations. \
             Enter the full path to your codex binary (find it with `which codex`)."
        )
    }
}

/// Whether the configured Codex CLI can actually be spawned, and where it
/// resolved to. Shown by the launch picker before a home is opened.
#[derive(Debug, Clone, Serialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BinaryStatus {
    /// The configured value (an override, an env var, or bare `codex`).
    pub(crate) binary: String,
    /// Absolute path it resolved to, when found.
    pub(crate) resolved: Option<String>,
    pub(crate) found: bool,
    /// Guidance to show when `found` is false.
    pub(crate) message: Option<String>,
}

pub(crate) fn status(binary: &Path) -> BinaryStatus {
    match resolve(binary) {
        Some(resolved) => BinaryStatus {
            binary: binary.display().to_string(),
            resolved: Some(resolved.display().to_string()),
            found: true,
            message: None,
        },
        None => BinaryStatus {
            binary: binary.display().to_string(),
            resolved: None,
            found: false,
            message: Some(missing_message(binary)),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[cfg(unix)]
    fn write_executable(path: &Path) {
        use std::os::unix::fs::PermissionsExt;
        fs::write(path, "#!/bin/sh\n").unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn resolves_a_bare_name_from_the_given_path() {
        let dir = tempfile::tempdir().unwrap();
        let codex = dir.path().join("codex");
        write_executable(&codex);
        let path_env = format!("/nowhere:{}", dir.path().display());

        assert_eq!(resolve_in(Path::new("codex"), Some(&path_env)), Some(codex));
    }

    #[test]
    #[cfg(unix)]
    fn ignores_non_executable_and_missing_candidates() {
        let dir = tempfile::tempdir().unwrap();
        let plain = dir.path().join("codex");
        fs::write(&plain, "not executable").unwrap();
        let path_env = dir.path().display().to_string();

        // A bare name is not asserted here: the fallback dirs are searched too,
        // so a host with Codex installed would legitimately resolve one.
        assert_eq!(resolve_in(Path::new(&plain), Some(&path_env)), None);
        assert_eq!(
            resolve_in(&dir.path().join("absent"), Some(&path_env)),
            None
        );
    }

    #[test]
    #[cfg(unix)]
    fn accepts_an_explicit_path_regardless_of_path_env() {
        let dir = tempfile::tempdir().unwrap();
        let codex = dir.path().join("my-codex");
        write_executable(&codex);

        assert_eq!(resolve_in(&codex, Some("")), Some(codex.clone()));
        assert!(status(&codex).found);
    }

    #[test]
    fn empty_binaries_never_resolve() {
        assert_eq!(resolve_in(Path::new(""), Some("/usr/bin")), None);
    }

    #[test]
    fn falls_back_to_common_install_dirs_when_path_is_bare() {
        // The fallback list is what makes a Finder launch work; keep PATH out
        // of it so the search order is asserted, not the host's environment.
        let dirs = search_dirs(Some("/usr/bin"));
        assert!(dirs.contains(&PathBuf::from("/opt/homebrew/bin")));
        // The one non-`bin` entry is Claude Code's own install location.
        assert!(dirs
            .iter()
            .all(|dir| dir.ends_with("bin") || dir.ends_with(".claude/local")));
        // PATH entries come first and are not duplicated by the fallbacks.
        assert_eq!(dirs.first(), Some(&PathBuf::from("/usr/bin")));
        assert_eq!(
            dirs.iter()
                .filter(|dir| *dir == &PathBuf::from("/usr/bin"))
                .count(),
            1
        );
    }

    #[test]
    fn missing_message_distinguishes_a_path_from_a_bare_name() {
        assert!(missing_message(Path::new("codex")).contains("on PATH"));
        assert!(missing_message(Path::new("/opt/codex")).contains("/opt/codex"));
    }
}
