//! Where checked-out PRs are on disk, as the inbox, PR page and Stack page
//! show it.
//!
//! A PR is checked out when it has a `review_checkouts` row whose directory is
//! there; nothing else counts, so a branch checked out by hand is not the PR's
//! until Check out adopts it. `pr_checkout` and `pr_checkouts` read and change
//! nothing (no fetches, no rows written or dropped): the inbox calls them on
//! every focus. Row repair happens on the way to a change (`live_row`).

use std::collections::HashMap;
use std::path::Path;

use super::check_out::is_managed_branch;
use super::ReviewDesk;
use crate::git::checkouts as git;
use crate::git::worktrees::{self, STATUS_PARALLELISM};
use crate::review::checkout_types::PrCheckout;
use crate::storage::{self, StoredCheckout};
use crate::util::host::Host;

/// What is true of a worktree directory, whichever PR asks.
#[derive(Clone, Debug)]
pub(super) struct WorktreeFacts {
    pub(super) head: Option<String>,
    pub(super) branch: Option<String>,
    pub(super) dirty: bool,
    pub(super) main_checkout: bool,
}

pub(super) fn read_facts(host: &Host, dir: &Path) -> Result<WorktreeFacts, String> {
    Ok(WorktreeFacts {
        head: git::head_of(host, dir)?,
        branch: git::branch_of(host, dir)?,
        dirty: !git::is_clean(host, dir).unwrap_or(true),
        main_checkout: !git::is_linked_worktree(host, dir).unwrap_or(true),
    })
}

/// Whether `branch`, checked out in `row`'s worktree, is the PR's own: the
/// branch it was checked out on, or a `pr/<n>` one. A detached HEAD counts
/// unless the worktree is a stack's: nothing else claims it, whereas a
/// stack's worktree is detached only for a rebase or a clean-up.
pub(super) fn is_own_branch(row: &StoredCheckout, branch: Option<&str>) -> bool {
    match branch {
        None => row.stack_number.is_none(),
        Some(branch) => {
            row.git_branch.as_deref() == Some(branch) || is_managed_branch(branch, row.pr_number)
        }
    }
}

impl ReviewDesk {
    /// Every worktree with a directory, and the git branch on it.
    pub(super) fn listed_worktrees(&self) -> Result<Vec<(String, Option<String>)>, String> {
        Ok(worktrees::worktree_branches(&self.host, self.dir())?
            .into_iter()
            .filter(|(path, _)| self.host.is_dir(path))
            .collect())
    }

    /// Whether `row`'s PR is on its own branch in its worktree, and the
    /// commits its branch there has and lacks against the PR's `head`. Away
    /// from its own branch, HEAD says nothing about the PR, so the counts
    /// come from the PR's local branch. A head not fetched yet can't be
    /// counted against.
    fn counts(
        &self,
        row: &StoredCheckout,
        head: Option<&str>,
        facts: &WorktreeFacts,
    ) -> Result<(bool, i64, i64), String> {
        let dir = Path::new(&row.path);
        let own = is_own_branch(row, facts.branch.as_deref());
        let tip = if own {
            facts.head.clone()
        } else {
            match &row.git_branch {
                Some(branch) => git::branch_tip(&self.host, self.dir(), branch)?,
                None => None,
            }
        };
        let (ahead, behind) = match (tip, head) {
            (Some(tip), Some(head)) if tip == head => (0, 0),
            (Some(tip), Some(head)) if git::has_commit(&self.host, dir, head)? => {
                git::ahead_behind(&self.host, dir, head, &tip).unwrap_or((0, 0))
            }
            _ => (0, 0),
        };
        Ok((own, ahead, behind))
    }

    /// `row`'s checkout given what is on disk (`facts`) and the PR's `head`.
    fn checkout_at(
        &self,
        row: &StoredCheckout,
        head: Option<&str>,
        facts: WorktreeFacts,
        setup_command: Option<&String>,
    ) -> Result<PrCheckout, String> {
        let (on_pr_branch, ahead, behind) = self.counts(row, head, &facts)?;
        Ok(PrCheckout {
            number: row.pr_number,
            path: row.path.clone(),
            head_sha: facts.head,
            git_branch: facts.branch.or(row.git_branch.clone()),
            setup_pending: (!row.setup_done).then(|| setup_command.cloned()).flatten(),
            ahead,
            behind,
            dirty: facts.dirty,
            adopted: row.adopted,
            on_pr_branch,
            main_checkout: facts.main_checkout,
            stack: row.stack_number,
        })
    }

    /// `row`'s checkout as it is on disk now.
    pub(super) async fn describe(&self, row: StoredCheckout) -> Result<PrCheckout, String> {
        let head = self.head_now(&row).await?;
        let setup_command = storage::read_repository_settings(&self.database, &self.repo_key())
            .await?
            .setup_command;
        self.blocking("Reading the checkout failed", move |desk| {
            let facts = read_facts(&desk.host, Path::new(&row.path))?;
            desk.checkout_at(&row, head.as_deref(), facts, setup_command.as_ref())
        })
        .await
    }

    /// PR `number`'s worktree, or `None` when it is not checked out.
    pub(crate) async fn pr_checkout(&self, number: i64) -> Result<Option<PrCheckout>, String> {
        let Some(row) = storage::read_checkout(&self.database, &self.repo_key(), number).await?
        else {
            return Ok(None);
        };
        if !self.host.is_dir(&row.path) {
            return Ok(None);
        }
        self.describe(row).await.map(Some)
    }

    /// Every checked-out PR of the repository. A worktree that can't be read
    /// counts as not checked out.
    pub(crate) async fn pr_checkouts(&self) -> Result<Vec<PrCheckout>, String> {
        let key = self.repo_key();
        let rows: Vec<StoredCheckout> = storage::read_checkouts(&self.database, &key)
            .await?
            .into_iter()
            .filter(|row| self.host.is_dir(&row.path))
            .collect();
        if rows.is_empty() {
            return Ok(Vec::new());
        }
        let heads: HashMap<i64, String> = self
            .inbox_snapshot()
            .await?
            .into_iter()
            .map(|pr| (pr.number, pr.head_sha))
            .collect();
        let setup_command = storage::read_repository_settings(&self.database, &key)
            .await?
            .setup_command;
        self.blocking("Reading the worktrees failed", move |desk| {
            let mut paths: Vec<&str> = rows.iter().map(|row| row.path.as_str()).collect();
            paths.sort_unstable();
            paths.dedup();
            let facts = read_all_facts(&desk.host, &paths);
            Ok(rows
                .iter()
                .filter_map(|row| {
                    let found = facts.get(row.path.as_str())?.clone();
                    let head = heads
                        .get(&row.pr_number)
                        .map(String::as_str)
                        .or(row.head_sha.as_deref());
                    desk.checkout_at(row, head, found, setup_command.as_ref())
                        .ok()
                })
                .collect())
        })
        .await
    }
}

/// The facts of each of `paths`, a few at a time. One unreadable worktree
/// leaves just that one out.
fn read_all_facts<'a>(host: &Host, paths: &[&'a str]) -> HashMap<&'a str, WorktreeFacts> {
    let mut facts = HashMap::with_capacity(paths.len());
    for chunk in paths.chunks(STATUS_PARALLELISM) {
        std::thread::scope(|scope| {
            let handles: Vec<_> = chunk
                .iter()
                .map(|path| {
                    let path = *path;
                    scope.spawn(move || (path, read_facts(host, Path::new(path))))
                })
                .collect();
            for handle in handles {
                if let Ok((path, Ok(found))) = handle.join() {
                    facts.insert(path, found);
                }
            }
        });
    }
    facts
}
