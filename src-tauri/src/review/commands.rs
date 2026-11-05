//! The review commands the frontend calls.
//!
//! Every `gh`-backed command runs on a blocking task, since the work is a
//! subprocess. The draft commands touch only the local database.

use std::path::Path;
use tauri::State;

use super::actions::{reply_to_comment, resolve_thread, submit_review};
use super::diff::parse_git_diff;
use super::gh::provider_status;
use super::inbox::{diff_snapshots, fetch_inbox, repo_slug};
use super::queries::{check_freshness, fetch_pr_detail, list_open_prs, local_diff};
use super::stacks::list_stacks;
use super::types::{
    ChangesSince, FetchedPr, Inbox, InboxPr, PendingComment, PrDetail, PrFile, PrFileContents,
    PrFreshness, PrSummary, ProviderStatus, ShaDiff, StackListing,
};
use crate::git::objects;
use crate::storage::{self, ReviewDraft};
use crate::util::host::Host;
use crate::AppState;

/// Run `work` on a blocking task with the window's Home host, since every
/// review read is a `gh` or `git` subprocess.
async fn blocking<T, F>(
    window: &crate::HomeWindow,
    state: &State<'_, AppState>,
    failed: &str,
    work: F,
) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(&Host) -> Result<T, String> + Send + 'static,
{
    let host = state.ctx(window).host();
    tauri::async_runtime::spawn_blocking(move || work(&host))
        .await
        .map_err(|_| failed.to_string())?
}

