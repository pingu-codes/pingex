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
use crate::util::json::Json;
use crate::util::time::unix_secs;
use crate::AppState;

#[tauri::command]
#[specta::specta]
pub(crate) fn read_runtime_settings(
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> RuntimeSettings {
    let overrides = prefs::read_overrides(&prefs::settings_path());
    runtime_settings(&state.ctx(&window).runtime(), &overrides)
}

/// What this window boots against. A window is "explicit" once it is bound to
/// a home — the first window inherits the launch binding, later windows are
/// bound by `open_home_window` or pick a home themselves.
#[tauri::command]
#[specta::specta]
pub(crate) fn read_launch_state(
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> LaunchState {
    let bound = state.window_bound(window.label());
    launch_state(&state.ctx(&window), bound)
}

/// What the running app-server said about itself at `initialize`: the CLI's
/// `userAgent` (which embeds its version) and platform. Spawns the child if it
/// is not running yet. Shown in Settings so a version mismatch is visible
/// without a terminal; nothing in the app branches on it.
#[tauri::command]
#[specta::specta]
pub(crate) async fn read_codex_server_info(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Json, String> {
    state.ctx(&window).session.server_info(&app).await.map(Json)
}

/// Bind *this window* to a Codex home. Safe pre-boot (nothing has spawned
/// yet) and also handles a live switch: the window is re-pointed at the
/// (reused or freshly opened) context for the new home, and the old context is
/// shut down only when no other window still uses it.
#[tauri::command]
#[specta::specta]
pub(crate) async fn select_codex_home(
    path: String,
    window: tauri::WebviewWindow,
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
    let configured = state.ctx(&window).runtime().codex_binary;
    if binary::resolve(&configured).is_none() {
        return Err(binary::missing_message(&configured));
    }
    // Opening the context also creates the home directory if it is new.
    let context = state.ensure_context(home.clone()).await?;
    // The main window's home is the app's default: quick chat and any window
    // that has not picked a home yet follow it.
    if window.label() == "main" {
        state.set_default_home(&context.home_key);
    }
    if let Some(orphaned) = state.bind_window(window.label(), &context.home_key) {
        // Nothing else uses the previous home any more: its agents were
        // spawned against it and its child holds its auth, so both die here.
        orphaned.shutdown();
    }
    prefs::record_recent_home(
        &prefs::settings_path(),
        &home.display().to_string(),
        unix_secs(),
    )?;
    Ok(launch_state(&context, true))
}

/// Open a new app window, optionally bound to a home straight away. With no
/// `path` the window shows the launch picker and binds itself on pick.
#[tauri::command]
#[specta::specta]
pub(crate) async fn open_home_window(
    path: Option<String>,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let label = state.next_window_label();
    if let Some(path) = path.as_deref().map(str::trim).filter(|path| !path.is_empty()) {
        let home = binary::expand_tilde(path);
        let configured = state.default_context().runtime().codex_binary;
        if binary::resolve(&configured).is_none() {
            return Err(binary::missing_message(&configured));
        }
        let context = state.ensure_context(home.clone()).await?;
        // Bind before the window loads, so its `read_launch_state` boots
        // straight into this home instead of showing the picker.
        if let Some(orphaned) = state.bind_window(&label, &context.home_key) {
            orphaned.shutdown();
        }
        prefs::record_recent_home(
            &prefs::settings_path(),
            &home.display().to_string(),
            unix_secs(),
        )?;
    }
    // Clone the declared `main` window config so new windows keep its size,
    // titlebar style, and URL without duplicating them in code.
    let mut config = app
        .config()
        .app
        .windows
        .iter()
        .find(|window| window.label == "main")
        .cloned()
        .ok_or("No main window configuration found")?;
    config.label = label.clone();
    tauri::WebviewWindowBuilder::from_config(&app, &config)
        .map_err(|error| format!("Could not configure the window: {error}"))?
        .build()
        .map_err(|error| format!("Could not open the window: {error}"))?;
    Ok(label)
}

/// Forget a home from the recents list shown by the launch picker. Does not
/// touch the folder on disk.
#[tauri::command]
#[specta::specta]
pub(crate) fn remove_recent_home(
    path: String,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<LaunchState, String> {
    prefs::forget_recent_home(&prefs::settings_path(), &path)?;
    Ok(launch_state(
        &state.ctx(&window),
        state.window_bound(window.label()),
    ))
}

/// Read-only overview of the active home's defaults (model, MCP servers,
/// skills) for the homepage dashboard.
///
/// Skills come from Codex rather than the filesystem so this agrees with the
/// Integrations tab; if Codex is unreachable the list is simply empty.
#[tauri::command]
#[specta::specta]
pub(crate) async fn read_home_overview(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<overview::HomeOverview, String> {
    let ctx = state.ctx(&window);
    let skills = crate::integrations::app_server::fetch_skills(&app, &ctx, Vec::new(), false)
        .await
        .into_iter()
        .map(|skill| overview::SkillInfo { name: skill.name })
        .collect();
    let runtime = ctx.runtime();
    Ok(overview::read_home_overview(
        &runtime.codex_home,
        &runtime.codex_binary.display().to_string(),
        skills,
    ))
}

/// Read the whitelisted `config.toml` settings for the active home, with their
/// source (default vs config) and restart semantics.
#[tauri::command]
#[specta::specta]
pub(crate) fn read_config_settings(
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Vec<codex_config::ConfigSetting> {
    codex_config::read_config_settings(&state.ctx(&window).runtime().codex_home)
}

/// Set or unset a single whitelisted `config.toml` key, preserving the rest of
/// the file. Passing `unset: true` removes the key so Codex inherits its
/// default; otherwise `value` is written. Returns the refreshed settings list.
#[tauri::command]
#[specta::specta]
pub(crate) fn write_config_setting(
    key: String,
    value: Option<String>,
    unset: Option<bool>,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Vec<codex_config::ConfigSetting>, String> {
    codex_config::write_config_setting(
        &state.ctx(&window).runtime().codex_home,
        &key,
        value.as_deref(),
        unset.unwrap_or(false),
    )
}

#[tauri::command]
#[specta::specta]
pub(crate) fn update_runtime_settings(
    codex_home: Option<String>,
    codex_binary: Option<String>,
    window: tauri::WebviewWindow,
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
    Ok(runtime_settings(&state.ctx(&window).runtime(), &overrides))
}

/// How app-owned subagents are configured, as the settings form sees them.
#[derive(serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentSettingsPayload {
    pub(crate) enabled: bool,
    pub(crate) sandbox: String,
    pub(crate) max_concurrent: u32,
    pub(crate) timeout_seconds: u64,
    /// The sandboxes the form may offer, so the choices live in one place.
    /// Ignored on the way back in (`default`, so the form may echo it).
    #[serde(default)]
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
#[specta::specta]
pub(crate) fn read_agent_settings() -> AgentSettingsPayload {
    agent_payload(prefs::read_agent_settings(&prefs::settings_path()))
}

#[tauri::command]
#[specta::specta]
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
#[specta::specta]
pub(crate) fn check_codex_binary(
    path: Option<String>,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> binary::BinaryStatus {
    let candidate = normalize_override(path)
        .map(PathBuf::from)
        .unwrap_or_else(|| state.ctx(&window).runtime().codex_binary);
    binary::status(&candidate)
}

/// Point the app at a different Codex CLI and apply it immediately: the
/// override is persisted, the live runtime is updated, and any running
/// app-server is dropped so the next request respawns with the new binary.
/// `path` of `None` clears the override back to bare `codex`.
#[tauri::command]
#[specta::specta]
pub(crate) async fn set_codex_binary(
    path: Option<String>,
    window: tauri::WebviewWindow,
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

    // The CLI is global: every open home starts using it on its next spawn.
    for context in state.all_contexts() {
        context.set_binary(candidate.clone());
        context.session.reset().await;
    }
    Ok(launch_state(
        &state.ctx(&window),
        state.window_bound(window.label()),
    ))
}
