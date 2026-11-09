//! Commands for checkout steps and the PR link on threads: thin wrappers
//! over `desk::ReviewDesk`, like `commands`. Thread-scoped commands find the
//! repository from the thread's PR link.

use std::path::PathBuf;
use std::sync::Arc;
use tauri::State;

use super::checkout_types::{BranchProgress, CheckoutTarget, PrCheckout, PruneReport, StepPlan};
use super::desk::ReviewDesk;
use super::github::GhCli;
use super::requirements::DeskAction;
use crate::storage::{self, PrLink, ReviewCheckoutPolicy};
use crate::AppState;

/// The desk for `repo_dir` on the window's Home, with the Home's directory
/// for review checkouts and worktrees.
fn desk(window: &crate::HomeWindow, state: &State<'_, AppState>, repo_dir: String) -> ReviewDesk {
    let ctx = state.ctx(window);
    let runtime = ctx.runtime();
    let host = runtime.host.clone();
    let dir = PathBuf::from(repo_dir);
    let github = Arc::new(GhCli::new(host.clone(), dir.clone(), ctx.database()));
    ReviewDesk::new(host, dir, github, ctx.database()).with_home_dir(runtime.codex_home_str())
}

/// The desk for the repository `thread_id`'s PR link names.
async fn thread_desk(
    window: &crate::HomeWindow,
    state: &State<'_, AppState>,
    thread_id: &str,
) -> Result<ReviewDesk, String> {
    let link = storage::read_pr_link(&state.ctx(window).database(), thread_id)
        .await?
        .ok_or_else(|| "This thread is not filed under a pull request".to_string())?;
    Ok(desk(window, state, link.repo_key))
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_checkout_state(
    repo_dir: String,
    target: CheckoutTarget,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<PrCheckout, String> {
    desk(&window, &state, repo_dir).checkout_state(target).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_plan_step(
    repo_dir: String,
    target: CheckoutTarget,
    action: DeskAction,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<StepPlan, String> {
    desk(&window, &state, repo_dir)
        .plan_step(target, action)
        .await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_ensure_review_checkout(
    repo_dir: String,
    target: CheckoutTarget,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<PrCheckout, String> {
    desk(&window, &state, repo_dir)
        .ensure_review_checkout(target)
        .await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_run_checkout_setup(
    repo_dir: String,
    number: i64,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    desk(&window, &state, repo_dir)
        .run_checkout_setup(number)
        .await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_promote_checkout(
    repo_dir: String,
    target: CheckoutTarget,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<PrCheckout, String> {
    desk(&window, &state, repo_dir)
        .promote_review_checkout(target)
        .await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_create_pr_worktree(
    repo_dir: String,
    target: CheckoutTarget,
    path: Option<String>,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<PrCheckout, String> {
    desk(&window, &state, repo_dir)
        .create_pr_worktree(target, path)
        .await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_remove_review_checkout(
    repo_dir: String,
    number: i64,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    desk(&window, &state, repo_dir)
        .remove_review_checkout(number)
        .await
}

/// Drop what closed PRs left behind, given `open_numbers`: every open PR
/// from a complete inbox sync of `repo_dir`.
#[tauri::command]
#[specta::specta]
pub(crate) async fn review_prune_closed(
    repo_dir: String,
    open_numbers: Vec<i64>,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<PruneReport, String> {
    let _t = crate::util::timing::phase(format!("review_prune_closed {repo_dir}"));
    desk(&window, &state, repo_dir)
        .prune_closed(open_numbers.into_iter().collect())
        .await
}

/// Set the review checkout policy and "Don't ask again for <repository>",
/// keeping the repository's other settings.
#[tauri::command]
#[specta::specta]
pub(crate) async fn review_set_checkout_policy(
    repo_dir: String,
    policy: ReviewCheckoutPolicy,
    skip_confirm: bool,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let database = state.ctx(&window).database();
    let mut settings = storage::read_repository_settings(&database, &repo_dir).await?;
    settings.review_checkout_policy = policy;
    settings.skip_checkout_confirm = skip_confirm;
    storage::write_repository_settings(&database, &settings).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_link_thread(
    repo_dir: String,
    thread_id: String,
    target: CheckoutTarget,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<PrLink, String> {
    desk(&window, &state, repo_dir)
        .link_thread(thread_id, target)
        .await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_pr_links(
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<Vec<PrLink>, String> {
    storage::read_pr_links(&state.ctx(&window).database()).await
}

/// File a thread under PR `number` instead (or under none), keeping its
/// recorded start.
#[tauri::command]
#[specta::specta]
pub(crate) async fn review_move_thread_to_pr(
    thread_id: String,
    number: Option<i64>,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    thread_desk(&window, &state, &thread_id)
        .await?
        .move_thread_to_pr(thread_id, number)
        .await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_branch_progress(
    thread_id: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<BranchProgress, String> {
    thread_desk(&window, &state, &thread_id)
        .await?
        .branch_progress(thread_id)
        .await
}

/// Check out a thread's start commit (`start`) or its git branch's tip.
#[tauri::command]
#[specta::specta]
pub(crate) async fn review_check_out_thread_commit(
    thread_id: String,
    start: bool,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    thread_desk(&window, &state, &thread_id)
        .await?
        .check_out_thread_commit(thread_id, start)
        .await
}
