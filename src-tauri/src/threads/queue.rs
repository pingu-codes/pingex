//! Server-side thread queue (`thread/queue/*`, experimental in the app-server).
//!
//! The server holds queued submissions durably but never auto-drains them:
//! `thread/queue/start` errors if a turn is already running, so the frontend
//! drives one start per idle transition.

use serde_json::{json, Value};
use tauri::{AppHandle, State};

use crate::AppState;

#[tauri::command]
pub(crate) async fn queue_add(
    thread_id: String,
    input: Value,
    client_user_message_id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    state.session.ensure_resumed(&app, &thread_id).await?;
    state
        .session
        .request(
            &app,
            "thread/queue/add",
            json!({
                "threadId": thread_id,
                "input": input,
                "clientUserMessageId": client_user_message_id,
            }),
        )
        .await
}

#[tauri::command]
pub(crate) async fn queue_list(
    thread_id: String,
    cursor: Option<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    state.session.ensure_resumed(&app, &thread_id).await?;
    state
        .session
        .request(
            &app,
            "thread/queue/list",
            json!({"threadId": thread_id, "cursor": cursor}),
        )
        .await
}

#[tauri::command]
pub(crate) async fn queue_update(
    thread_id: String,
    queued_submission_id: String,
    input: Value,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    state.session.ensure_resumed(&app, &thread_id).await?;
    state
        .session
        .request(
            &app,
            "thread/queue/update",
            json!({
                "threadId": thread_id,
                "queuedSubmissionId": queued_submission_id,
                "input": input,
            }),
        )
        .await
}

#[tauri::command]
pub(crate) async fn queue_delete(
    thread_id: String,
    queued_submission_id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    state.session.ensure_resumed(&app, &thread_id).await?;
    state
        .session
        .request(
            &app,
            "thread/queue/delete",
            json!({"threadId": thread_id, "queuedSubmissionId": queued_submission_id}),
        )
        .await
}

#[tauri::command]
pub(crate) async fn queue_reorder(
    thread_id: String,
    queued_submission_ids: Vec<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    state.session.ensure_resumed(&app, &thread_id).await?;
    state
        .session
        .request(
            &app,
            "thread/queue/reorder",
            json!({"threadId": thread_id, "queuedSubmissionIds": queued_submission_ids}),
        )
        .await
}

#[tauri::command]
pub(crate) async fn queue_start(
    thread_id: String,
    queued_submission_id: Option<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    state.session.ensure_resumed(&app, &thread_id).await?;
    state
        .session
        .request(
            &app,
            "thread/queue/start",
            json!({"threadId": thread_id, "queuedSubmissionId": queued_submission_id}),
        )
        .await
}