fn pr_number(number: i64) -> Result<u32, String> {
    u32::try_from(number)
        .ok()
        .filter(|number| *number > 0)
        .ok_or_else(|| "That is not a pull request number".to_string())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_provider_status(
    repo_dir: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<ProviderStatus, String> {
    blocking(&window, &state, "Provider check failed", move |host| {
        Ok(provider_status(host, Path::new(&repo_dir)))
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_list_prs(
    repo_dir: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<Vec<PrSummary>, String> {
    blocking(
        &window,
        &state,
        "Pull-request listing failed",
        move |host| list_open_prs(host, Path::new(&repo_dir)),
    )
    .await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_pr_detail(
    repo_dir: String,
    number: i64,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<PrDetail, String> {
    blocking(&window, &state, "Pull-request fetch failed", move |host| {
        fetch_pr_detail(host, Path::new(&repo_dir), number)
    })
    .await
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
    blocking(&window, &state, "Freshness check failed", move |host| {
        check_freshness(
            host,
            Path::new(&repo_dir),
            number,
            &known_head,
            &known_updated_at,
        )
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn review_local_diff(
    repo_dir: String,
    base: String,
    head: Option<String>,
    window: crate::HomeWindow,
    state: State<'_, crate::AppState>,
) -> Result<Vec<PrFile>, String> {
    blocking(&window, &state, "Local diff failed", move |host| {
        local_diff(host, Path::new(&repo_dir), &base, head.as_deref())
    })
    .await
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
    blocking(&window, &state, "Review submission failed", move |host| {
        submit_review(
            host,
            Path::new(&repo_dir),
            number,
            &event,
            &body,
            commit_id.as_deref(),
            &comments,
        )
    })
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
    blocking(&window, &state, "Reply failed", move |host| {
        reply_to_comment(host, Path::new(&repo_dir), number, comment_id, &body)
    })
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
    blocking(&window, &state, "Resolve failed", move |host| {
        resolve_thread(host, Path::new(&repo_dir), &thread_id)
    })
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
    let read = blocking(&window, &state, "Inbox sync failed", move |host| {
        fetch_inbox(host, Path::new(&repo_dir))
    })
    .await?;
    let database = state.ctx(&window).database();
    let previous: Option<Vec<InboxPr>> =
        storage::read_inbox_snapshot(&database, "github", &read.repo)
            .await?
            .and_then(|payload| serde_json::from_str(&payload).ok());
    let events = previous
        .map(|previous| diff_snapshots(&previous, &read.prs, &read.viewer))
        .unwrap_or_default();
    let payload =
        serde_json::to_string(&read.prs).map_err(|_| "Could not store the inbox".to_string())?;
    storage::write_inbox_snapshot(&database, "github", &read.repo, &payload).await?;
    Ok(Inbox {
        repo: read.repo,
        viewer: read.viewer,
        prs: read.prs,
        events,
        rate_limit: read.rate_limit,
        complete: read.complete,
        synced_at: crate::util::time::unix_millis(),
    })
}

/// The repository's native stacks (bottom PR first), or `supported: false`.
#[tauri::command]
#[specta::specta]
pub(crate) async fn review_stacks(
    repo_dir: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<StackListing, String> {
    blocking(&window, &state, "Stack listing failed", move |host| {
        let dir = Path::new(&repo_dir);
        let (owner, name) = repo_slug(host, dir)?;
        list_stacks(host, dir, &format!("{owner}/{name}"))
    })
    .await
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
    blocking(
        &window,
        &state,
        "Fetching pull requests failed",
        move |host| {
            let dir = Path::new(&repo_dir);
            let numbers = numbers
                .into_iter()
                .map(pr_number)
                .collect::<Result<Vec<_>, _>>()?;
            let (owner, name) = repo_slug(host, dir)?;
            let slug = format!("{owner}/{name}");
            let remote = objects::remote_for_slug(host, dir, &slug)?
                .ok_or_else(|| format!("No remote in this repository points at {slug}"))?;
            Ok(objects::fetch_pr_heads(host, dir, &remote, &numbers)?
                .into_iter()
                .map(|fetched| FetchedPr {
                    number: i64::from(fetched.number),
                    head_sha: fetched.sha,
                })
                .collect())
        },
    )
    .await
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
    blocking(&window, &state, "Diff failed", move |host| {
        let patch = objects::diff_shas(host, Path::new(&repo_dir), &base_sha, &head_sha)?;
        Ok(ShaDiff {
            files: parse_git_diff(&patch.text),
            truncated: patch.truncated,
        })
    })
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
    blocking(&window, &state, "Reading the file failed", move |host| {
        let blob = objects::read_blob(host, Path::new(&repo_dir), &sha, &path)?;
        Ok(PrFileContents {
            text: blob.text,
            size: i64::try_from(blob.size).unwrap_or(i64::MAX),
            truncated: blob.truncated,
        })
    })
    .await
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
    blocking(&window, &state, "Reading changes failed", move |host| {
        changes_since(
            host,
            Path::new(&repo_dir),
            pr_number(number)?,
            &base_sha,
            &head_sha,
        )
    })
    .await
}

fn changes_since(
    host: &Host,
    dir: &Path,
    number: u32,
    base: &str,
    head: &str,
) -> Result<ChangesSince, String> {
    let seen = objects::seen_head(host, dir, number)?;
    let Some(seen_sha) = seen.clone() else {
        // Never seen: everything is new.
        let patch = objects::diff_shas(host, dir, base, head)?;
        return Ok(ChangesSince {
            seen_sha: None,
            head_sha: head.to_string(),
            fast_forward: false,
            files: parse_git_diff(&patch.text),
            range_diff: None,
            truncated: patch.truncated,
        });
    };
    let fast_forward = objects::is_ancestor(host, dir, &seen_sha, head)?;
    let patch = objects::diff_between(host, dir, &seen_sha, head)?;
    let range_diff = if fast_forward {
        None
    } else {
        Some(objects::range_diff(host, dir, base, &seen_sha, head)?.text)
    };
    Ok(ChangesSince {
        seen_sha: seen,
        head_sha: head.to_string(),
        fast_forward,
        files: parse_git_diff(&patch.text),
        range_diff,
        truncated: patch.truncated,
    })
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
    blocking(&window, &state, "Marking seen failed", move |host| {
        objects::mark_seen(host, Path::new(&repo_dir), pr_number(number)?, &sha)
    })
    .await
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
    blocking(&window, &state, "Cleaning up failed", move |host| {
        objects::prune_pr_refs(host, Path::new(&repo_dir), pr_number(number)?)
    })
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
