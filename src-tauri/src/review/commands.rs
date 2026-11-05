//! The review commands the frontend calls: thin wrappers over the review
//! desk service (`desk::ReviewDesk`), built per call for the window's Home.
//! The draft commands touch only the local database.

use std::path::PathBuf;
use std::sync::Arc;
use tauri::State;

use super::desk::ReviewDesk;
use super::github::GhCli;
use super::repository::{FolderFacts, KnownClone, Repository};
use super::types::{
    ChangesSince, CheckLog, CommentRef, FetchedPr, GitHubAccount, GitHubRepo, Inbox, MergeRequest,
    NewPr, PendingComment, PrDetail, PrFile, PrFileContents, PrFreshness, PrSummary,
    ProviderStatus, ReviewRequests, ReviewSubmission, ReviewVerdict, ShaDiff, StackListing,
};
use crate::storage::{self, RepositorySettings, ReviewDraft};
use crate::util::host::Host;
use crate::AppState;

/// The desk for the repository at `repo_dir` on the window's Home.
fn desk(window: &crate::HomeWindow, state: &State<'_, AppState>, repo_dir: String) -> ReviewDesk {
    let ctx = state.ctx(window);
    let host = ctx.host();
    let dir = PathBuf::from(repo_dir);
    let github = Arc::new(GhCli::new(host.clone(), dir.clone()));
    ReviewDesk::new(host, dir, github, ctx.database())
}

