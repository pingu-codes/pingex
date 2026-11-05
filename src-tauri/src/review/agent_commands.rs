//! Commands for agent reviews, their Findings and the repository review
//! rules: thin wrappers over `desk::ReviewDesk`, like `commands`. The queue
//! commands are Home-wide and take no repository.

use std::path::PathBuf;
use std::sync::Arc;
use tauri::State;

use super::agent_review::{AgentReview, AgentReviewRequest, ReviewRule};
use super::checkout_types::CheckoutTarget;
use super::desk::{self, ReviewDesk};
use super::findings::{DismissReason, Finding, FindingState};
use super::github::GhCli;
use crate::AppState;

fn desk(window: &crate::HomeWindow, state: &State<'_, AppState>, repo_dir: String) -> ReviewDesk {
    let ctx = state.ctx(window);
    let host = ctx.host();
    let dir = PathBuf::from(repo_dir);
    let github = Arc::new(GhCli::new(host.clone(), dir.clone()));
    ReviewDesk::new(host, dir, github, ctx.database())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_enqueue_agent_review(
    repo_dir: String,
    request: AgentReviewRequest,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<AgentReview, String> {
    desk(&window, &state, repo_dir)
        .enqueue_agent_review(request)
        .await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_agent_reviews(
    repo_dir: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<Vec<AgentReview>, String> {
    desk(&window, &state, repo_dir).agent_reviews().await
}

/// The Home's queued and running agent reviews, oldest first.
#[tauri::command]
#[specta::specta]
pub(crate) async fn review_active_agent_reviews(
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<Vec<AgentReview>, String> {
    desk::active_agent_reviews(&state.ctx(&window).database()).await
}

/// The queued agent reviews that may start now, moved to running.
#[tauri::command]
#[specta::specta]
pub(crate) async fn review_claim_agent_reviews(
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<Vec<AgentReview>, String> {
    desk::claim_agent_reviews(&state.ctx(&window).database()).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_agent_review_started(
    repo_dir: String,
    id: String,
    thread_id: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    desk(&window, &state, repo_dir)
        .agent_review_started(id, thread_id)
        .await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_agent_review_prompt(
    repo_dir: String,
    id: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<String, String> {
    desk(&window, &state, repo_dir)
        .agent_review_prompt(id)
        .await
}

/// End a run with its final message (`None` when the turn failed).
#[tauri::command]
#[specta::specta]
pub(crate) async fn review_record_agent_review_result(
    repo_dir: String,
    id: String,
    message: Option<String>,
    error: Option<String>,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<AgentReview, String> {
    desk(&window, &state, repo_dir)
        .record_agent_review_result(id, message, error)
        .await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_rerun_agent_review(
    repo_dir: String,
    id: String,
    targets: Vec<CheckoutTarget>,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<AgentReview, String> {
    desk(&window, &state, repo_dir)
        .rerun_agent_review(id, targets)
        .await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_rerun_on_new_commits(
    repo_dir: String,
    heads: Vec<CheckoutTarget>,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<Vec<AgentReview>, String> {
    desk(&window, &state, repo_dir)
        .rerun_on_new_commits(heads)
        .await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_remove_agent_review(
    repo_dir: String,
    id: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    desk(&window, &state, repo_dir)
        .remove_agent_review(id)
        .await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_findings(
    repo_dir: String,
    number: i64,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<Vec<Finding>, String> {
    desk(&window, &state, repo_dir).findings(number).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_set_finding_state(
    repo_dir: String,
    id: String,
    finding_state: FindingState,
    reason: Option<DismissReason>,
    note: Option<String>,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<Finding, String> {
    desk(&window, &state, repo_dir)
        .set_finding_state(id, finding_state, reason, note)
        .await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_rules(
    repo_dir: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<Vec<ReviewRule>, String> {
    desk(&window, &state, repo_dir).review_rules().await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_add_rule(
    repo_dir: String,
    rule: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<ReviewRule, String> {
    desk(&window, &state, repo_dir).add_review_rule(rule).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_update_rule(
    repo_dir: String,
    id: String,
    rule: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    desk(&window, &state, repo_dir)
        .update_review_rule(id, rule)
        .await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_remove_rule(
    repo_dir: String,
    id: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    desk(&window, &state, repo_dir).remove_review_rule(id).await
}
