//! The review commands the frontend calls.
//!
//! Every `gh`-backed command runs on a blocking task, since the work is a
//! subprocess. The draft commands touch only the local database.

use std::path::Path;
use tauri::State;

use super::actions::{reply_to_comment, resolve_thread, submit_review};
use super::gh::provider_status;
use super::queries::{check_freshness, fetch_pr_detail, list_open_prs, local_diff};
use super::types::{PendingComment, PrDetail, PrFile, PrFreshness, PrSummary, ProviderStatus};
use crate::storage::{self, ReviewDraft};
use crate::AppState;

#[tauri::command]
pub(crate) async fn review_provider_status(repo_dir: String) -> Result<ProviderStatus, String> {
    tauri::async_runtime::spawn_blocking(move || provider_status(Path::new(&repo_dir)))
        .await
        .map_err(|_| "Provider check failed".to_string())
}

#[tauri::command]
pub(crate) async fn review_list_prs(repo_dir: String) -> Result<Vec<PrSummary>, String> {
    tauri::async_runtime::spawn_blocking(move || list_open_prs(Path::new(&repo_dir)))
        .await
        .map_err(|_| "Pull-request listing failed".to_string())?
}

#[tauri::command]
pub(crate) async fn review_pr_detail(repo_dir: String, number: i64) -> Result<PrDetail, String> {
    tauri::async_runtime::spawn_blocking(move || fetch_pr_detail(Path::new(&repo_dir), number))
        .await
        .map_err(|_| "Pull-request fetch failed".to_string())?
}

#[tauri::command]
pub(crate) async fn review_check_fresh(
    repo_dir: String,
    number: i64,
    known_head: String,
    known_updated_at: String,
) -> Result<PrFreshness, String> {
    tauri::async_runtime::spawn_blocking(move || {
        check_freshness(Path::new(&repo_dir), number, &known_head, &known_updated_at)
    })
    .await
    .map_err(|_| "Freshness check failed".to_string())?
}

#[tauri::command]
pub(crate) async fn review_local_diff(
    repo_dir: String,
    base: String,
    head: Option<String>,
) -> Result<Vec<PrFile>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        local_diff(Path::new(&repo_dir), &base, head.as_deref())
    })
    .await
    .map_err(|_| "Local diff failed".to_string())?
}

#[tauri::command]
pub(crate) async fn review_submit(
    repo_dir: String,
    number: i64,
    event: String,
    body: String,
    comments: Vec<PendingComment>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        submit_review(Path::new(&repo_dir), number, &event, &body, &comments)
    })
    .await
    .map_err(|_| "Review submission failed".to_string())?
}

#[tauri::command]
pub(crate) async fn review_reply(
    repo_dir: String,
    number: i64,
    comment_id: i64,
    body: String,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        reply_to_comment(Path::new(&repo_dir), number, comment_id, &body)
    })
    .await
    .map_err(|_| "Reply failed".to_string())?
}

#[tauri::command]
pub(crate) async fn review_resolve_thread(
    repo_dir: String,
    thread_id: String,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || resolve_thread(Path::new(&repo_dir), &thread_id))
        .await
        .map_err(|_| "Resolve failed".to_string())?
}

#[tauri::command]
pub(crate) async fn review_save_draft(
    provider: String,
    repo: String,
    pr_number: i64,
    head_sha: String,
    payload: String,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let ctx = state.ctx(&window);
    storage::write_review_draft(
        &ctx.database(),
        &provider,
        &repo,
        pr_number,
        &head_sha,
        &payload,
    )
    .await
}

#[tauri::command]
pub(crate) async fn review_load_draft(
    provider: String,
    repo: String,
    pr_number: i64,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Option<ReviewDraft>, String> {
    let ctx = state.ctx(&window);
    storage::read_review_draft(&ctx.database(), &provider, &repo, pr_number).await
}

#[tauri::command]
pub(crate) async fn review_delete_draft(
    provider: String,
    repo: String,
    pr_number: i64,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let ctx = state.ctx(&window);
    storage::delete_review_draft(&ctx.database(), &provider, &repo, pr_number).await
}
