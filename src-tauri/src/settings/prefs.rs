use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::util::host::Host;

/// How many previously-used Codex homes we remember for the launch picker.
const MAX_RECENT_HOMES: usize = 10;

/// A Codex home the user has booted into before, newest first.
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct RecentHome {
    pub(crate) path: String,
    pub(crate) last_used: i64,
    /// Where the home lives. Absent in files written before hosts existed,
    /// which were all native.
    #[serde(default)]
    pub(crate) host: Host,
}

/// Default global shortcut that toggles the quick-chat window. Documented so
/// users know what to press before customising it in Settings.
pub(crate) const DEFAULT_QUICK_SHORTCUT: &str = "CmdOrCtrl+Shift+Space";

/// User-editable runtime overrides saved from the settings dialog. They live
/// outside CODEX_HOME because they may themselves relocate CODEX_HOME.
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct RuntimeOverrides {
    pub(crate) codex_home: Option<String>,
    pub(crate) codex_binary: Option<String>,
    /// Where `codex_home` and `codex_binary` live when the app launches
    /// without an explicit home. `None` means native.
    pub(crate) codex_host: Option<Host>,
    /// Path to the `claude` binary. `None` means "resolve bare `claude`".
    pub(crate) claude_binary: Option<String>,
    /// Claude Code config directory (`CLAUDE_CONFIG_DIR`). `None` means the
    /// CLI's own default, `~/.claude`.
    pub(crate) claude_config_dir: Option<String>,
    /// Where `claude` runs and where its config directory lives. `None`
    /// means native.
    pub(crate) claude_host: Option<Host>,
    pub(crate) recent_homes: Vec<RecentHome>,
    /// Accelerator string (Tauri syntax) for the quick-chat global shortcut.
    /// `None` means "use the documented default".
    pub(crate) quick_shortcut: Option<String>,
    /// Whether new threads get a model-generated sidebar title. `None` means on.
    pub(crate) auto_name_threads: Option<bool>,
    /// Model slug for the naming turn. `None` leaves the model unset so the
    /// app-server picks its own default.
    pub(crate) auto_name_model: Option<String>,
}

/// How thread auto-naming is configured, with the defaults already applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AutoNaming {
    pub(crate) enabled: bool,
    pub(crate) model: Option<String>,
}

pub(crate) fn read_auto_naming(path: &Path) -> AutoNaming {
    let overrides = read_overrides(path);
    AutoNaming {
        enabled: overrides.auto_name_threads.unwrap_or(true),
        model: overrides
            .auto_name_model
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
    }
}

pub(crate) fn settings_path() -> PathBuf {
    pingex_settings_path()
}

fn pingex_settings_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("pingex")
        .join("settings.json")
}

fn legacy_settings_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("pingu-codex")
        .join("settings.json")
}

/// Copy valid settings from Pingu Codex on Pingex's first run. The old file is
/// deliberately retained so the previous app remains usable as a rollback.
pub(crate) fn migrate_legacy_settings() -> Result<bool, String> {
    migrate_legacy_settings_at(&legacy_settings_path(), &pingex_settings_path())
}

fn migrate_legacy_settings_at(source: &Path, destination: &Path) -> Result<bool, String> {
    if !source.exists() || destination.exists() {
        return Ok(false);
    }
    if serde_json::from_str::<RuntimeOverrides>(&fs::read_to_string(source).unwrap_or_default())
        .is_err()
    {
        return Ok(false);
    }
    crate::util::migration::copy_file_if_missing(source, destination)
}

pub(crate) fn read_overrides(path: &Path) -> RuntimeOverrides {
    let Ok(text) = fs::read_to_string(path) else {
        return RuntimeOverrides::default();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

pub(crate) fn write_overrides(path: &Path, overrides: &RuntimeOverrides) -> Result<(), String> {
    let text = serde_json::to_string_pretty(overrides).map_err(|error| error.to_string())?;
    crate::util::fs::write_atomic(path, text.as_bytes())
        .map_err(|error| format!("Could not save settings: {error}"))
}

/// Read, change and write back the settings file under one process-wide lock,
/// so concurrent saves from two windows cannot drop each other's change.
/// Unlike [`read_overrides`], an unreadable file is an error here: starting
/// from defaults would silently erase every saved setting on the next save.
pub(crate) fn update_overrides<T>(
    path: &Path,
    update: impl FnOnce(&mut RuntimeOverrides) -> Result<T, String>,
) -> Result<T, String> {
    static WRITE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = WRITE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut overrides = match fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).map_err(|error| {
            format!(
                "{} is not valid settings JSON ({error}). Fix or delete it, then try again.",
                path.display()
            )
        })?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => RuntimeOverrides::default(),
        Err(error) => return Err(format!("Could not read settings: {error}")),
    };
    let result = update(&mut overrides)?;
    write_overrides(path, &overrides)?;
    Ok(result)
}

