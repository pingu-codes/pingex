//! Typed read/write access to a whitelisted subset of `config.toml`.
//!
//! Only obviously-safe scalar keys are exposed. Writes preserve the file's
//! existing contents, comments, and formatting via `toml_edit`, and distinguish
//! an explicit unset (remove the key so Codex inherits its default) from a set
//! value. Secrets — including `mcp_servers.*.env` blocks — are never read or
//! written here.

use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

/// The kind of control a setting maps to in the UI.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum SettingKind {
    /// One of a fixed set of `options`.
    Enum,
    /// A free-form string (e.g. a model slug).
    String,
    /// A boolean toggle.
    Bool,
}

/// Where a setting's current value comes from. `env` is reserved for
/// environment/managed overrides we cannot edit from here.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum SettingSource {
    /// No value in `config.toml`; Codex applies its built-in default.
    Default,
    /// Explicitly set in `config.toml`.
    Config,
}

/// Static metadata for one supported key.
struct KeySpec {
    key: &'static str,
    /// Section the control lives under in the settings UI.
    section: &'static str,
    label: &'static str,
    kind: SettingKind,
    /// Codex's built-in default, rendered as a string, when nothing is set.
    default: Option<&'static str>,
    /// Allowed values for `Enum` kinds.
    options: &'static [&'static str],
    /// A config.toml change here needs a restart to take effect; otherwise it
    /// applies to the next thread the app-server starts.
    restart_required: bool,
}

/// The whitelist. Every writable key must appear here; anything else is
/// rejected, so untrusted input can never reach an arbitrary config key.
const KEYS: &[KeySpec] = &[
    KeySpec {
        key: "model",
        section: "agent",
        label: "Model",
        kind: SettingKind::String,
        default: None,
        options: &[],
        restart_required: false,
    },
    KeySpec {
        key: "model_reasoning_effort",
        section: "agent",
        label: "Reasoning effort",
        kind: SettingKind::Enum,
        default: Some("medium"),
        options: &["minimal", "low", "medium", "high", "xhigh"],
        restart_required: false,
    },
    KeySpec {
        key: "approval_policy",
        section: "agent",
        label: "Approval policy",
        kind: SettingKind::Enum,
        default: Some("on-request"),
        options: &["untrusted", "on-failure", "on-request", "never"],
        restart_required: false,
    },
    KeySpec {
        key: "sandbox_mode",
        section: "agent",
        label: "Sandbox mode",
        kind: SettingKind::Enum,
        default: Some("read-only"),
        options: &["read-only", "workspace-write", "danger-full-access"],
        restart_required: false,
    },
    KeySpec {
        key: "model_reasoning_summary",
        section: "modelFeatures",
        label: "Reasoning summaries",
        kind: SettingKind::Enum,
        default: Some("auto"),
        options: &["auto", "concise", "detailed", "none"],
        restart_required: false,
    },
    KeySpec {
        key: "hide_agent_reasoning",
        section: "modelFeatures",
        label: "Hide reasoning stream",
        kind: SettingKind::Bool,
        default: Some("false"),
        options: &[],
        restart_required: false,
    },
    KeySpec {
        key: "file_opener",
        section: "coding",
        label: "File opener scheme",
        kind: SettingKind::Enum,
        default: Some("vscode"),
        options: &["vscode", "vscode-insiders", "windsurf", "cursor", "none"],
        restart_required: false,
    },
];

/// One setting reported to the frontend.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConfigSetting {
    key: String,
    section: String,
    label: String,
    kind: SettingKind,
    /// The effective value: the config value if set, else the default.
    value: Option<String>,
    /// Codex's built-in default (shown when the value is inherited).
    default: Option<String>,
    source: SettingSource,
    options: Vec<String>,
    restart_required: bool,
}

pub(crate) fn config_path(codex_home: &Path) -> PathBuf {
    codex_home.join("config.toml")
}

/// Parse a home's `config.toml` for editing, preserving comments and layout.
/// A missing file yields an empty document — "no config yet", not an error.
pub(crate) fn read_doc(codex_home: &Path) -> Result<toml_edit::DocumentMut, String> {
    let text = match fs::read_to_string(config_path(codex_home)) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(format!("Could not read config.toml: {error}")),
    };
    text.parse::<toml_edit::DocumentMut>()
        .map_err(|error| format!("config.toml is not valid TOML: {error}"))
}

