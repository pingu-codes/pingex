//! Server-side thread queue (`thread/queue/*`, experimental in the app-server).
//!
//! The server holds queued submissions durably but never auto-drains them:
//! `thread/queue/start` errors if a turn is already running, so the frontend
//! drives one start per idle transition.
//!
//! Not every Codex can do this. The APIs landed upstream in 0.149 (0.146.0,
//! the previous stable, has none), they sit behind the `experimentalApi`
//! capability, and even on a build that has them the queue needs a SQLite
//! state database to exist at all. Rather than pin a version, the generic
//! refusals are recognised by [`crate::codex::compat`] and the storage one by
//! [`classify`]; the verdict is cached on the live child and reported to the
//! frontend under [`QUEUE_UNSUPPORTED`] so it can fall back to queueing in the
//! window under `Feature::QUEUE`'s error prefix.

use serde_json::Value;
use tauri::{AppHandle, State};

use crate::codex::compat::{error_payload, Feature};
use crate::codex::requests;
use crate::AppState;

/// The queue-specific refusal on top of the generic ones: a build that has the
/// APIs but no queue database for this home. `None` for a normal failure of a
/// queue that does work (a full queue, a busy thread, an unknown thread).
fn classify(error: &str) -> Option<String> {
    let payload = error_payload(error);
    let message = payload
        .as_ref()
        .and_then(|value| value.get("message"))
        .and_then(Value::as_str)
        .unwrap_or(error);
    message
        .contains("user message queue is unavailable")
        .then(|| "Codex has no queue database for this home".to_string())
}

/// Run one `thread/queue/*` call, short-circuiting if this child has already
/// refused the API and classifying it if this is the first refusal.
async fn queue_request(
    app: &AppHandle,
    ctx: &crate::HomeContext,
    thread_id: &str,
    request: requests::Request,
) -> Result<Value, String> {
    if let Some(reason) = ctx.session.unsupported(app, Feature::QUEUE).await? {
        return Err(Feature::QUEUE.error(&reason));
    }
    ctx.session.ensure_resumed(app, thread_id).await?;
    ctx.session
        .send_gated(app, Feature::QUEUE, request, classify)
        .await
}

#[tauri::command]
pub(crate) async fn queue_add(
    thread_id: String,
    input: Value,
    client_user_message_id: String,
    app: AppHandle,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let ctx = state.ctx(&window);
    let request = requests::queue_add(&thread_id, input, &client_user_message_id);
    queue_request(&app, &ctx, &thread_id, request).await
}

#[tauri::command]
pub(crate) async fn queue_list(
    thread_id: String,
    cursor: Option<String>,
    app: AppHandle,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let ctx = state.ctx(&window);
    let request = requests::queue_list(&thread_id, cursor.as_deref());
    queue_request(&app, &ctx, &thread_id, request).await
}

#[tauri::command]
pub(crate) async fn queue_update(
    thread_id: String,
    queued_submission_id: String,
    input: Value,
    app: AppHandle,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let ctx = state.ctx(&window);
    let request = requests::queue_update(&thread_id, &queued_submission_id, input);
    queue_request(&app, &ctx, &thread_id, request).await
}

#[tauri::command]
pub(crate) async fn queue_delete(
    thread_id: String,
    queued_submission_id: String,
    app: AppHandle,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let ctx = state.ctx(&window);
    let request = requests::queue_delete(&thread_id, &queued_submission_id);
    queue_request(&app, &ctx, &thread_id, request).await
}

#[tauri::command]
pub(crate) async fn queue_reorder(
    thread_id: String,
    queued_submission_ids: Vec<String>,
    app: AppHandle,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let ctx = state.ctx(&window);
    let request = requests::queue_reorder(&thread_id, &queued_submission_ids);
    queue_request(&app, &ctx, &thread_id, request).await
}

#[tauri::command]
pub(crate) async fn queue_start(
    thread_id: String,
    queued_submission_id: Option<String>,
    app: AppHandle,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let ctx = state.ctx(&window);
    let request = requests::queue_start(&thread_id, queued_submission_id.as_deref());
    queue_request(&app, &ctx, &thread_id, request).await
}

#[cfg(test)]
mod tests {
    use super::classify;
    use crate::codex::compat::{method_unsupported, Feature};

    /// Wrap a message the way `child.rs` reports a JSON-RPC failure.
    fn failure(code: i64, message: &str) -> String {
        format!(
            "Codex request failed: {}",
            serde_json::json!({"code": code, "message": message})
        )
    }

    /// Either refusal shape `send_gated` would act on for the queue feature.
    fn unsupported(error: &str) -> bool {
        method_unsupported(error, Feature::QUEUE.method_prefix).is_some() || classify(error).is_some()
    }

    #[test]
    fn classifies_a_codex_without_the_queue_apis() {
        // Codex 0.147.0: serde rejects the method name against its own enum.
        assert!(unsupported(&failure(
            -32600,
            "Invalid request: unknown variant `thread/queue/add`, expected one of `initialize`, `thread/start`"
        )));
        assert!(unsupported(&failure(-32601, "Method not found")));
    }

    /// The exact refusal codex-cli 0.147.0 returns, captured off the wire, so a
    /// rephrasing upstream fails here rather than in the user's window.
    #[test]
    fn classifies_the_refusal_codex_0_147_actually_sends() {
        let observed = r#"Codex request failed: {"code":-32600,"message":"Invalid request: unknown variant `thread/queue/add`, expected one of `initialize`, `thread/start`, `thread/resume`, `thread/fork`, `thread/archive`, `thread/delete`, `turn/start`, `turn/steer`, `turn/interrupt`, `fuzzyFileSearch/sessionStop`"}"#;
        assert!(unsupported(observed));
    }

    #[test]
    fn classifies_a_gated_or_storageless_queue() {
        assert!(unsupported(&failure(
            -32600,
            "thread/queue/add requires experimentalApi capability"
        )));
        assert!(unsupported(&failure(-32600, "user message queue is unavailable")));
    }

    #[test]
    fn passes_through_failures_of_a_working_queue() {
        // These share the -32600 code with the unsupported cases above, which
        // is exactly why the code alone cannot be the signal.
        assert!(!unsupported(&failure(
            -32600,
            "queue cannot contain more than 100 submissions"
        )));
        assert!(!unsupported(&failure(
            -32600,
            "thread already has an active or pending turn"
        )));
        assert!(!unsupported(&failure(-32600, "invalid queue pagination cursor: x")));
        assert!(!unsupported(&failure(-32600, "thread not found: abc")));
    }

    #[test]
    fn survives_errors_that_are_not_json() {
        assert!(!unsupported("Codex exited before responding"));
        assert!(!unsupported("Codex request failed: client not found"));
    }
}