/// Remember `home` on `host` as the most recently used Codex home. Moves an
/// existing entry to the front (refreshing its timestamp) and caps the list
/// length so the launch picker only ever shows a handful of recents. The same
/// path on two hosts is two homes.
pub(crate) fn record_recent_home(
    path: &Path,
    home: &str,
    host: &Host,
    now: i64,
) -> Result<(), String> {
    update_overrides(path, |overrides| {
        overrides
            .recent_homes
            .retain(|entry| !(entry.path == home && &entry.host == host));
        overrides.recent_homes.insert(
            0,
            RecentHome {
                path: home.to_string(),
                last_used: now,
                host: host.clone(),
            },
        );
        overrides.recent_homes.truncate(MAX_RECENT_HOMES);
        Ok(())
    })
}

/// Drop `home` on `host` from the recents list. Only forgets the entry — the
/// folder on disk is untouched.
pub(crate) fn forget_recent_home(path: &Path, home: &str, host: &Host) -> Result<(), String> {
    update_overrides(path, |overrides| {
        overrides
            .recent_homes
            .retain(|entry| !(entry.path == home && &entry.host == host));
        Ok(())
    })
}

/// The persisted quick-chat shortcut, or the documented default when unset.
pub(crate) fn read_quick_shortcut(path: &Path) -> String {
    read_overrides(path)
        .quick_shortcut
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_QUICK_SHORTCUT.to_string())
}

