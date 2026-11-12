//! What worktrees are for: the stack each Stack worktree holds, by path, and
//! the managed PR worktrees read off `review_checkouts`.
//!
//! `stack_worktrees` is a cache for listings and worktree context. It is
//! written when the desk finds or makes a stack's worktree and dropped with
//! the worktree; a row left behind is harmless, because readers check the
//! branch the worktree is on.

use turso::{params, Database};

use super::db;
use crate::review::worktree_purpose::{StackMarkerPr, WorktreePurpose};
use crate::util::time::unix_secs;

/// Record that the worktree at `path` holds `purpose`'s stack, replacing any
/// other worktree recorded for that stack. Anything but a stack is ignored.
pub(crate) async fn write_stack_worktree(
    database: &Database,
    path: &str,
    purpose: &WorktreePurpose,
) -> Result<(), String> {
    let WorktreePurpose::Stack {
        repo_dir,
        number,
        name,
        base_ref,
        prs,
        local_stack,
    } = purpose
    else {
        return Ok(());
    };
    let prs = serde_json::to_string(prs).map_err(|error| error.to_string())?;
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "DELETE FROM stack_worktrees WHERE repo_key = ? AND stack_number = ? AND path <> ?",
        params![repo_dir.as_str(), *number, path],
    )
    .await?;
    db::exec(
        &connection,
        "INSERT INTO stack_worktrees(path, repo_key, stack_number, stack_name, base_ref, prs, local_stack, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(path) DO UPDATE SET
             repo_key = excluded.repo_key,
             stack_number = excluded.stack_number,
             stack_name = excluded.stack_name,
             base_ref = excluded.base_ref,
             prs = excluded.prs,
             local_stack = excluded.local_stack,
             updated_at = excluded.updated_at",
        params![
            path,
            repo_dir.as_str(),
            *number,
            name.as_str(),
            base_ref.as_str(),
            prs,
            i64::from(*local_stack),
            unix_secs()
        ],
    )
    .await
}

pub(crate) async fn delete_stack_worktree(database: &Database, path: &str) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "DELETE FROM stack_worktrees WHERE path = ?",
        params![path],
    )
    .await
}

/// Drop `repo_key`'s stack worktree rows whose PRs have all closed.
pub(crate) async fn prune_stack_worktrees(
    database: &Database,
    repo_key: &str,
    is_open: impl Fn(i64) -> bool,
) -> Result<(), String> {
    for (path, purpose) in read_stack_worktrees(database).await? {
        let WorktreePurpose::Stack { repo_dir, prs, .. } = purpose else {
            continue;
        };
        if repo_dir == repo_key && !prs.iter().any(|pr| is_open(pr.number)) {
            delete_stack_worktree(database, &path).await?;
        }
    }
    Ok(())
}

async fn read_stack_worktrees(
    database: &Database,
) -> Result<Vec<(String, WorktreePurpose)>, String> {
    let connection = db::conn(database)?;
    db::rows(
        &connection,
        "SELECT path, repo_key, stack_number, stack_name, base_ref, prs, local_stack FROM stack_worktrees",
        (),
        |row| {
            let prs: Vec<StackMarkerPr> =
                serde_json::from_str(&db::text(row, 5)?).unwrap_or_default();
            Ok((
                db::text(row, 0)?,
                WorktreePurpose::Stack {
                    repo_dir: db::text(row, 1)?,
                    number: db::int(row, 2)?,
                    name: db::text(row, 3)?,
                    base_ref: db::text(row, 4)?,
                    prs,
                    local_stack: db::int(row, 6)? != 0,
                },
            ))
        },
    )
    .await
}

/// Every worktree Pingex knows the purpose of, by path. A stack wins over a
/// PR at the same path: a stack's worktree also has a row per PR checked out
/// in it.
pub(crate) async fn read_worktree_purposes(
    database: &Database,
) -> Result<Vec<(String, WorktreePurpose)>, String> {
    let mut purposes = read_stack_worktrees(database).await?;
    let connection = db::conn(database)?;
    let managed = db::rows(
        &connection,
        "SELECT path, repo_key, pr_number FROM review_checkouts
         WHERE git_branch = 'pr/' || pr_number OR git_branch = 'pr/' || pr_number || '-local'",
        (),
        |row| {
            Ok((
                db::text(row, 0)?,
                WorktreePurpose::ManagedPr {
                    repo_dir: db::text(row, 1)?,
                    number: db::int(row, 2)?,
                },
            ))
        },
    )
    .await?;
    for (path, purpose) in managed {
        if !purposes.iter().any(|(known, _)| *known == path) {
            purposes.push((path, purpose));
        }
    }
    Ok(purposes)
}

