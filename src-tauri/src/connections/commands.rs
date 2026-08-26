//! The connections commands the frontend calls.
//!
//! Every one is idempotent: disconnecting a device that is already gone, or
//! revoking one the relay has already forgotten, both succeed.

use serde_json::json;

use crate::util::time::unix_secs;
use tauri::{AppHandle, State};

use super::protocol::{collect_connections, environment_id};
use super::store::{delete_record, ensure_table, set_name};
use super::Connection;
use crate::AppState;

#[tauri::command]
#[specta::specta]
pub(crate) async fn list_connections(
    app: AppHandle,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Vec<Connection>, String> {
    let ctx = state.ctx(&window);
    collect_connections(&app, &ctx).await
}

/// Re-poll the relay for fresh health. Identical to `list_connections` today;
/// kept as a distinct command so the UI's "refresh" affordance reads clearly.
#[tauri::command]
#[specta::specta]
pub(crate) async fn refresh_connections(
    app: AppHandle,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Vec<Connection>, String> {
    let ctx = state.ctx(&window);
    collect_connections(&app, &ctx).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn rename_connection(
    client_id: String,
    name: String,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let ctx = state.ctx(&window);
    ensure_table(&ctx.database()).await?;
    let trimmed = name.trim();
    let name = if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    };
    set_name(&ctx.database(), &client_id, name, unix_secs()).await
}

/// Safe action: forget the local record. The credential is untouched, so an
/// active device reappears on the next refresh with its default name.
#[tauri::command]
#[specta::specta]
pub(crate) async fn disconnect_connection(
    client_id: String,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let ctx = state.ctx(&window);
    ensure_table(&ctx.database()).await?;
    delete_record(&ctx.database(), &client_id).await
}

/// Destructive action: revoke the credential through the relay (idempotent —
/// a missing/already-revoked client is treated as success) and drop the local
/// record.
#[tauri::command]
#[specta::specta]
pub(crate) async fn revoke_connection(
    client_id: String,
    app: AppHandle,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let ctx = state.ctx(&window);
    ensure_table(&ctx.database()).await?;
    if let Some(environment_id) = environment_id(&app, &ctx).await {
        // A NotFound/already-revoked client maps to invalid_request; treat that
        // as success so the action is idempotent. Other errors surface.
        if let Err(error) = ctx
            .session
            .request(
                &app,
                "remoteControl/client/revoke",
                json!({ "environmentId": environment_id, "clientId": client_id }),
            )
            .await
        {
            if !is_already_gone(&error) {
                return Err(error);
            }
        }
    }
    delete_record(&ctx.database(), &client_id).await
}

/// A revoke or disconnect for a device the relay has already forgotten is a
/// success, not a failure — the desired end state is already true.
fn is_already_gone(error: &str) -> bool {
    let lowered = error.to_lowercase();
    lowered.contains("not found") || lowered.contains("unknown") || lowered.contains("no such")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn already_gone_errors_are_idempotent() {
        assert!(is_already_gone("Codex request failed: client not found"));
        assert!(is_already_gone("No such client"));
        assert!(!is_already_gone("network unreachable"));
    }
}