/// Persist the quick-chat shortcut while preserving the other overrides.
/// `None` clears it back to the default.
pub(crate) fn write_quick_shortcut(path: &Path, accelerator: Option<String>) -> Result<(), String> {
    update_overrides(path, |overrides| {
        overrides.quick_shortcut = accelerator
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_overrides_and_tolerates_missing_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("nested").join("settings.json");
        assert_eq!(read_overrides(&path), RuntimeOverrides::default());

        let overrides = RuntimeOverrides {
            codex_home: Some("/tmp/codex-home".into()),
            codex_binary: None,
            codex_host: Some(Host::wsl("Ubuntu")),
            claude_binary: Some("/bin/claude".into()),
            claude_config_dir: Some("/tmp/claude-config".into()),
            claude_host: None,
            recent_homes: vec![RecentHome {
                path: "/tmp/codex-home".into(),
                last_used: 42,
                host: Host::wsl("Ubuntu"),
            }],
            quick_shortcut: None,
            auto_name_threads: None,
            auto_name_model: None,
        };
        write_overrides(&path, &overrides).unwrap();
        assert_eq!(read_overrides(&path), overrides);
    }

    #[test]
    fn quick_shortcut_falls_back_to_default_and_round_trips() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        assert_eq!(read_quick_shortcut(&path), DEFAULT_QUICK_SHORTCUT);

        write_quick_shortcut(&path, Some("CmdOrCtrl+Shift+K".into())).unwrap();
        assert_eq!(read_quick_shortcut(&path), "CmdOrCtrl+Shift+K");
    }

    #[test]
    fn migrates_valid_legacy_settings_once_without_removing_them() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("pingu/settings.json");
        let destination = directory.path().join("pingex/settings.json");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(&source, r#"{"quickShortcut":"CmdOrCtrl+Shift+K"}"#).unwrap();
        assert!(migrate_legacy_settings_at(&source, &destination).unwrap());
        assert_eq!(read_quick_shortcut(&destination), "CmdOrCtrl+Shift+K");
        assert!(source.exists());
        assert!(!migrate_legacy_settings_at(&source, &destination).unwrap());

        let malformed = directory.path().join("bad.json");
        let missing = directory.path().join("missing.json");
        fs::write(&malformed, "not json").unwrap();
        assert!(!migrate_legacy_settings_at(&malformed, &missing).unwrap());
        assert!(!missing.exists());
    }

    #[test]
    fn writing_quick_shortcut_preserves_other_overrides() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        write_overrides(
            &path,
            &RuntimeOverrides {
                codex_home: Some("/tmp/home".into()),
                codex_binary: Some("/bin/codex".into()),
                ..RuntimeOverrides::default()
            },
        )
        .unwrap();

        write_quick_shortcut(&path, Some("Alt+Space".into())).unwrap();
        let stored = read_overrides(&path);
        assert_eq!(stored.codex_home.as_deref(), Some("/tmp/home"));
        assert_eq!(stored.codex_binary.as_deref(), Some("/bin/codex"));
        assert_eq!(stored.quick_shortcut.as_deref(), Some("Alt+Space"));

        // Clearing resets to the default without disturbing the rest.
        write_quick_shortcut(&path, None).unwrap();
        assert_eq!(read_quick_shortcut(&path), DEFAULT_QUICK_SHORTCUT);
        assert_eq!(
            read_overrides(&path).codex_home.as_deref(),
            Some("/tmp/home")
        );
    }

    #[test]
    fn auto_naming_defaults_to_on_with_no_model_override() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        assert_eq!(
            read_auto_naming(&path),
            AutoNaming {
                enabled: true,
                model: None
            }
        );

        write_overrides(
            &path,
            &RuntimeOverrides {
                auto_name_threads: Some(false),
                auto_name_model: Some("  ".into()),
                ..RuntimeOverrides::default()
            },
        )
        .unwrap();
        assert_eq!(
            read_auto_naming(&path),
            AutoNaming {
                enabled: false,
                // A blank slug is not a model: it falls back to the default.
                model: None
            }
        );
    }

    #[test]
    fn ignores_corrupt_settings_files() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        fs::write(&path, "not-json").unwrap();
        assert_eq!(read_overrides(&path), RuntimeOverrides::default());
    }

    #[test]
    fn saving_over_a_corrupt_settings_file_refuses_instead_of_resetting() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        fs::write(&path, r#"{"codexHome": "/mine", "#).unwrap();
        let error = write_quick_shortcut(&path, Some("CmdOrCtrl+K".into())).unwrap_err();
        assert!(error.contains("not valid settings JSON"), "{error}");
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            r#"{"codexHome": "/mine", "#
        );
    }

    #[test]
    fn reads_legacy_settings_without_recent_homes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        fs::write(&path, r#"{"codexHome":"/legacy"}"#).unwrap();
        let overrides = read_overrides(&path);
        assert_eq!(overrides.codex_home.as_deref(), Some("/legacy"));
        assert!(overrides.recent_homes.is_empty());
    }

    #[test]
    fn records_recent_homes_newest_first_without_duplicates() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");

        record_recent_home(&path, "/home/a", &Host::Native, 10).unwrap();
        record_recent_home(&path, "/home/b", &Host::Native, 20).unwrap();
        // Re-selecting an existing home moves it to the front and refreshes the time.
        record_recent_home(&path, "/home/a", &Host::Native, 30).unwrap();

        let recents = read_overrides(&path).recent_homes;
        assert_eq!(
            recents
                .iter()
                .map(|home| home.path.as_str())
                .collect::<Vec<_>>(),
            vec!["/home/a", "/home/b"]
        );
        assert_eq!(recents[0].last_used, 30);
    }

    #[test]
    fn forgets_a_recent_home() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        record_recent_home(&path, "/home/a", &Host::Native, 10).unwrap();
        record_recent_home(&path, "/home/b", &Host::Native, 20).unwrap();

        forget_recent_home(&path, "/home/a", &Host::Native).unwrap();

        let recents = read_overrides(&path).recent_homes;
        assert_eq!(recents.len(), 1);
        assert_eq!(recents[0].path, "/home/b");
        // Forgetting an unknown path is a no-op, not an error.
        forget_recent_home(&path, "/home/missing", &Host::Native).unwrap();
        assert_eq!(read_overrides(&path).recent_homes.len(), 1);
    }

    #[test]
    fn the_same_path_on_two_hosts_is_two_recent_homes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let wsl = Host::wsl("Ubuntu");
        record_recent_home(&path, "/home/u/.codex", &Host::Native, 10).unwrap();
        record_recent_home(&path, "/home/u/.codex", &wsl, 20).unwrap();
        assert_eq!(read_overrides(&path).recent_homes.len(), 2);

        forget_recent_home(&path, "/home/u/.codex", &wsl).unwrap();
        let recents = read_overrides(&path).recent_homes;
        assert_eq!(recents.len(), 1);
        assert_eq!(recents[0].host, Host::Native);
    }

    #[test]
    fn recent_homes_without_a_host_are_native() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        fs::write(&path, r#"{"recentHomes":[{"path":"/old","lastUsed":1}]}"#).unwrap();
        assert_eq!(read_overrides(&path).recent_homes[0].host, Host::Native);
    }

    #[test]
    fn recent_homes_are_capped() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        for index in 0..(MAX_RECENT_HOMES + 5) {
            record_recent_home(
                &path,
                &format!("/home/{index}"),
                &Host::Native,
                index as i64,
            )
            .unwrap();
        }
        assert_eq!(read_overrides(&path).recent_homes.len(), MAX_RECENT_HOMES);
    }

    #[test]
    fn recording_recent_homes_preserves_overrides() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        write_overrides(
            &path,
            &RuntimeOverrides {
                codex_home: Some("/pinned".into()),
                codex_binary: Some("/bin/codex".into()),
                ..RuntimeOverrides::default()
            },
        )
        .unwrap();
        record_recent_home(&path, "/home/a", &Host::Native, 5).unwrap();
        let overrides = read_overrides(&path);
        assert_eq!(overrides.codex_home.as_deref(), Some("/pinned"));
        assert_eq!(overrides.codex_binary.as_deref(), Some("/bin/codex"));
        assert_eq!(overrides.recent_homes.len(), 1);
    }
}
