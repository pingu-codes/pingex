//! Recording a thread's start tips before it works in a repository.
//!
//! Runs at thread start and before every turn: only repositories the thread
//! has no tips for yet are read, so after the first turn this costs one
//! `rev-parse` per directory. It never holds a turn up for long: past a short
//! wait the turn goes ahead and the recording finishes behind it.

use std::path::Path;
use std::time::Duration;

use turso::Database;

use super::discover::{resolve_roots, thread_dirs};
use super::listing::dirty_paths;
use super::refs::{current_branch, empty_tree, head_sha, read_refs};
use crate::storage::{self, TipSnapshot};
use crate::util::host::Host;
use crate::HomeContext;

/// How long a turn waits for its start tips before going ahead.
const WAIT: Duration = Duration::from_secs(2);

/// Every branch tip of the repository at `root`, `HEAD`, and the paths
/// already dirty.
pub(super) fn take(host: &Host, root: &str) -> Result<TipSnapshot, String> {
    let path = Path::new(root);
    let refs = read_refs(host, path)?;
    let head = head_sha(host, path);
    let base = match &head {
        Some(sha) => sha.clone(),
        None => empty_tree(host, path)?,
    };
    Ok(TipSnapshot {
        root: root.to_string(),
        head_branch: current_branch(host, path),
        dirty_paths: dirty_paths(host, path, &base),
        branches: refs
            .heads
            .iter()
            .map(|(name, sha)| (format!("refs/heads/{name}"), sha.clone()))
            .collect(),
        head_sha: head,
        taken_at: 0,
    })
}

async fn record(
    database: Database,
    host: Host,
    thread_id: String,
    cwd: Option<String>,
) -> Result<(), String> {
    let dirs = thread_dirs(&database, &thread_id, cwd).await?;
    if dirs.is_empty() {
        return Ok(());
    }
    let recorded = storage::recorded_snapshot_roots(&database, &thread_id).await?;
    let snapshots = tauri::async_runtime::spawn_blocking(move || {
        resolve_roots(&host, &dirs)
            .into_iter()
            .filter(|repo| !recorded.contains(&repo.root))
            .filter_map(|repo| take(&host, &repo.root).ok())
            .collect::<Vec<_>>()
    })
    .await
    .map_err(|_| "Could not read the thread's repositories".to_string())?;
    for snapshot in &snapshots {
        storage::write_tip_snapshot(&database, &thread_id, snapshot).await?;
    }
    Ok(())
}

/// Record the start tips of every repository `thread_id` works in that has
/// none yet. Best effort: a failure is logged, never returned, and the
/// caller waits at most `WAIT`.
pub(crate) async fn record_start_tips(ctx: &HomeContext, thread_id: &str, cwd: Option<String>) {
    let task = tauri::async_runtime::spawn(record(
        ctx.database(),
        ctx.host(),
        thread_id.to_string(),
        cwd,
    ));
    // Past the wait, or failed to join: the recording carries on (or is lost)
    // without holding the turn.
    if let Ok(Ok(Err(error))) = tokio::time::timeout(WAIT, task).await {
        eprintln!("[changes] start tips not recorded: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::test_repo::TestRepo;

    #[tokio::test]
    async fn start_tips_are_recorded_once_per_repository() {
        let Some(fixture) = TestRepo::new() else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let database = crate::storage::open(dir.path()).await.unwrap();
        let cwd = fixture.repo.to_str().unwrap().to_string();
        std::fs::write(fixture.repo.join("README.md"), "dirty\n").unwrap();
        record(
            database.clone(),
            Host::Native,
            "t".into(),
            Some(cwd.clone()),
        )
        .await
        .unwrap();
        let roots = storage::recorded_snapshot_roots(&database, "t")
            .await
            .unwrap();
        assert_eq!(roots.len(), 1);
        let first = storage::read_tip_snapshot(&database, "t", &roots[0])
            .await
            .unwrap()
            .unwrap();
        assert_eq!(first.head_branch.as_deref(), Some("main"));
        assert_eq!(first.dirty_paths, vec!["README.md".to_string()]);
        assert_eq!(first.branches.len(), 1);

        fixture.commit("later.txt", "1\n", "later");
        record(database.clone(), Host::Native, "t".into(), Some(cwd))
            .await
            .unwrap();
        let again = storage::read_tip_snapshot(&database, "t", &roots[0])
            .await
            .unwrap()
            .unwrap();
        assert_eq!(again.head_sha, first.head_sha);
    }
}
