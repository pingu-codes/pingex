//! Server-side thread queue (`thread/queue/*`, experimental in the app-server).
//!
//! The server holds queued submissions durably but never auto-drains them:
//! `thread/queue/start` errors if a turn is already running, so the frontend
//! drives one start per idle transition.
//!
//! Not every Codex can do this. The APIs landed upstream after 0.147.x, they
//! sit behind the `experimentalApi` capability, and even on a build that has
//! them the queue needs a SQLite state database to exist at all. Rather than
//! pin a version, [`classify`] recognises those three refusals, caches them on
//! the live child, and reports them to the frontend as [`QUEUE_UNSUPPORTED`]
//! so it can fall back to queueing in the window.

use serde_json::Value;
use tauri::{AppHandle, State};

use crate::codex::requests;
use crate::AppState;

/// Prefix marking an error as "this Codex has no usable server queue", as
/// opposed to a real failure of an otherwise-working one. The frontend matches
/// this exact string; both sides assert on it in tests.
pub(crate) const QUEUE_UNSUPPORTED: &str = "codex-queue-unsupported";

/// JSON-RPC "method not found" — what a well-behaved server returns for an API
/// it does not have. Codex ≤0.147.x instead fails to deserialise the method
/// name at all, so [`classify`] has to recognise that shape too.
const METHOD_NOT_FOUND: i64 = -32601;

/// Why this Codex cannot hold queued messages, or `None` if the error is a
/// normal failure of a queue that does work (a full queue, a busy thread, an
/// unknown thread) and should be surfaced as-is.
///
/// Deliberately does not key off the `-32600` "invalid request" code, which
/// every one of those recoverable errors also carries; treating it as
/// unsupported would strand a thread in local-queue mode for the rest of the
/// child's life over a transient full queue.
fn classify(error: &str) -> Option<String> {
    // `child.rs` formats failures as `Codex request failed: {json}`, so the
    // original error object — including its code — is still recoverable.
    let payload = error
        .split_once("Codex request failed: ")
        .and_then(|(_, rest)| serde_json::from_str::<Value>(rest).ok());
    let code = payload
        .as_ref()
        .and_then(|value| value.get("code"))
        .and_then(Value::as_i64);
    let message = payload
        .as_ref()
        .and_then(|value| value.get("message"))
        .and_then(Value::as_str)
        .unwrap_or(error);

    if code == Some(METHOD_NOT_FOUND) {
        return Some("this Codex has no thread/queue APIs".into());
    }
    // Codex ≤0.147.x predates the queue variants, so serde rejects the method
    // name before the server ever dispatches it.
    if message.contains("unknown variant") && message.contains("thread/queue/") {
        return Some("this Codex version is older than the thread/queue APIs".into());
    }
    if message.contains("requires experimentalApi capability") {
        return Some("Codex did not grant the experimental API this queue needs".into());
    }
    if message.contains("user message queue is unavailable") {
        return Some("Codex has no queue database for this home".into());
    }
    None
}

/// Run one `thread/queue/*` call, short-circuiting if this child has already
/// refused the API and classifying it if this is the first refusal.
async fn queue_request(
    app: &AppHandle,
    ctx: &crate::HomeContext,
    thread_id: &str,
    request: requests::Request,
) -> Result<Value, String> {
    if let Some(reason) = ctx.session.queue_unsupported(app).await? {
        return Err(format!("{QUEUE_UNSUPPORTED}: {reason}"));
    }
    ctx.session.ensure_resumed(app, thread_id).await?;
    match ctx.session.send(app, request).await {
        Ok(response) => Ok(response),
        Err(error) => match classify(&error) {
            Some(reason) => {
                ctx.session.mark_queue_unsupported(app, &reason).await?;
                Err(format!("{QUEUE_UNSUPPORTED}: {reason}"))
            }
            None => Err(error),
        },
    }
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

    /// Wrap a message the way `child.rs` reports a JSON-RPC failure.
    fn failure(code: i64, message: &str) -> String {
        format!(
            "Codex request failed: {}",
            serde_json::json!({"code": code, "message": message})
        )
    }

    #[test]
    fn classifies_a_codex_without_the_queue_apis() {
        // Codex 0.147.0: serde rejects the method name against its own enum.
        assert!(classify(&failure(
            -32600,
            "Invalid request: unknown variant `thread/queue/add`, expected one of `initialize`, `thread/start`"
        ))
        .is_some());
        assert!(classify(&failure(-32601, "Method not found")).is_some());
    }

    /// The exact refusal codex-cli 0.147.0 returns, captured off the wire, so a
    /// rephrasing upstream fails here rather than in the user's window.
    #[test]
    fn classifies_the_refusal_codex_0_147_actually_sends() {
        let observed = r#"Codex request failed: {"code":-32600,"message":"Invalid request: unknown variant `thread/queue/add`, expected one of `initialize`, `thread/start`, `thread/resume`, `thread/fork`, `thread/archive`, `thread/delete`, `turn/start`, `turn/steer`, `turn/interrupt`, `fuzzyFileSearch/sessionStop`"}"#;
        assert!(classify(observed).is_some());
    }

    #[test]
    fn classifies_a_gated_or_storageless_queue() {
        assert!(classify(&failure(
            -32600,
            "thread/queue/add requires experimentalApi capability"
        ))
        .is_some());
        assert!(classify(&failure(-32600, "user message queue is unavailable")).is_some());
    }

    #[test]
    fn passes_through_failures_of_a_working_queue() {
        // These share the -32600 code with the unsupported cases above, which
        // is exactly why the code alone cannot be the signal.
        assert!(classify(&failure(
            -32600,
            "queue cannot contain more than 100 submissions"
        ))
        .is_none());
        assert!(classify(&failure(
            -32600,
            "thread already has an active or pending turn"
        ))
        .is_none());
        assert!(classify(&failure(-32600, "invalid queue pagination cursor: x")).is_none());
        assert!(classify(&failure(-32600, "thread not found: abc")).is_none());
    }

    #[test]
    fn survives_errors_that_are_not_json() {
        assert!(classify("Codex exited before responding").is_none());
        assert!(classify("Codex request failed: client not found").is_none());
    }
}
