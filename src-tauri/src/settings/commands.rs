//! Commands behind the settings dialog and the launch picker.
//!
//! Changing the home or the CLI takes effect immediately rather than on next
//! launch: the live runtime is swapped and the app-server child is dropped so
//! the next request respawns against the new target.

use std::path::PathBuf;
use tauri::State;

use super::runtime::{
    launch_state, normalize_override, runtime_settings, LaunchState, RuntimeSettings,
};
use super::{codex_config, overview, prefs};
use crate::codex::binary;
use crate::util::time::unix_secs;
use crate::{storage, AppState};

#[tauri::command]
pub(crate) fn read_runtime_settings(state: State<'_, AppState>) -> RuntimeSettings {
    let overrides = prefs::read_overrides(&prefs::settings_path());
    runtime_settings(&state.runtime(), &overrides)
}

#[tauri::command]
pub(crate) fn read_launch_state(state: State<'_, AppState>) -> LaunchState {
    launch_state(&state)
}

/// Switch the active Codex home. Safe pre-boot (nothing has spawned yet) and
/// also handles a live switch: the frontend database is reopened against the
/// new home and any running app-server child is killed so the next request
/// respawns with the new `CODEX_HOME`.
#[tauri::command]
pub(crate) async fn select_codex_home(
    path: String,
    state: State<'_, AppState>,
) -> Result<LaunchState, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("Choose a Codex home folder".to_string());
    }
    // Expand a leading `~` so raw typed paths like `~/.codex-work` resolve.
    let home = binary::expand_tilde(trimmed);
    // Selecting a home creates it on disk, so refuse before creating anything
    // we could never boot: without a working CLI the app-server spawn fails.
    let configured = state.runtime().codex_binary;
    if binary::resolve(&configured).is_none() {
        return Err(binary::missing_message(&configured));
    }
    // Opening the database also creates the home directory if it is new.
    let database = storage::open(&home).await?;
    let mut runtime = state.runtime();
    runtime.codex_home = home.clone();
    // Swap the active runtime/database first, then drop the old app-server
    // child so the next request respawns against the new home.
    state.set_active(runtime, database);
    // Running agents were spawned against the old CODEX_HOME and write into
    // the database we just swapped out, so they cannot be carried over.
    state.agents.kill_all();
    state.session.reset().await;
    prefs::record_recent_home(
        &prefs::settings_path(),
        &home.display().to_string(),
        unix_secs(),
    )?;
    Ok(launch_state(&state))
}

/// Forget a home from the recents list shown by the launch picker. Does not
/// touch the folder on disk.
#[tauri::command]
pub(crate) fn remove_recent_home(
    path: String,
    state: State<'_, AppState>,
) -> Result<LaunchState, String> {
    prefs::forget_recent_home(&prefs::settings_path(), &path)?;
    Ok(launch_state(&state))
}

