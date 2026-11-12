//! Commands for PR worktrees and the PR link on threads: thin wrappers
//! over `desk::ReviewDesk`, like `commands`. Thread-scoped commands find the
//! repository from the thread's PR link.

use std::path::PathBuf;
use std::sync::Arc;
use tauri::State;

use super::checkout_types::{
    BranchProgress, CheckOutOutcome, ManagedPrWorktree, PrCheckout, PruneReport,
};
use super::desk::ReviewDesk;
use super::github::GhCli;
use super::worktree_context::WorktreeContext;
use super::worktree_purpose::WorktreePurpose;
use crate::storage::{self, PrLink};
use crate::AppState;

/// The desk for `repo_dir` on the window's Home, with the Home's directory
/// for PR worktrees.
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

/// Check out PR `number`: its own worktree for a lone PR, the whole stack's
/// for a stack's PR. `BranchInUse` when its git branch is checked out
/// somewhere already; `review_adopt_worktree` then takes that.
#[tauri::command]
#[specta::specta]
pub(crate) async fn review_check_out(
    repo_dir: String,
    number: i64,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<CheckOutOutcome, String> {
    desk(&window, &state, repo_dir).check_out(number).await
}

/// Check out every open PR of GitHub stack `stack` in one worktree.
#[tauri::command]
#[specta::specta]
pub(crate) async fn review_check_out_stack(
    repo_dir: String,
    stack: i64,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<CheckOutOutcome, String> {
    desk(&window, &state, repo_dir).check_out_stack(stack).await
}

/// Make the worktree at `path` PR `number`'s (its stack's, for a stack's
/// PR), as Check out found it. Pingex never switches, fast-forwards or
/// removes an adopted worktree.
#[tauri::command]
#[specta::specta]
pub(crate) async fn review_adopt_worktree(
    repo_dir: String,
    number: i64,
    path: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<PrCheckout, String> {
    desk(&window, &state, repo_dir)
        .adopt_worktree(number, path)
        .await
}

/// Put the stack worktree PR `number` is checked out in on its git branch.
#[tauri::command]
#[specta::specta]
pub(crate) async fn review_switch_stack_branch(
    repo_dir: String,
    number: i64,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<PrCheckout, String> {
    desk(&window, &state, repo_dir)
        .switch_stack_branch(number)
        .await
}

/// PR `number`'s worktree, or `None` when it is not checked out. Changes
/// nothing.
#[tauri::command]
#[specta::specta]
pub(crate) async fn review_pr_checkout(
    repo_dir: String,
    number: i64,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<Option<PrCheckout>, String> {
    desk(&window, &state, repo_dir).pr_checkout(number).await
}

/// Every checked-out PR of `repo_dir`, for the inbox and the Stack page.
/// Changes nothing.
#[tauri::command]
#[specta::specta]
pub(crate) async fn review_pr_checkouts(
    repo_dir: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<Vec<PrCheckout>, String> {
    desk(&window, &state, repo_dir).pr_checkouts().await
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

/// Undo checking PR `number` out: remove the worktree Pingex made for it
/// (its stack's, for a stack's PR). Refused when it holds work.
#[tauri::command]
#[specta::specta]
pub(crate) async fn review_remove_pr_worktree(
    repo_dir: String,
    number: i64,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    desk(&window, &state, repo_dir)
        .remove_pr_worktree(number)
        .await
}

/// Push PR `number`'s worktree to the PR's head branch, without forcing.
#[tauri::command]
#[specta::specta]
pub(crate) async fn review_push_to_pr(
    repo_dir: String,
    number: i64,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    desk(&window, &state, repo_dir).push_to_pr(number).await
}

/// The lone PR of the worktree Pingex made at `dir`, if that is what it is.
#[tauri::command]
#[specta::specta]
pub(crate) async fn review_managed_worktree_at(
    dir: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<Option<ManagedPrWorktree>, String> {
    let Some((repo_dir, number)) =
        storage::read_checkout_at_path(&state.ctx(&window).database(), &dir).await?
    else {
        return Ok(None);
    };
    let managed = desk(&window, &state, repo_dir.clone())
        .is_managed_worktree(dir)
        .await?;
    Ok(managed.then_some(ManagedPrWorktree { repo_dir, number }))
}

/// What to tell an agent starting a thread in `dir`, when `dir` is a PR
/// worktree or a Stack worktree.
#[tauri::command]
#[specta::specta]
pub(crate) async fn review_worktree_context(
    dir: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<Option<WorktreeContext>, String> {
    let Some(purpose) =
        storage::read_worktree_purpose(&state.ctx(&window).database(), &dir).await?
    else {
        return Ok(None);
    };
    let (WorktreePurpose::ManagedPr { repo_dir, .. } | WorktreePurpose::Stack { repo_dir, .. }) =
        purpose;
    desk(&window, &state, repo_dir).worktree_context(dir).await
}

/// Drop what closed PRs left behind, given `open_numbers`: every open PR
/// an inbox sync of `repo_dir` listed, and whether that sync was `complete`.
/// An incomplete sync prunes nothing.
#[tauri::command]
#[specta::specta]
pub(crate) async fn review_prune_closed(
    repo_dir: String,
    open_numbers: Vec<i64>,
    complete: bool,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<PruneReport, String> {
    let _t = crate::util::timing::phase(format!("review_prune_closed {repo_dir}"));
    desk(&window, &state, repo_dir)
        .prune_closed(open_numbers.into_iter().collect(), complete)
        .await
}

/// File `thread_id`, just started in PR `number`'s worktree, under the PR.
#[tauri::command]
#[specta::specta]
pub(crate) async fn review_link_thread(
    repo_dir: String,
    thread_id: String,
    number: i64,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<PrLink, String> {
    let created = desk(&window, &state, repo_dir)
        .link_thread(thread_id.clone(), number)
        .await?;
    // The thread's start tips were taken before the worktree caught up with
    // the PR: take them again, from where it starts now.
    if created.moved {
        let ctx = state.ctx(&window);
        storage::delete_tip_snapshots(&ctx.database(), &thread_id).await?;
        crate::changes::snapshot::record_start_tips(
            &ctx,
            &thread_id,
            created.link.checkout_path.clone(),
        )
        .await;
    }
    Ok(created.link)
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
