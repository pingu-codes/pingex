//! Commands for the Stack page: thin wrappers over `desk::ReviewDesk`'s
//! stack operations. Each takes the stack as the inbox has it
//! (`StackInput`), so nothing here re-reads GitHub to find the stack.

use std::path::PathBuf;
use std::sync::Arc;
use tauri::State;

use super::desk::ReviewDesk;
use super::github::GhCli;
use super::stack_types::{
    PrBaseInput, PrBehind, PushReport, RestackPlan, RestackProgress, SplitRequest, StackInput,
    StackOverview,
};
use crate::AppState;

fn desk(window: &crate::HomeWindow, state: &State<'_, AppState>, repo_dir: String) -> ReviewDesk {
    let ctx = state.ctx(window);
    let runtime = ctx.runtime();
    let host = runtime.host.clone();
    let dir = PathBuf::from(repo_dir);
    let github = Arc::new(GhCli::new(host.clone(), dir.clone()));
    ReviewDesk::new(host, dir, github, ctx.database()).with_home_dir(runtime.codex_home_str())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_stack_overview(
    repo_dir: String,
    input: StackInput,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<StackOverview, String> {
    desk(&window, &state, repo_dir).stack_overview(input).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_prs_behind(
    repo_dir: String,
    prs: Vec<PrBaseInput>,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<Vec<PrBehind>, String> {
    desk(&window, &state, repo_dir).prs_behind(prs).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_restack_plan(
    repo_dir: String,
    input: StackInput,
    from: Option<i64>,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<RestackPlan, String> {
    desk(&window, &state, repo_dir)
        .restack_plan(input, from)
        .await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_restack(
    repo_dir: String,
    input: StackInput,
    from: Option<i64>,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<RestackProgress, String> {
    desk(&window, &state, repo_dir).restack(input, from).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_continue_restack(
    repo_dir: String,
    stack: i64,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<RestackProgress, String> {
    desk(&window, &state, repo_dir)
        .continue_restack(stack)
        .await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_abort_restack(
    repo_dir: String,
    stack: i64,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    desk(&window, &state, repo_dir).abort_restack(stack).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_push_stack(
    repo_dir: String,
    input: StackInput,
    only: Option<i64>,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<PushReport, String> {
    desk(&window, &state, repo_dir)
        .push_stack(input, only)
        .await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_reorder_stack(
    repo_dir: String,
    input: StackInput,
    order: Vec<i64>,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    desk(&window, &state, repo_dir)
        .reorder_stack(input, order)
        .await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_fold_into_parent(
    repo_dir: String,
    input: StackInput,
    number: i64,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    desk(&window, &state, repo_dir)
        .fold_into_parent(input, number)
        .await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_remove_from_stack(
    repo_dir: String,
    input: StackInput,
    number: i64,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    desk(&window, &state, repo_dir)
        .remove_from_stack(input, number)
        .await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_split_branch(
    repo_dir: String,
    input: StackInput,
    number: i64,
    split: SplitRequest,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<i64, String> {
    desk(&window, &state, repo_dir)
        .split_branch(input, number, split)
        .await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_rename_stack_branch(
    repo_dir: String,
    input: StackInput,
    number: i64,
    to: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    desk(&window, &state, repo_dir)
        .rename_stack_branch(input, number, to)
        .await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_add_stack_branch(
    repo_dir: String,
    input: StackInput,
    name: String,
    below: Option<i64>,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    desk(&window, &state, repo_dir)
        .add_stack_branch(input, name, below)
        .await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_delete_merged_branches(
    repo_dir: String,
    input: StackInput,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    desk(&window, &state, repo_dir)
        .delete_merged_branches(input)
        .await
}
