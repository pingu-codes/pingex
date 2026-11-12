//! PR worktrees per (repository, PR), and the PR link on threads.
//!
//! `review_checkouts` holds a row for each PR that is checked out: its
//! worktree's path, the PR's git branch there and the PR head last seen. A
//! stack's open PRs each have a row naming the stack and its one worktree. An
//! adopted row is a worktree Pingex found, not made: it is never switched,
//! fast-forwarded or removed. Rows written before review checkouts became PR
//! worktrees say `review_checkout` until they are adopted (`step`, which new
//! rows set to `worktree`, is otherwise history). `review_pr_links` records,
//! per thread, the PR it is about and where it started.

use serde::{Deserialize, Serialize};
use turso::{params, Database, Row};

use super::db;
use crate::util::time::unix_secs;

/// The `step` new rows store; the column is kept for rows from before.
const WORKTREE_STEP: &str = "worktree";

/// A PR's stored worktree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StoredCheckout {
    pub(crate) pr_number: i64,
    /// `worktree`, or `review_checkout` on a row not yet adopted.
    pub(crate) step: String,
    pub(crate) path: String,
    /// The PR head last seen: what proves a closed PR's commits are safe once
    /// its refs have been pruned.
    pub(crate) head_sha: Option<String>,
    /// The PR's git branch in the worktree, which may have another checked out.
    pub(crate) git_branch: Option<String>,
    pub(crate) setup_done: bool,
    pub(crate) closed_at: Option<i64>,
    /// Pingex found the worktree rather than made it.
    pub(crate) adopted: bool,
    /// GitHub's number for the stack whose worktree this is.
    pub(crate) stack_number: Option<i64>,
}

impl StoredCheckout {
    /// A new row for a PR checked out at `path` on `git_branch`.
    pub(crate) fn new(pr_number: i64, path: &str, git_branch: &str, head_sha: &str) -> Self {
        Self {
            pr_number,
            step: WORKTREE_STEP.to_string(),
            path: path.to_string(),
            head_sha: (!head_sha.is_empty()).then(|| head_sha.to_string()),
            git_branch: Some(git_branch.to_string()),
            setup_done: true,
            closed_at: None,
            adopted: false,
            stack_number: None,
        }
    }
}

const CHECKOUT_COLUMNS: &str =
    "pr_number, step, path, head_sha, git_branch, setup_done, closed_at, adopted, stack_number";

fn checkout_row(row: &Row) -> Result<StoredCheckout, String> {
    Ok(StoredCheckout {
        pr_number: db::int(row, 0)?,
        step: db::text(row, 1)?,
        path: db::text(row, 2)?,
        head_sha: db::opt_text(row, 3)?,
        git_branch: db::opt_text(row, 4)?,
        setup_done: db::flag(row, 5)?,
        closed_at: db::opt_int(row, 6)?,
        adopted: db::flag(row, 7)?,
        stack_number: db::opt_int(row, 8)?,
    })
}

pub(crate) async fn read_checkout(
    database: &Database,
    repo_key: &str,
    pr_number: i64,
) -> Result<Option<StoredCheckout>, String> {
    let connection = db::conn(database)?;
    db::one(
        &connection,
        &format!(
            "SELECT {CHECKOUT_COLUMNS} FROM review_checkouts WHERE repo_key = ? AND pr_number = ?"
        ),
        params![repo_key, pr_number],
        checkout_row,
    )
    .await
}

pub(crate) async fn read_checkouts(
    database: &Database,
    repo_key: &str,
) -> Result<Vec<StoredCheckout>, String> {
    let connection = db::conn(database)?;
    db::rows(
        &connection,
        &format!(
            "SELECT {CHECKOUT_COLUMNS} FROM review_checkouts WHERE repo_key = ? ORDER BY pr_number"
        ),
        params![repo_key],
        checkout_row,
    )
    .await
}