/// Write an edited document back, creating the home directory if needed.
pub(crate) fn write_doc(
    codex_home: &Path,
    document: &toml_edit::DocumentMut,
) -> Result<(), String> {
    let path = config_path(codex_home);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create Codex home: {error}"))?;
    }
    fs::write(&path, document.to_string())
        .map_err(|error| format!("Could not write config.toml: {error}"))
}

/// Render a parsed value as the string we show/store. Only scalar types are
/// supported; anything else (tables, arrays) is treated as unset.
fn scalar_string(value: &toml::Value) -> Option<String> {
    match value {
        toml::Value::String(text) => Some(text.clone()),
        toml::Value::Boolean(flag) => Some(flag.to_string()),
        toml::Value::Integer(number) => Some(number.to_string()),
        toml::Value::Float(number) => Some(number.to_string()),
        _ => None,
    }
}

fn setting_for(spec: &KeySpec, config: Option<&toml::Table>) -> ConfigSetting {
    let stored = config
        .and_then(|table| table.get(spec.key))
        .and_then(scalar_string);
    let (value, source) = match stored {
        Some(value) => (Some(value), SettingSource::Config),
        None => (spec.default.map(str::to_string), SettingSource::Default),
    };
    ConfigSetting {
        key: spec.key.to_string(),
        section: spec.section.to_string(),
        label: spec.label.to_string(),
        kind: spec.kind,
        value,
        default: spec.default.map(str::to_string),
        source,
        options: spec
            .options
            .iter()
            .map(|option| option.to_string())
            .collect(),
        restart_required: spec.restart_required,
    }
}

/// Read every whitelisted setting from `config.toml`, filling in defaults for
/// keys that are absent. A missing or unparseable file yields all-default
/// settings rather than an error.
pub(crate) fn read_config_settings(codex_home: &Path) -> Vec<ConfigSetting> {
    let config = fs::read_to_string(config_path(codex_home))
        .ok()
        .and_then(|text| text.parse::<toml::Table>().ok());
    KEYS.iter()
        .map(|spec| setting_for(spec, config.as_ref()))
        .collect()
}

fn spec_for(key: &str) -> Option<&'static KeySpec> {
    KEYS.iter().find(|spec| spec.key == key)
}

/// Coerce and validate a requested string value for `spec`, returning the
/// `toml_edit` value to store.
fn edit_value(spec: &KeySpec, value: &str) -> Result<toml_edit::Value, String> {
    match spec.kind {
        SettingKind::Enum => {
            if spec.options.contains(&value) {
                Ok(value.into())
            } else {
                Err(format!(
                    "{value:?} is not a valid value for {} (expected one of {:?})",
                    spec.key, spec.options
                ))
            }
        }
        SettingKind::Bool => match value {
            "true" => Ok(true.into()),
            "false" => Ok(false.into()),
            _ => Err(format!("{} expects true or false", spec.key)),
        },
        SettingKind::String => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                Err(format!("{} cannot be empty", spec.key))
            } else {
                Ok(trimmed.into())
            }
        }
    }
}

