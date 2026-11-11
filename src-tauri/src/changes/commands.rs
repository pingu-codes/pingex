//! The Changes page's commands: the segment overview for a thread, and each
//! segment's patches on demand.

use std::path::Path;

use tauri::State;
use turso::Database;

use super::discover::{resolve_roots, thread_dirs, Repo};
use super::listing::{committed_diff, working_diff};
use super::segments::{branch_segments, inspect, thread_segments, PrHint, RepoFacts, RepoState};
use super::{ChangesScope, RepoChanges, ThreadChanges};
use crate::review::types::{InboxPr, ShaDiff};
use crate::storage::{self, PrLink, TipSnapshot};
use crate::util::host::Host;
use crate::AppState;

const PROVIDER: &str = "github";

/// A repository read, with what Pingex knows about it, or why it could not be.
type RepoEntry = Result<(RepoState, RepoFacts), String>;

/// What `thread_id` has changed in each repository it works in, split into
/// Change segments. `cwd` is the thread's directory as the caller knows it;
/// a workspace thread reads its members instead.
#[tauri::command]
#[specta::specta]
pub(crate) async fn git_thread_changes(
    thread_id: String,
    cwd: Option<String>,
    scope: ChangesScope,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<ThreadChanges, String> {
    let ctx = state.ctx(&window);
    let database = ctx.database();
    let host = ctx.host();
    let dirs = thread_dirs(&database, &thread_id, cwd).await?;
    let inspect_host = host.clone();
    let inspected = tauri::async_runtime::spawn_blocking(move || {
        resolve_roots(&inspect_host, &dirs)
            .into_iter()
            .map(|repo| {
                let state = inspect(&inspect_host, &repo.root);
                (repo, state)
            })
            .collect::<Vec<_>>()
    })
    .await
    .map_err(|_| "Could not read the thread's repositories".to_string())?;

    let link = storage::read_pr_link(&database, &thread_id)
        .await
        .ok()
        .flatten();
    let mut work: Vec<(Repo, RepoEntry)> = Vec::new();
    for (repo, state) in inspected {
        let entry = match state {
            Ok(state) => {
                let facts =
                    facts_for(&database, &thread_id, &repo, &state, link.as_ref(), scope).await;
                Ok((state, facts))
            }
            Err(error) => Err(error),
        };
        work.push((repo, entry));
    }
    let snapshot_at = work
        .iter()
        .filter_map(|(_, entry)| entry.as_ref().ok())
        .filter_map(|(_, facts)| facts.snapshot.as_ref())
        .map(|snapshot| snapshot.taken_at)
        .filter(|at| *at > 0)
        .min();
    let repos = tauri::async_runtime::spawn_blocking(move || {
        work.into_iter()
            .map(|(repo, entry)| build_repo(&host, repo, entry, scope))
            .collect::<Vec<_>>()
    })
    .await
    .map_err(|_| "Could not read the thread's changes".to_string())?;
    Ok(ThreadChanges {
        scope,
        repos,
        snapshot_at,
        read_at: crate::util::time::unix_millis(),
    })
}

fn build_repo(host: &Host, repo: Repo, entry: RepoEntry, scope: ChangesScope) -> RepoChanges {
    let (state, facts) = match entry {
        Ok(entry) => entry,
        Err(error) => {
            return RepoChanges {
                root: repo.root,
                label: repo.label,
                branch: None,
                head_sha: None,
                in_progress: None,
                purpose: None,
                segments: Vec::new(),
                error: Some(error),
                note: None,
            }
        }
    };
    let (segments, note) = match scope {
        ChangesScope::Branch => (branch_segments(host, &state, &facts), None),
        ChangesScope::Thread => match thread_segments(host, &state, &facts) {
            Some(segments) => (segments, None),
            None => (
                // Without start tips, the working tree is all that can be
                // told apart.
                branch_segments(host, &state, &RepoFacts::default())
                    .into_iter()
                    .filter(|segment| segment.id == "uncommitted")
                    .collect(),
                Some("Branch tips are recorded from this thread's next turn".to_string()),
            ),
        },
    };
    RepoChanges {
        root: repo.root,
        label: repo.label,
        branch: state.branch,
        head_sha: state.head_sha,
        in_progress: state.in_progress,
        purpose: facts.purpose,
        segments,
        error: None,
        note,
    }
}

/// The stack, PR and start tips Pingex knows for one repository. Every read
/// is best effort: a missing fact only makes the split coarser.
async fn facts_for(
    database: &Database,
    thread_id: &str,
    repo: &Repo,
    state: &RepoState,
    link: Option<&PrLink>,
    scope: ChangesScope,
) -> RepoFacts {
    let purpose = storage::read_worktree_purpose(database, &repo.root)
        .await
        .ok()
        .flatten()
        .filter(|purpose| purpose.holds_on(state.branch.as_deref()));
    let link_here = link.filter(|link| {
        link.checkout_path
            .as_deref()
            .is_some_and(|path| same_path(path, &repo.root))
    });
    let number = match &purpose {
        Some(crate::review::worktree_purpose::WorktreePurpose::ManagedPr { number, .. }) => {
            Some(*number)
        }
        _ => link_here.map(|link| link.pr_number),
    };
    let pr = pr_hint(database, state, number).await;
    let (snapshot, partial_snapshot) = match scope {
        ChangesScope::Branch => (None, false),
        ChangesScope::Thread => {
            match storage::read_tip_snapshot(database, thread_id, &repo.root)
                .await
                .ok()
                .flatten()
            {
                Some(snapshot) => (Some(snapshot), false),
                None => (link_here.and_then(link_snapshot), true),
            }
        }
    };
    RepoFacts {
        purpose,
        pr,
        snapshot,
        partial_snapshot,
    }
}

fn same_path(a: &str, b: &str) -> bool {
    a.trim_end_matches(['/', '\\']) == b.trim_end_matches(['/', '\\'])
}

/// A PR thread made before start tips existed still knows the commit it
/// started on.
fn link_snapshot(link: &PrLink) -> Option<TipSnapshot> {
    let (branch, sha) = (link.git_branch.as_ref()?, link.start_sha.as_ref()?);
    Some(TipSnapshot {
        root: link.checkout_path.clone().unwrap_or_default(),
        head_sha: Some(sha.clone()),
        head_branch: Some(branch.clone()),
        dirty_paths: Vec::new(),
        branches: vec![(format!("refs/heads/{branch}"), sha.clone())],
        taken_at: link.created_at,
    })
}

/// The open PR this git branch is, from the last inbox read of any GitHub
/// remote: by number when the thread or worktree names one, else by head.
async fn pr_hint(database: &Database, state: &RepoState, number: Option<i64>) -> Option<PrHint> {
    for (remote, slug) in &state.slugs {
        let Some(payload) = storage::read_inbox_snapshot_any_case(database, PROVIDER, slug)
            .await
            .ok()
            .flatten()
        else {
            continue;
        };
        let Ok(prs) = serde_json::from_str::<Vec<InboxPr>>(&payload) else {
            continue;
        };
        let found = prs.iter().find(|pr| match number {
            Some(number) => pr.number == number,
            None => state.branch.as_deref() == Some(pr.head_ref.as_str()),
        });
        if let Some(pr) = found {
            return Some(PrHint {
                number: pr.number,
                base_ref: pr.base_ref.clone(),
                remote: Some(remote.clone()),
            });
        }
    }
    None
}

/// A committed segment's patches: `head` against `base`, straight (two-dot),
/// by SHA.
#[tauri::command]
#[specta::specta]
pub(crate) async fn git_segment_diff(
    repo_dir: String,
    base_sha: String,
    head_sha: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<ShaDiff, String> {
    let host = state.ctx(&window).host();
    tauri::async_runtime::spawn_blocking(move || {
        committed_diff(&host, Path::new(&repo_dir), &base_sha, &head_sha)
    })
    .await
    .map_err(|_| "Diff failed".to_string())?
}

/// The working tree's patches against `base_sha`, untracked files included.
#[tauri::command]
#[specta::specta]
pub(crate) async fn git_working_diff(
    repo_dir: String,
    base_sha: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<ShaDiff, String> {
    let host = state.ctx(&window).host();
    tauri::async_runtime::spawn_blocking(move || {
        working_diff(&host, Path::new(&repo_dir), &base_sha)
    })
    .await
    .map_err(|_| "Diff failed".to_string())?
}
