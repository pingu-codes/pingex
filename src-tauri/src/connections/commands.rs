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
    window: crate::HomeWindow,
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
    window: crate::HomeWindow,
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
    window: crate::HomeWindow,
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
    window: crate::HomeWindow,
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
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let ctx = state.ctx(&window);
    ensure_table(&ctx.database()).await?;
    // Without the relay there is no way to revoke the credential; dropping
    // the local record anyway would report a revoke that never happened.
    let environment_id = environment_id(&app, &ctx)
        .await
        .ok_or("Remote control is unavailable, so the device could not be revoked")?;
    // A client the relay has already forgotten answers 404; treat that as
    // success so the action is idempotent. Other errors surface and keep the
    // local record.
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
    delete_record(&ctx.database(), &client_id).await
}

/// A revoke for a device the relay has already forgotten is a success, not a
/// failure — the desired end state is already true. Codex reports that as the
/// relay's HTTP 404 inside the `client revoke` failure message; anything else
/// (including a JSON-RPC "method not found") is a real failure.
fn is_already_gone(error: &str) -> bool {
    let message = crate::codex::compat::error_payload(error)
        .and_then(|payload| {
            payload
                .get("message")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| error.to_string());
    message.contains("client revoke failed") && message.contains("HTTP 404")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn already_gone_errors_are_idempotent() {
        let gone = r#"Codex request failed: {"code":-32600,"message":"remote control client revoke failed at `https://chatgpt.com/backend-api/wham/remote/control/environments/env/clients/c`: HTTP 404 Not Found, headers: {}, body: {\"detail\":\"Client not found\"}"}"#;
        assert!(is_already_gone(gone));
    }

    #[test]
    fn other_failures_are_not_idempotent() {
        let method_missing =
            r#"Codex request failed: {"code":-32601,"message":"Method not found"}"#;
        assert!(!is_already_gone(method_missing));
        let unknown_variant = r#"Codex request failed: {"code":-32600,"message":"Invalid request: unknown variant `remoteControl/client/revoke`"}"#;
        assert!(!is_already_gone(unknown_variant));
        let unavailable = r#"Codex request failed: {"code":-32600,"message":"remote control environment not found"}"#;
        assert!(!is_already_gone(unavailable));
        let server_error = r#"Codex request failed: {"code":-32603,"message":"remote control client revoke failed at `u`: HTTP 500 Internal Server Error, headers: {}, body: "}"#;
        assert!(!is_already_gone(server_error));
        assert!(!is_already_gone("network unreachable"));
    }
}
