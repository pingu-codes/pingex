use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// How many previously-used Codex homes we remember for the launch picker.
const MAX_RECENT_HOMES: usize = 10;

/// A Codex home the user has booted into before, newest first.
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct RecentHome {
    pub(crate) path: String,
    pub(crate) last_used: i64,
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
    /// Path to the `claude` binary. `None` means "resolve bare `claude`".
    pub(crate) claude_binary: Option<String>,
    /// Claude Code config directory (`CLAUDE_CONFIG_DIR`). `None` means the
    /// CLI's own default, `~/.claude`.
    pub(crate) claude_config_dir: Option<String>,
    pub(crate) recent_homes: Vec<RecentHome>,
    /// Accelerator string (Tauri syntax) for the quick-chat global shortcut.
    /// `None` means "use the documented default".
    pub(crate) quick_shortcut: Option<String>,
    /// Whether new threads get a model-generated sidebar title. `None` means on.
    pub(crate) auto_name_threads: Option<bool>,
    /// Model slug for the naming turn. `None` leaves the model unset so the
    /// app-server picks its own default.
    pub(crate) auto_name_model: Option<String>,
    /// Whether new threads get the app's own `pingex_*` agent tools. `None`
    /// means off: this replaces how Codex would otherwise delegate, so it is
    /// opt-in rather than something a user discovers by surprise.
    pub(crate) app_subagents: Option<bool>,
    /// The most permissive sandbox a spawned agent may run under. A tool call
    /// may ask for less but never more.
    pub(crate) app_subagent_sandbox: Option<String>,
    /// How many spawned agents may run at once.
    pub(crate) app_subagent_max_concurrent: Option<u32>,
    /// How long a single agent may run before it is killed.
    pub(crate) app_subagent_timeout_seconds: Option<u64>,
}

/// Sandbox ceiling applied when the tool call does not ask for something
/// narrower. Deliberately not `danger-full-access`: an agent the user never
/// sees being prompted should not be able to escape the workspace.
pub(crate) const DEFAULT_AGENT_SANDBOX: &str = "workspace-write";
const DEFAULT_AGENT_MAX_CONCURRENT: usize = 4;
const DEFAULT_AGENT_TIMEOUT_SECONDS: u64 = 900;

/// Sandboxes a spawned agent may be given, narrowest first. `danger-full-access`
/// is absent on purpose — it is not reachable from a tool call.
pub(crate) const AGENT_SANDBOXES: [&str; 2] = ["read-only", "workspace-write"];

/// How app-owned subagents are configured, with the defaults already applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AgentSettings {
    pub(crate) enabled: bool,
    pub(crate) sandbox: String,
    pub(crate) max_concurrent: usize,
    pub(crate) timeout_seconds: u64,
}

pub(crate) fn read_agent_settings(path: &Path) -> AgentSettings {
    let overrides = read_overrides(path);
    AgentSettings {
        enabled: overrides.app_subagents.unwrap_or(false),
        sandbox: overrides
            .app_subagent_sandbox
            .map(|value| value.trim().to_string())
            .filter(|value| AGENT_SANDBOXES.contains(&value.as_str()))
            .unwrap_or_else(|| DEFAULT_AGENT_SANDBOX.to_string()),
        // Zero would mean "no agent can ever start", which is what the toggle
        // is for; treat it as unset rather than a deadlock.
        max_concurrent: overrides
            .app_subagent_max_concurrent
            .filter(|value| *value > 0)
            .map(|value| value as usize)
            .unwrap_or(DEFAULT_AGENT_MAX_CONCURRENT),
        timeout_seconds: overrides
            .app_subagent_timeout_seconds
            .filter(|value| *value > 0)
            .unwrap_or(DEFAULT_AGENT_TIMEOUT_SECONDS),
    }
}

/// Persist the agent settings while preserving the other overrides.
pub(crate) fn write_agent_settings(path: &Path, settings: &AgentSettings) -> Result<(), String> {
    if !AGENT_SANDBOXES.contains(&settings.sandbox.as_str()) {
        return Err(format!("Unknown agent sandbox: {}", settings.sandbox));
    }
    let mut overrides = read_overrides(path);
    overrides.app_subagents = Some(settings.enabled);
    overrides.app_subagent_sandbox = Some(settings.sandbox.clone());
    overrides.app_subagent_max_concurrent = Some(settings.max_concurrent.max(1) as u32);
    overrides.app_subagent_timeout_seconds = Some(settings.timeout_seconds.max(1));
    write_overrides(path, &overrides)
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
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create settings directory: {error}"))?;
    }
    let text = serde_json::to_string_pretty(overrides).map_err(|error| error.to_string())?;
    fs::write(path, text).map_err(|error| format!("Could not save settings: {error}"))
}