/// A desk for the account-level reads, which need no repository: `gh` runs
/// in the Home's home directory.
fn account_desk(window: &crate::HomeWindow, state: &State<'_, AppState>) -> ReviewDesk {
    let home = state
        .ctx(window)
        .host()
        .home_dir()
        .unwrap_or_else(|| "/".to_string());
    desk(window, state, home)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_provider_status(
    repo_dir: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<ProviderStatus, String> {
    desk(&window, &state, repo_dir).provider_status().await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_list_prs(
    repo_dir: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<Vec<PrSummary>, String> {
    desk(&window, &state, repo_dir).list_prs().await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_pr_detail(
    repo_dir: String,
    number: i64,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<PrDetail, String> {
    desk(&window, &state, repo_dir).pr_detail(number).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_check_fresh(
    repo_dir: String,
    number: i64,
    known_head: String,
    known_updated_at: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<PrFreshness, String> {
    desk(&window, &state, repo_dir)
        .check_fresh(number, known_head, known_updated_at)
        .await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_local_diff(
    repo_dir: String,
    base: String,
    head: Option<String>,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<Vec<PrFile>, String> {
    desk(&window, &state, repo_dir).local_diff(base, head).await
}

/// Submit a review. `commit_id` is the head SHA the reviewer read; pass it so
/// the review and its inline comments stay on that commit even if the author
/// pushes before submit.
#[tauri::command]
#[specta::specta]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn review_submit(
    repo_dir: String,
    number: i64,
    event: String,
    body: String,
    commit_id: Option<String>,
    comments: Vec<PendingComment>,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let review = ReviewSubmission {
        verdict: ReviewVerdict::from_event(&event),
        body,
        commit_id,
        comments,
    };
    desk(&window, &state, repo_dir)
        .submit_review(number, review)
        .await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_reply(
    repo_dir: String,
    number: i64,
    comment_id: i64,
    body: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    desk(&window, &state, repo_dir)
        .reply(number, comment_id, body)
        .await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_resolve_thread(
    repo_dir: String,
    thread_id: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    desk(&window, &state, repo_dir)
        .resolve_thread(thread_id)
        .await
}

/// Sync a repository's inbox: every open PR in one paginated query, plus what
/// changed since the previous sync (compared with the stored snapshot).
#[tauri::command]
#[specta::specta]
pub(crate) async fn review_inbox(
    repo_dir: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<Inbox, String> {
    desk(&window, &state, repo_dir).sync_inbox().await
}

/// The repository's native stacks (bottom PR first), or `supported: false`.
#[tauri::command]
#[specta::specta]
pub(crate) async fn review_stacks(
    repo_dir: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<StackListing, String> {
    desk(&window, &state, repo_dir).stacks().await
}

/// Fetch PR heads into `refs/pingex/pr/<n>/head`, in one round trip, from the
/// remote that points at the repository's GitHub repository. No working tree
/// changes.
#[tauri::command]
#[specta::specta]
pub(crate) async fn review_fetch_prs(
    repo_dir: String,
    numbers: Vec<i64>,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<Vec<FetchedPr>, String> {
    desk(&window, &state, repo_dir).fetch_prs(numbers).await
}

/// The PR diff GitHub shows, `head` against its merge base with `base`, read
/// locally by SHA. Fetch the PR first.
#[tauri::command]
#[specta::specta]
pub(crate) async fn review_diff(
    repo_dir: String,
    base_sha: String,
    head_sha: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<ShaDiff, String> {
    desk(&window, &state, repo_dir)
        .diff(base_sha, head_sha)
        .await
}

/// One file as it is at commit `sha`, without checking anything out.
#[tauri::command]
#[specta::specta]
pub(crate) async fn review_file(
    repo_dir: String,
    sha: String,
    path: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<PrFileContents, String> {
    desk(&window, &state, repo_dir).file(sha, path).await
}

/// What PR `number` changed since its head was last marked seen. `base_sha`
/// is the current tip of the PR's base branch, for the range diff after a
/// rebase.
#[tauri::command]
#[specta::specta]
pub(crate) async fn review_changes_since(
    repo_dir: String,
    number: i64,
    base_sha: String,
    head_sha: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<ChangesSince, String> {
    desk(&window, &state, repo_dir)
        .changes_since(number, base_sha, head_sha)
        .await
}

/// Pin `sha` as the head last seen on PR `number`; its commits then survive a
/// force-push for "changes since".
#[tauri::command]
#[specta::specta]
pub(crate) async fn review_mark_seen(
    repo_dir: String,
    number: i64,
    sha: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    desk(&window, &state, repo_dir).mark_seen(number, sha).await
}

/// Drop the refs kept for a closed PR.
#[tauri::command]
#[specta::specta]
pub(crate) async fn review_forget_pr(
    repo_dir: String,
    number: i64,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    desk(&window, &state, repo_dir).forget_pr(number).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_edit_comment(
    repo_dir: String,
    comment: CommentRef,
    body: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    desk(&window, &state, repo_dir)
        .edit_comment(comment, body)
        .await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_delete_comment(
    repo_dir: String,
    comment: CommentRef,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    desk(&window, &state, repo_dir)
        .delete_comment(comment)
        .await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_merge(
    repo_dir: String,
    number: i64,
    request: MergeRequest,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    desk(&window, &state, repo_dir).merge(number, request).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_close(
    repo_dir: String,
    number: i64,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    desk(&window, &state, repo_dir).close(number).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_retarget(
    repo_dir: String,
    number: i64,
    base: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    desk(&window, &state, repo_dir).retarget(number, base).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_mark_ready(
    repo_dir: String,
    number: i64,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    desk(&window, &state, repo_dir).mark_ready(number).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_check_logs(
    repo_dir: String,
    number: i64,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<Vec<CheckLog>, String> {
    desk(&window, &state, repo_dir)
        .failed_check_logs(number)
        .await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_rerun_failed_checks(
    repo_dir: String,
    number: i64,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    desk(&window, &state, repo_dir)
        .rerun_failed_checks(number)
        .await
}

/// Open a PR from a pushed git branch; returns its number.
#[tauri::command]
#[specta::specta]
pub(crate) async fn review_create_pr(
    repo_dir: String,
    pr: NewPr,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<i64, String> {
    desk(&window, &state, repo_dir).create_pr(pr).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_my_repositories(
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<Vec<GitHubRepo>, String> {
    account_desk(&window, &state).my_repositories().await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_repositories_requesting_review(
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<Vec<ReviewRequests>, String> {
    account_desk(&window, &state)
        .repositories_requesting_review()
        .await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_save_draft(
    provider: String,
    repo: String,
    pr_number: i64,
    head_sha: String,
    payload: String,
    window: crate::HomeWindow,
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
#[specta::specta]
pub(crate) async fn review_load_draft(
    provider: String,
    repo: String,
    pr_number: i64,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<Option<ReviewDraft>, String> {
    let ctx = state.ctx(&window);
    storage::read_review_draft(&ctx.database(), &provider, &repo, pr_number).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_delete_draft(
    provider: String,
    repo: String,
    pr_number: i64,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let ctx = state.ctx(&window);
    storage::delete_review_draft(&ctx.database(), &provider, &repo, pr_number).await
}

// --- Repositories (Add repository, the sidebar's repository states) ---

/// A desk over `path` on an explicitly chosen Host, as Add repository picks
/// it: a share path picked in a distribution runs there unless `host` says
/// otherwise.
fn desk_on(
    window: &crate::HomeWindow,
    state: &State<'_, AppState>,
    path: &str,
    host: Option<Host>,
) -> ReviewDesk {
    let (host, dir) = match host {
        Some(host) => {
            let dir = host.to_host_path(path.trim());
            (host, dir)
        }
        None => Host::from_local(path.trim()),
    };
    let dir = PathBuf::from(dir);
    let github = Arc::new(GhCli::new(host.clone(), dir.clone()));
    ReviewDesk::new(host, dir, github, state.ctx(window).database())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_github_account(
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<GitHubAccount, String> {
    account_desk(&window, &state).github_account().await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_switch_github_account(
    login: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    account_desk(&window, &state)
        .switch_github_account(login)
        .await
}

/// What a local folder holds before it is added as a repository.
#[tauri::command]
#[specta::specta]
pub(crate) async fn review_inspect_folder(
    folder: String,
    host: Option<Host>,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<FolderFacts, String> {
    desk_on(&window, &state, &folder, host)
        .inspect_folder()
        .await
}

/// A project's repository state: cloned, or with no GitHub remote feeding it.
#[tauri::command]
#[specta::specta]
pub(crate) async fn review_repository(
    repo_dir: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<Repository, String> {
    desk(&window, &state, repo_dir).repository().await
}

/// `repo_dir` is the clone's path, or a PRs-only repository's slug.
#[tauri::command]
#[specta::specta]
pub(crate) async fn review_repository_settings(
    repo_dir: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<RepositorySettings, String> {
    desk(&window, &state, repo_dir).repository_settings().await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_save_repository_settings(
    repo_dir: String,
    settings: RepositorySettings,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    desk(&window, &state, repo_dir)
        .save_repository_settings(settings)
        .await
}

/// Make the clone at `repo_dir` feed GitHub repository `slug`'s PRs.
#[tauri::command]
#[specta::specta]
pub(crate) async fn review_link_clone(
    repo_dir: String,
    slug: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<Repository, String> {
    desk(&window, &state, repo_dir).link_clone(slug).await
}

/// For each of `slugs`, a clone of it among `cwds` (folders Pingex knows).
#[tauri::command]
#[specta::specta]
pub(crate) async fn review_find_clones(
    slugs: Vec<String>,
    cwds: Vec<String>,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<Vec<KnownClone>, String> {
    account_desk(&window, &state).find_clones(slugs, cwds).await
}

/// Clone `url` into `<parent_dir>/<name>` on `host`; returns the clone's path
/// on that Host, ready to add as a project.
#[tauri::command]
#[specta::specta]
pub(crate) async fn review_clone_repository(
    parent_dir: String,
    url: String,
    name: String,
    host: Option<Host>,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<String, String> {
    desk_on(&window, &state, &parent_dir, host)
        .clone_repository(url, name)
        .await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_prs_only_repositories(
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<Vec<Repository>, String> {
    account_desk(&window, &state).prs_only_repositories().await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_add_prs_only_repository(
    slug: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    account_desk(&window, &state)
        .add_prs_only_repository(slug)
        .await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_remove_prs_only_repository(
    slug: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    account_desk(&window, &state)
        .remove_prs_only_repository(slug)
        .await
}