/// Insert or replace a PR's checkout. `closed_at` is cleared: a PR being
/// checked out is open.
pub(crate) async fn write_checkout(
    database: &Database,
    repo_key: &str,
    checkout: &StoredCheckout,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "INSERT INTO review_checkouts(repo_key, pr_number, step, path, head_sha, git_branch,
             setup_done, closed_at, updated_at, adopted, stack_number)
         VALUES (?, ?, ?, ?, ?, ?, ?, NULL, ?, ?, ?)
         ON CONFLICT(repo_key, pr_number) DO UPDATE SET
             step = excluded.step,
             path = excluded.path,
             head_sha = excluded.head_sha,
             git_branch = excluded.git_branch,
             setup_done = excluded.setup_done,
             closed_at = NULL,
             updated_at = excluded.updated_at,
             adopted = excluded.adopted,
             stack_number = excluded.stack_number",
        params![
            repo_key,
            checkout.pr_number,
            checkout.step.as_str(),
            checkout.path.as_str(),
            checkout.head_sha.as_deref(),
            checkout.git_branch.as_deref(),
            i64::from(checkout.setup_done),
            unix_secs(),
            i64::from(checkout.adopted),
            checkout.stack_number
        ],
    )
    .await
}

/// Forget every PR checked out at `path`.
pub(crate) async fn delete_checkouts_at(
    database: &Database,
    repo_key: &str,
    path: &str,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "DELETE FROM review_checkouts WHERE repo_key = ? AND path = ?",
        params![repo_key, path],
    )
    .await
}

/// The repository and PR of the lone PR checked out at `path`; a stack's
/// worktree holds several, so it has none.
pub(crate) async fn read_checkout_at_path(
    database: &Database,
    path: &str,
) -> Result<Option<(String, i64)>, String> {
    let connection = db::conn(database)?;
    db::one(
        &connection,
        "SELECT repo_key, pr_number FROM review_checkouts WHERE path = ? AND stack_number IS NULL",
        params![path],
        |row| Ok((db::text(row, 0)?, db::int(row, 1)?)),
    )
    .await
}

/// Record the PR head last seen, keeping `closed_at`.
pub(crate) async fn update_checkout_head(
    database: &Database,
    repo_key: &str,
    pr_number: i64,
    head_sha: &str,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "UPDATE review_checkouts SET head_sha = ?, updated_at = ?
         WHERE repo_key = ? AND pr_number = ?",
        params![head_sha, unix_secs(), repo_key, pr_number],
    )
    .await
}

/// Make a `review_checkout` row a worktree on `git_branch`, keeping `closed_at`.
pub(crate) async fn adopt_checkout(
    database: &Database,
    repo_key: &str,
    pr_number: i64,
    git_branch: &str,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "UPDATE review_checkouts SET step = 'worktree', git_branch = ?, updated_at = ?
         WHERE repo_key = ? AND pr_number = ?",
        params![git_branch, unix_secs(), repo_key, pr_number],
    )
    .await
}

/// Mark every setup at `path` done: setup runs once per checkout directory,
/// whichever PR of a stack created it.
pub(crate) async fn mark_checkout_setup_done(
    database: &Database,
    repo_key: &str,
    path: &str,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "UPDATE review_checkouts SET setup_done = 1 WHERE repo_key = ? AND path = ?",
        params![repo_key, path],
    )
    .await
}

/// Record that a PR closed at `now`, unless already recorded.
pub(crate) async fn mark_checkout_closed(
    database: &Database,
    repo_key: &str,
    pr_number: i64,
    now: i64,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "UPDATE review_checkouts SET closed_at = ?
         WHERE repo_key = ? AND pr_number = ? AND closed_at IS NULL",
        params![now, repo_key, pr_number],
    )
    .await
}

/// Clear a reopened PR's `closed_at`.
pub(crate) async fn mark_checkout_open(
    database: &Database,
    repo_key: &str,
    pr_number: i64,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "UPDATE review_checkouts SET closed_at = NULL WHERE repo_key = ? AND pr_number = ?",
        params![repo_key, pr_number],
    )
    .await
}

pub(crate) async fn delete_checkout(
    database: &Database,
    repo_key: &str,
    pr_number: i64,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "DELETE FROM review_checkouts WHERE repo_key = ? AND pr_number = ?",
        params![repo_key, pr_number],
    )
    .await
}