/// Remember `home` as the most recently used Codex home. Moves an existing
/// entry to the front (refreshing its timestamp) and caps the list length so
/// the launch picker only ever shows a handful of recents.
pub(crate) fn record_recent_home(path: &Path, home: &str, now: i64) -> Result<(), String> {
    let mut overrides = read_overrides(path);
    overrides.recent_homes.retain(|entry| entry.path != home);
    overrides.recent_homes.insert(
        0,
        RecentHome {
            path: home.to_string(),
            last_used: now,
        },
    );
    overrides.recent_homes.truncate(MAX_RECENT_HOMES);
    write_overrides(path, &overrides)
}

/// Drop `home` from the recents list. Only forgets the entry — the folder on
/// disk is untouched.
pub(crate) fn forget_recent_home(path: &Path, home: &str) -> Result<(), String> {
    let mut overrides = read_overrides(path);
    overrides.recent_homes.retain(|entry| entry.path != home);
    write_overrides(path, &overrides)
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
    let mut overrides = read_overrides(path);
    overrides.quick_shortcut = accelerator
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    write_overrides(path, &overrides)
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
            claude_binary: Some("/bin/claude".into()),
            claude_config_dir: Some("/tmp/claude-config".into()),
            recent_homes: vec![RecentHome {
                path: "/tmp/codex-home".into(),
                last_used: 42,
            }],
            quick_shortcut: None,
            auto_name_threads: None,
            auto_name_model: None,
            app_subagents: Some(true),
            app_subagent_sandbox: Some("read-only".into()),
            app_subagent_max_concurrent: Some(2),
            app_subagent_timeout_seconds: Some(60),
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
    fn agent_settings_default_to_off_inside_the_workspace() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        assert_eq!(
            read_agent_settings(&path),
            AgentSettings {
                enabled: false,
                sandbox: DEFAULT_AGENT_SANDBOX.into(),
                max_concurrent: 4,
                timeout_seconds: 900,
            }
        );
    }

    #[test]
    fn agent_settings_round_trip_and_preserve_other_overrides() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        write_quick_shortcut(&path, Some("Alt+Space".into())).unwrap();

        let settings = AgentSettings {
            enabled: true,
            sandbox: "read-only".into(),
            max_concurrent: 8,
            timeout_seconds: 120,
        };
        write_agent_settings(&path, &settings).unwrap();

        assert_eq!(read_agent_settings(&path), settings);
        assert_eq!(read_quick_shortcut(&path), "Alt+Space");
    }

    #[test]
    fn agent_settings_reject_an_unknown_sandbox_and_ignore_a_stored_one() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        assert!(write_agent_settings(
            &path,
            &AgentSettings {
                enabled: true,
                sandbox: "danger-full-access".into(),
                max_concurrent: 1,
                timeout_seconds: 60,
            }
        )
        .is_err());

        // A value that reached the file some other way still cannot widen the
        // sandbox: the read falls back to the default.
        write_overrides(
            &path,
            &RuntimeOverrides {
                app_subagent_sandbox: Some("danger-full-access".into()),
                ..RuntimeOverrides::default()
            },
        )
        .unwrap();
        assert_eq!(read_agent_settings(&path).sandbox, DEFAULT_AGENT_SANDBOX);
    }

    #[test]
    fn agent_limits_of_zero_fall_back_to_the_defaults() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        write_overrides(
            &path,
            &RuntimeOverrides {
                app_subagent_max_concurrent: Some(0),
                app_subagent_timeout_seconds: Some(0),
                ..RuntimeOverrides::default()
            },
        )
        .unwrap();

        let settings = read_agent_settings(&path);
        assert_eq!(settings.max_concurrent, 4);
        assert_eq!(settings.timeout_seconds, 900);
    }

    #[test]
    fn ignores_corrupt_settings_files() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        fs::write(&path, "not-json").unwrap();
        assert_eq!(read_overrides(&path), RuntimeOverrides::default());
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

        record_recent_home(&path, "/home/a", 10).unwrap();
        record_recent_home(&path, "/home/b", 20).unwrap();
        // Re-selecting an existing home moves it to the front and refreshes the time.
        record_recent_home(&path, "/home/a", 30).unwrap();

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
        record_recent_home(&path, "/home/a", 10).unwrap();
        record_recent_home(&path, "/home/b", 20).unwrap();

        forget_recent_home(&path, "/home/a").unwrap();

        let recents = read_overrides(&path).recent_homes;
        assert_eq!(recents.len(), 1);
        assert_eq!(recents[0].path, "/home/b");
        // Forgetting an unknown path is a no-op, not an error.
        forget_recent_home(&path, "/home/missing").unwrap();
        assert_eq!(read_overrides(&path).recent_homes.len(), 1);
    }

    #[test]
    fn recent_homes_are_capped() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        for index in 0..(MAX_RECENT_HOMES + 5) {
            record_recent_home(&path, &format!("/home/{index}"), index as i64).unwrap();
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
        record_recent_home(&path, "/home/a", 5).unwrap();
        let overrides = read_overrides(&path);
        assert_eq!(overrides.codex_home.as_deref(), Some("/pinned"));
        assert_eq!(overrides.codex_binary.as_deref(), Some("/bin/codex"));
        assert_eq!(overrides.recent_homes.len(), 1);
    }
}