/// Read-only overview of the active home's defaults (model, MCP servers,
/// skills) for the homepage dashboard.
///
/// Skills come from Codex rather than the filesystem so this agrees with the
/// Integrations tab; if Codex is unreachable the list is simply empty.
#[tauri::command]
pub(crate) async fn read_home_overview(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<overview::HomeOverview, String> {
    let skills = crate::integrations::app_server::fetch_skills(&app, &state, Vec::new())
        .await
        .into_iter()
        .map(|skill| overview::SkillInfo { name: skill.name })
        .collect();
    let runtime = state.runtime();
    Ok(overview::read_home_overview(
        &runtime.codex_home,
        &runtime.codex_binary.display().to_string(),
        skills,
    ))
}

/// Read the whitelisted `config.toml` settings for the active home, with their
/// source (default vs config) and restart semantics.
#[tauri::command]
pub(crate) fn read_config_settings(state: State<'_, AppState>) -> Vec<codex_config::ConfigSetting> {
    codex_config::read_config_settings(&state.runtime().codex_home)
}

/// Set or unset a single whitelisted `config.toml` key, preserving the rest of
/// the file. Passing `unset: true` removes the key so Codex inherits its
/// default; otherwise `value` is written. Returns the refreshed settings list.
#[tauri::command]
pub(crate) fn write_config_setting(
    key: String,
    value: Option<String>,
    unset: Option<bool>,
    state: State<'_, AppState>,
) -> Result<Vec<codex_config::ConfigSetting>, String> {
    codex_config::write_config_setting(
        &state.runtime().codex_home,
        &key,
        value.as_deref(),
        unset.unwrap_or(false),
    )
}

#[tauri::command]
pub(crate) fn update_runtime_settings(
    codex_home: Option<String>,
    codex_binary: Option<String>,
    state: State<'_, AppState>,
) -> Result<RuntimeSettings, String> {
    let codex_binary = normalize_override(codex_binary);
    // Reject a binary that cannot be spawned rather than saving an override
    // that only fails on the next launch.
    if let Some(candidate) = &codex_binary {
        let candidate = PathBuf::from(candidate);
        if binary::resolve(&candidate).is_none() {
            return Err(binary::missing_message(&candidate));
        }
    }
    // Read-modify-write so unrelated overrides (recent homes, the quick-chat
    // shortcut) survive a save from the runtime-identity form.
    let mut overrides = prefs::read_overrides(&prefs::settings_path());
    overrides.codex_home = normalize_override(codex_home);
    overrides.codex_binary = codex_binary;
    prefs::write_overrides(&prefs::settings_path(), &overrides)?;
    Ok(runtime_settings(&state.runtime(), &overrides))
}

/// How app-owned subagents are configured, as the settings form sees them.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentSettingsPayload {
    pub(crate) enabled: bool,
    pub(crate) sandbox: String,
    pub(crate) max_concurrent: u32,
    pub(crate) timeout_seconds: u64,
    /// The sandboxes the form may offer, so the choices live in one place.
    #[serde(skip_deserializing)]
    pub(crate) sandbox_options: Vec<String>,
}

fn agent_payload(settings: prefs::AgentSettings) -> AgentSettingsPayload {
    AgentSettingsPayload {
        enabled: settings.enabled,
        sandbox: settings.sandbox,
        max_concurrent: settings.max_concurrent as u32,
        timeout_seconds: settings.timeout_seconds,
        sandbox_options: prefs::AGENT_SANDBOXES
            .iter()
            .map(|value| value.to_string())
            .collect(),
    }
}

#[tauri::command]
pub(crate) fn read_agent_settings() -> AgentSettingsPayload {
    agent_payload(prefs::read_agent_settings(&prefs::settings_path()))
}

#[tauri::command]
pub(crate) fn write_agent_settings(
    settings: AgentSettingsPayload,
) -> Result<AgentSettingsPayload, String> {
    let path = prefs::settings_path();
    prefs::write_agent_settings(
        &path,
        &prefs::AgentSettings {
            enabled: settings.enabled,
            sandbox: settings.sandbox,
            max_concurrent: settings.max_concurrent as usize,
            timeout_seconds: settings.timeout_seconds,
        },
    )?;
    Ok(agent_payload(prefs::read_agent_settings(&path)))
}

/// Probe a candidate Codex CLI without saving it, so the picker and the
/// settings form can show "found at …" as the user types.
#[tauri::command]
pub(crate) fn check_codex_binary(
    path: Option<String>,
    state: State<'_, AppState>,
) -> binary::BinaryStatus {
    let candidate = normalize_override(path)
        .map(PathBuf::from)
        .unwrap_or_else(|| state.runtime().codex_binary);
    binary::status(&candidate)
}

/// Point the app at a different Codex CLI and apply it immediately: the
/// override is persisted, the live runtime is updated, and any running
/// app-server is dropped so the next request respawns with the new binary.
/// `path` of `None` clears the override back to bare `codex`.
#[tauri::command]
pub(crate) async fn set_codex_binary(
    path: Option<String>,
    state: State<'_, AppState>,
) -> Result<LaunchState, String> {
    let override_binary = normalize_override(path);
    let candidate = override_binary
        .clone()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("codex"));
    if binary::resolve(&candidate).is_none() {
        return Err(binary::missing_message(&candidate));
    }
    let mut overrides = prefs::read_overrides(&prefs::settings_path());
    overrides.codex_binary = override_binary;
    prefs::write_overrides(&prefs::settings_path(), &overrides)?;

    let mut runtime = state.runtime();
    runtime.codex_binary = candidate;
    state.set_active(runtime, state.database());
    state.session.reset().await;
    Ok(launch_state(&state))
}
