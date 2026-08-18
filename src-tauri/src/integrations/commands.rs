//! The integrations commands the frontend calls.
//!
//! Each mutation is read-modify-write over `config.toml`, so unrelated keys and
//! comments in the user's file survive. Every mutation then asks Codex to
//! reload — otherwise the running session keeps serving whatever servers it
//! started with and the UI reports changes the agent cannot see.

use serde::Deserialize;
use std::collections::BTreeMap;
use tauri::{AppHandle, State};

use super::app_server::{fetch_skills, reload_mcp_config};
use super::config_doc::{
    load, remove_server_from_doc, rename_server_in_doc, save, set_enabled_in_doc,
    summarize_mcp_servers, upsert_http_server, upsert_stdio_server, validate_server_name,
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

/// One MCP server as the edit form describes it.
///
/// Transport is chosen by which fields are populated, matching Codex's own
/// config shape: a `command` means stdio, a `url` means streamable HTTP.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct McpServerInput {
    /// The name the server is currently stored under. `None` adds a new server;
    /// `Some` edits that entry, which is also how a rename is expressed.
    #[serde(default)]
    previous_name: Option<String>,
    name: String,
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    args: Vec<String>,
    /// Newly typed secret values only; see `env_keys`.
    #[serde(default)]
    env: BTreeMap<String, String>,
    /// Full desired set of env variable names; see `upsert_stdio_server`. Absent
    /// means "leave the stored env table alone".
    #[serde(default)]
    env_keys: Option<Vec<String>>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    bearer_token_env_var: Option<String>,
}

/// Add a new MCP server, or save edits to an existing one.
#[tauri::command]
pub(crate) async fn save_mcp_server(
    server: McpServerInput,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<IntegrationsList, String> {
    let name = server.name;
    validate_server_name(&name)?;
    let home = state.runtime().codex_home;
    let mut doc = load(&home)?;

    // Rename first so every later edit targets the entry under its final key,
    // and so a name collision fails before anything is written to disk.
    match server.previous_name.as_deref() {
        Some(previous) => rename_server_in_doc(&mut doc, previous, &name)?,
        None => {
            if summarize_mcp_servers(&doc)
                .iter()
                .any(|existing| existing.name == name)
            {
                return Err(format!("An MCP server named '{name}' already exists"));
            }
        }
    }

    let command = server
        .command
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let url = server
        .url
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    match (command, url) {
        (Some(command), _) => upsert_stdio_server(
            &mut doc,
            &name,
            &command,
            &server.args,
            &server.env,
            server.env_keys.as_deref(),
        )?,
        (None, Some(url)) => upsert_http_server(
            &mut doc,
            &name,
            &url,
            server.bearer_token_env_var.as_deref(),
        )?,
        (None, None) => return Err("Provide a command (stdio) or a URL (http)".to_string()),
    }

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