/// What the worktree at `path` is for, if Pingex knows.
pub(crate) async fn read_worktree_purpose(
    database: &Database,
    path: &str,
) -> Result<Option<WorktreePurpose>, String> {
    Ok(read_worktree_purposes(database)
        .await?
        .into_iter()
        .find(|(known, _)| known == path)
        .map(|(_, purpose)| purpose))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{write_checkout, StoredCheckout};

    fn stack(number: i64, prs: &[i64]) -> WorktreePurpose {
        WorktreePurpose::Stack {
            repo_dir: "/repo".into(),
            number,
            name: "billing".into(),
            base_ref: "main".into(),
            prs: prs
                .iter()
                .map(|number| StackMarkerPr {
                    number: *number,
                    title: format!("PR {number}"),
                    head_ref: format!("feat/{number}"),
                })
                .collect(),
            local_stack: false,
        }
    }

    fn checkout(pr_number: i64, path: &str, branch: &str) -> StoredCheckout {
        StoredCheckout {
            pr_number,
            step: "worktree".into(),
            path: path.into(),
            head_sha: None,
            git_branch: Some(branch.into()),
            setup_done: false,
            closed_at: None,
        }
    }

    #[tokio::test]
    async fn a_stack_has_one_worktree_and_wins_over_its_pr_rows() {
        let dir = tempfile::tempdir().unwrap();
        let database = crate::storage::open(dir.path()).await.unwrap();
        write_stack_worktree(&database, "/wt/a", &stack(7, &[1, 2]))
            .await
            .unwrap();
        write_stack_worktree(&database, "/wt/b", &stack(7, &[1, 2, 3]))
            .await
            .unwrap();
        write_checkout(&database, "/repo", &checkout(2, "/wt/b", "feat/2"))
            .await
            .unwrap();
        assert_eq!(
            read_worktree_purposes(&database).await.unwrap(),
            vec![("/wt/b".to_string(), stack(7, &[1, 2, 3]))]
        );
    }

    #[tokio::test]
    async fn only_a_pr_branch_makes_a_checkout_managed() {
        let dir = tempfile::tempdir().unwrap();
        let database = crate::storage::open(dir.path()).await.unwrap();
        write_checkout(&database, "/repo", &checkout(12, "/wt/pr-12", "pr/12"))
            .await
            .unwrap();
        write_checkout(&database, "/repo", &checkout(13, "/wt/mine", "feat/mine"))
            .await
            .unwrap();
        write_checkout(
            &database,
            "/repo",
            &checkout(14, "/wt/pr-14", "pr/14-local"),
        )
        .await
        .unwrap();
        let managed = |number| WorktreePurpose::ManagedPr {
            repo_dir: "/repo".into(),
            number,
        };
        assert_eq!(
            read_worktree_purpose(&database, "/wt/pr-12").await.unwrap(),
            Some(managed(12))
        );
        assert_eq!(
            read_worktree_purpose(&database, "/wt/pr-14").await.unwrap(),
            Some(managed(14))
        );
        assert_eq!(
            read_worktree_purpose(&database, "/wt/mine").await.unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn a_stack_row_goes_with_its_worktree_or_its_last_open_pr() {
        let dir = tempfile::tempdir().unwrap();
        let database = crate::storage::open(dir.path()).await.unwrap();
        write_stack_worktree(&database, "/wt/a", &stack(7, &[1, 2]))
            .await
            .unwrap();
        write_stack_worktree(&database, "/wt/c", &stack(9, &[5]))
            .await
            .unwrap();
        prune_stack_worktrees(&database, "/repo", |number| number == 2)
            .await
            .unwrap();
        assert_eq!(
            read_worktree_purposes(&database).await.unwrap(),
            vec![("/wt/a".to_string(), stack(7, &[1, 2]))]
        );
        delete_stack_worktree(&database, "/wt/a").await.unwrap();
        assert_eq!(read_worktree_purposes(&database).await.unwrap(), vec![]);
    }
}