/// A thread's PR link: the PR it is about and where it started. Only the PR number (and whether the PR is mine) ever changes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PrLink {
    pub(crate) thread_id: String,
    /// The repository's project root path.
    pub(crate) repo_key: String,
    pub(crate) pr_number: i64,
    /// Whether I wrote the PR.
    pub(crate) mine: bool,
    /// The worktree the thread started in; `None` on a link from before every
    /// PR thread ran in one.
    pub(crate) checkout_path: Option<String>,
    pub(crate) git_branch: Option<String>,
    pub(crate) start_sha: Option<String>,
    pub(crate) created_at: i64,
    /// When the PR was seen closed; such a thread leaves the Reviews group.
    pub(crate) closed_at: Option<i64>,
}

const LINK_COLUMNS: &str = "thread_id, repo_key, pr_number, mine, checkout_path, git_branch, \
     start_sha, created_at, closed_at";

fn link_row(row: &Row) -> Result<PrLink, String> {
    Ok(PrLink {
        thread_id: db::text(row, 0)?,
        repo_key: db::text(row, 1)?,
        pr_number: db::int(row, 2)?,
        mine: db::flag(row, 3)?,
        checkout_path: db::opt_text(row, 4)?,
        git_branch: db::opt_text(row, 5)?,
        start_sha: db::opt_text(row, 6)?,
        created_at: db::int(row, 7)?,
        closed_at: db::opt_int(row, 8)?,
    })
}

pub(crate) async fn write_pr_link(database: &Database, link: &PrLink) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        &format!(
            "INSERT OR REPLACE INTO review_pr_links({LINK_COLUMNS}, step) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        ),
        params![
            link.thread_id.as_str(),
            link.repo_key.as_str(),
            link.pr_number,
            i64::from(link.mine),
            link.checkout_path.as_deref(),
            link.git_branch.as_deref(),
            link.start_sha.as_deref(),
            link.created_at,
            link.closed_at,
            WORKTREE_STEP
        ],
    )
    .await
}

pub(crate) async fn read_pr_link(
    database: &Database,
    thread_id: &str,
) -> Result<Option<PrLink>, String> {
    let connection = db::conn(database)?;
    db::one(
        &connection,
        &format!("SELECT {LINK_COLUMNS} FROM review_pr_links WHERE thread_id = ?"),
        params![thread_id],
        link_row,
    )
    .await
}

/// Every PR link in this Home.
pub(crate) async fn read_pr_links(database: &Database) -> Result<Vec<PrLink>, String> {
    let connection = db::conn(database)?;
    db::rows(
        &connection,
        &format!("SELECT {LINK_COLUMNS} FROM review_pr_links ORDER BY created_at, thread_id"),
        (),
        link_row,
    )
    .await
}

/// Move a thread to another PR, keeping its recorded start.
pub(crate) async fn move_pr_link(
    database: &Database,
    thread_id: &str,
    pr_number: i64,
    mine: bool,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "UPDATE review_pr_links SET pr_number = ?, mine = ?, closed_at = NULL WHERE thread_id = ?",
        params![pr_number, i64::from(mine), thread_id],
    )
    .await
}

/// Correct whether a link's PR is the viewer's, which changes when the
/// repository's GitHub account does.
pub(crate) async fn set_pr_link_mine(
    database: &Database,
    thread_id: &str,
    mine: bool,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "UPDATE review_pr_links SET mine = ? WHERE thread_id = ?",
        params![i64::from(mine), thread_id],
    )
    .await
}

pub(crate) async fn delete_pr_link(database: &Database, thread_id: &str) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "DELETE FROM review_pr_links WHERE thread_id = ?",
        params![thread_id],
    )
    .await
}

/// Record that the PR closed for every thread linked to it.
pub(crate) async fn mark_pr_links_closed(
    database: &Database,
    repo_key: &str,
    pr_number: i64,
    now: i64,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "UPDATE review_pr_links SET closed_at = ?
         WHERE repo_key = ? AND pr_number = ? AND closed_at IS NULL",
        params![now, repo_key, pr_number],
    )
    .await
}
