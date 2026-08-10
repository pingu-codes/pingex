//! The integrations commands the frontend calls.
//!
//! Each mutation is read-modify-write over `config.toml`, so unrelated keys and
//! comments in the user's file survive. Every mutation then asks Codex to
//! reload — otherwise the running session keeps serving whatever servers it
//! started with and the UI reports changes the agent cannot see.

use std::collections::BTreeMap;
use tauri::{AppHandle, State};

use super::app_server::{fetch_skills, reload_mcp_config};
use super::config_doc::{
    load, remove_server_from_doc, save, set_enabled_in_doc, summarize_mcp_servers,
    upsert_stdio_server, validate_server_name,
};
use super::IntegrationsList;
use crate::AppState;

/// Build the list from the current on-disk config plus Codex's skill view.
///
/// `cwds` scopes the skill lookup: passing the active project's directory
/// surfaces project skills alongside user ones. An empty list still returns
/// user- and system-scoped skills.
async fn build_list(
    app: &AppHandle,
    state: &State<'_, AppState>,
    cwds: Vec<String>,
) -> Result<IntegrationsList, String> {
    let doc = load(&state.runtime().codex_home)?;
    Ok(IntegrationsList {
        mcp_servers: summarize_mcp_servers(&doc),
        skills: fetch_skills(app, state, cwds).await,
        plugins: Vec::new(),
        plugins_supported: false,
    })
}

#[tauri::command]
pub(crate) async fn list_integrations(
    cwds: Option<Vec<String>>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<IntegrationsList, String> {
    build_list(&app, &state, cwds.unwrap_or_default()).await
}

#[tauri::command]
pub(crate) async fn add_mcp_server(
    name: String,
    command: String,
    args: Vec<String>,
    env: BTreeMap<String, String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<IntegrationsList, String> {
    validate_server_name(&name)?;
    let home = state.runtime().codex_home;
    let mut doc = load(&home)?;
    upsert_stdio_server(&mut doc, &name, &command, &args, &env)?;
    save(&home, &doc)?;
    reload(&app, &state).await;
    // Re-read so the returned list reflects exactly what is on disk (redacted).
    build_list(&app, &state, Vec::new()).await
}

#[tauri::command]
pub(crate) async fn remove_mcp_server(
    name: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<IntegrationsList, String> {
    let home = state.runtime().codex_home;
    let mut doc = load(&home)?;
    remove_server_from_doc(&mut doc, &name)?;
    save(&home, &doc)?;
    reload(&app, &state).await;
    build_list(&app, &state, Vec::new()).await
}

#[tauri::command]
pub(crate) async fn set_mcp_enabled(
    name: String,
    enabled: bool,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<IntegrationsList, String> {
    let home = state.runtime().codex_home;
    let mut doc = load(&home)?;
    set_enabled_in_doc(&mut doc, &name, enabled)?;
    save(&home, &doc)?;
    reload(&app, &state).await;
    build_list(&app, &state, Vec::new()).await
}

/// Ask Codex to re-read `config.toml`. Deliberately best-effort: the edit is
/// already durable on disk, so a reload failure means "takes effect next
/// restart", not "the change was lost". Failing the whole command here would
/// misreport a successful save.
async fn reload(app: &AppHandle, state: &State<'_, AppState>) {
    if let Err(error) = reload_mcp_config(app, state).await {
        eprintln!("MCP config reload failed; changes apply on next restart: {error}");
    }
}
