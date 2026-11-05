//! Checkout steps per (repository, PR), and the PR link on threads.
//!
//! `review_checkouts` holds a row only for a PR above No checkout: its review
//! checkout (path and head) or its worktree (path and git branch). Several
//! PRs of one stack can share one path. `review_pr_links` records, per
//! thread, the PR it is about and what was checked out when it started.

use serde::{Deserialize, Serialize};
use turso::{params, Database, Row};

use super::db;
use crate::review::requirements::CheckoutStep;
use crate::util::time::unix_secs;

fn step_name(step: CheckoutStep) -> &'static str {
    match step {
        CheckoutStep::NoCheckout => "no_checkout",
        CheckoutStep::ReviewCheckout => "review_checkout",
        CheckoutStep::Worktree => "worktree",
    }
}

fn parse_step(name: &str) -> CheckoutStep {
    match name {
        "review_checkout" => CheckoutStep::ReviewCheckout,
        "worktree" => CheckoutStep::Worktree,
        _ => CheckoutStep::NoCheckout,
    }
}

/// A PR's stored checkout above No checkout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StoredCheckout {
    pub(crate) pr_number: i64,
    /// `review_checkout` or `worktree`.
    pub(crate) step: String,
    pub(crate) path: String,
    pub(crate) head_sha: Option<String>,
    pub(crate) git_branch: Option<String>,
    pub(crate) setup_done: bool,
    pub(crate) closed_at: Option<i64>,
}

const CHECKOUT_COLUMNS: &str = "pr_number, step, path, head_sha, git_branch, setup_done, closed_at";

fn checkout_row(row: &Row) -> Result<StoredCheckout, String> {
    Ok(StoredCheckout {
        pr_number: db::int(row, 0)?,
        step: db::text(row, 1)?,
        path: db::text(row, 2)?,
        head_sha: db::opt_text(row, 3)?,
        git_branch: db::opt_text(row, 4)?,
        setup_done: db::flag(row, 5)?,
        closed_at: db::opt_int(row, 6)?,
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
             setup_done, closed_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, NULL, ?)
         ON CONFLICT(repo_key, pr_number) DO UPDATE SET
             step = excluded.step,
             path = excluded.path,
             head_sha = excluded.head_sha,
             git_branch = excluded.git_branch,
             setup_done = excluded.setup_done,
             closed_at = NULL,
             updated_at = excluded.updated_at",
        params![
            repo_key,
            checkout.pr_number,
            checkout.step.as_str(),
            checkout.path.as_str(),
            checkout.head_sha.as_deref(),
            checkout.git_branch.as_deref(),
            i64::from(checkout.setup_done),
            unix_secs()
        ],
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

/// A thread's PR link: the PR it is about and what was checked out when it
/// started. Only the PR number (and whether the PR is mine) ever changes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PrLink {
    pub(crate) thread_id: String,
    /// The repository's project root path.
    pub(crate) repo_key: String,
    pub(crate) pr_number: i64,
    /// Whether I wrote the PR.
    pub(crate) mine: bool,
    /// The checkout step when the thread started.
    pub(crate) step: CheckoutStep,
    pub(crate) checkout_path: Option<String>,
    pub(crate) git_branch: Option<String>,
    pub(crate) start_sha: Option<String>,
    pub(crate) created_at: i64,
    /// When the PR was seen closed; such a thread leaves the Reviews group.
    pub(crate) closed_at: Option<i64>,
}

const LINK_COLUMNS: &str =
    "thread_id, repo_key, pr_number, mine, step, checkout_path, git_branch, \
     start_sha, created_at, closed_at";

fn link_row(row: &Row) -> Result<PrLink, String> {
    Ok(PrLink {
        thread_id: db::text(row, 0)?,
        repo_key: db::text(row, 1)?,
        pr_number: db::int(row, 2)?,
        mine: db::flag(row, 3)?,
        step: parse_step(&db::text(row, 4)?),
        checkout_path: db::opt_text(row, 5)?,
        git_branch: db::opt_text(row, 6)?,
        start_sha: db::opt_text(row, 7)?,
        created_at: db::int(row, 8)?,
        closed_at: db::opt_int(row, 9)?,
    })
}

pub(crate) async fn write_pr_link(database: &Database, link: &PrLink) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        &format!(
            "INSERT OR REPLACE INTO review_pr_links({LINK_COLUMNS}) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        ),
        params![
            link.thread_id.as_str(),
            link.repo_key.as_str(),
            link.pr_number,
            i64::from(link.mine),
            step_name(link.step),
            link.checkout_path.as_deref(),
            link.git_branch.as_deref(),
            link.start_sha.as_deref(),
            link.created_at,
            link.closed_at
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