/// Set (or, when `unset` is true, remove) a single whitelisted key in
/// `config.toml`, preserving the rest of the file. Returns the refreshed list of
/// all settings.
pub(crate) fn write_config_setting(
    codex_home: &Path,
    key: &str,
    value: Option<&str>,
    unset: bool,
) -> Result<Vec<ConfigSetting>, String> {
    let spec = spec_for(key).ok_or_else(|| format!("{key} is not an editable setting"))?;

    let mut document = read_doc(codex_home)?;

    if unset {
        document.as_table_mut().remove(spec.key);
    } else {
        let value = value.ok_or_else(|| "No value supplied".to_string())?;
        document[spec.key] = toml_edit::value(edit_value(spec, value)?);
    }

    write_doc(codex_home, &document)?;
    Ok(read_config_settings(codex_home))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find<'a>(settings: &'a [ConfigSetting], key: &str) -> &'a ConfigSetting {
        settings.iter().find(|setting| setting.key == key).unwrap()
    }

    #[test]
    fn absent_config_reports_defaults() {
        let directory = tempfile::tempdir().unwrap();
        let settings = read_config_settings(directory.path());
        let effort = find(&settings, "model_reasoning_effort");
        assert_eq!(effort.source, SettingSource::Default);
        assert_eq!(effort.value.as_deref(), Some("medium"));
        // A key with no built-in default reports no value.
        assert_eq!(find(&settings, "model").value, None);
    }

    #[test]
    fn detects_source_and_value_from_config() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(
            directory.path().join("config.toml"),
            "model = \"gpt-5.6-luna\"\napproval_policy = \"never\"\n",
        )
        .unwrap();
        let settings = read_config_settings(directory.path());

        let model = find(&settings, "model");
        assert_eq!(model.source, SettingSource::Config);
        assert_eq!(model.value.as_deref(), Some("gpt-5.6-luna"));

        let approval = find(&settings, "approval_policy");
        assert_eq!(approval.source, SettingSource::Config);
        assert_eq!(approval.value.as_deref(), Some("never"));

        // Unmentioned key stays default.
        assert_eq!(
            find(&settings, "sandbox_mode").source,
            SettingSource::Default
        );
    }

    #[test]
    fn set_and_unset_round_trip() {
        let directory = tempfile::tempdir().unwrap();

        let settings = write_config_setting(
            directory.path(),
            "sandbox_mode",
            Some("workspace-write"),
            false,
        )
        .unwrap();
        assert_eq!(
            find(&settings, "sandbox_mode").value.as_deref(),
            Some("workspace-write")
        );
        assert_eq!(
            find(&settings, "sandbox_mode").source,
            SettingSource::Config
        );

        // Re-reading from disk sees the written value.
        let reread = read_config_settings(directory.path());
        assert_eq!(
            find(&reread, "sandbox_mode").value.as_deref(),
            Some("workspace-write")
        );

        // Unset removes the key, reverting to the inherited default.
        let after_unset =
            write_config_setting(directory.path(), "sandbox_mode", None, true).unwrap();
        let sandbox = find(&after_unset, "sandbox_mode");
        assert_eq!(sandbox.source, SettingSource::Default);
        assert_eq!(sandbox.value.as_deref(), Some("read-only"));
    }

    #[test]
    fn preserves_comments_and_other_keys() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        fs::write(
            &path,
            "# my config\nmodel = \"gpt-5.6-luna\" # pinned\n\n[mcp_servers.alpha]\ncommand = \"run\"\n",
        )
        .unwrap();

        write_config_setting(
            directory.path(),
            "approval_policy",
            Some("on-request"),
            false,
        )
        .unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("# my config"));
        assert!(text.contains("# pinned"));
        assert!(text.contains("[mcp_servers.alpha]"));
        assert!(text.contains("approval_policy = \"on-request\""));
    }

    #[test]
    fn writes_bool_as_toml_boolean_not_string() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        write_config_setting(
            directory.path(),
            "hide_agent_reasoning",
            Some("true"),
            false,
        )
        .unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("hide_agent_reasoning = true"));
        assert_eq!(
            find(
                &read_config_settings(directory.path()),
                "hide_agent_reasoning"
            )
            .value
            .as_deref(),
            Some("true")
        );
    }

    #[test]
    fn rejects_unknown_keys_and_invalid_enum_values() {
        let directory = tempfile::tempdir().unwrap();
        assert!(write_config_setting(directory.path(), "danger_key", Some("x"), false).is_err());
        assert!(
            write_config_setting(directory.path(), "sandbox_mode", Some("bogus"), false).is_err()
        );
        // Nothing should have been written for a rejected request.
        assert!(!directory.path().join("config.toml").exists());
    }

    #[test]
    fn ignores_non_scalar_stored_values() {
        let directory = tempfile::tempdir().unwrap();
        // `model` declared as a table is not a scalar; treat it as unset.
        fs::write(directory.path().join("config.toml"), "[model]\nfoo = 1\n").unwrap();
        let settings = read_config_settings(directory.path());
        assert_eq!(find(&settings, "model").value, None);
        assert_eq!(find(&settings, "model").source, SettingSource::Default);
    }
}
