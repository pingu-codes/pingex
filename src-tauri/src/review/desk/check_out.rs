//! Checking a PR out: the one way a PR gets onto disk.
//!
//! A PR is either not checked out or checked out in its worktree, and its
//! `review_checkouts` row says which. A lone PR's worktree is its own; a PR in
//! a stack is always in the stack's one worktree (`stack_worktree`). Only an
//! explicit Check out makes one (`check_out`, `check_out_stack`), and the desk
//! works out the PR itself (`target_for`): the caller names a number.
//!
//! Every worktree Pingex makes is the same kind: under the repository's
//! worktree directory, locked with [`git::PR_WORKTREE_LOCK`], and removed by
//! cleanup once its PR (or every PR of its stack) has closed, only when it
//! holds no work. The branch decides only how a push gets back: my
//! same-repository PR, or a stack's, is on its real git branch, where a plain
//! `git push` updates the PR; anyone else's PR, and any fork, is on `pr/<n>`
//! with no upstream, and `push_to_pr` is the way back. That keeps fork branch
//! names from colliding.
//!
//! When the git branch is already checked out somewhere, Check out says so
//! (`BranchInUse`) instead of taking that worktree; `adopt_worktree` then
//! makes it the PR's. Pingex never switches, fast-forwards or removes an
//! adopted worktree.
//!
//! The repository's setup command runs once per worktree Pingex creates.

use std::path::Path;

use super::pr_checkouts::is_own_branch;
use super::{ReviewDesk, PROVIDER};
use crate::git::stack::{self as stack_git};
use crate::git::{checkouts as git, objects};
use crate::review::checkout_types::{CheckOutOutcome, CheckoutTarget, PrCheckout};
use crate::review::stack_types::{StackInput, StackInputPr};
use crate::review::types::{InboxPr, PrStack};
use crate::storage::{self, StoredCheckout};
use crate::util::process::{run, Run};

/// The step on a row written before review checkouts became PR worktrees.
const LEGACY_REVIEW_CHECKOUT: &str = "review_checkout";

/// The longest a setup command may run.
const SETUP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20 * 60);

/// A path segment from a git branch or stack name: lower case, `[a-z0-9._-]`.
fn segment(name: &str) -> String {
    let slug: String = name
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '-'
            }
        })
        .collect();
    let slug = slug.trim_matches(['-', '.']).to_string();
    if slug.is_empty() {
        "checkout".to_string()
    } else {
        slug
    }
}

/// A stack's name: its bottom PR's git branch without an `owner/` prefix, as
/// the inbox names it.
pub(super) fn stack_name(stack: &PrStack) -> String {
    let bottom = stack
        .prs
        .first()
        .map(|pr| pr.head_ref.as_str())
        .unwrap_or("");
    match bottom.rsplit('/').next() {
        Some(name) if !name.is_empty() => name.to_string(),
        _ => format!("Stack {}", stack.number),
    }
}

/// What removing a worktree did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Removal {
    /// It is gone, or was only forgotten: adopted, or not Pingex's.
    Removed,
    /// It holds work nothing else has, so it stays.
    KeptLocalWork,
}

/// A worktree with a git branch Check out wanted.
pub(super) struct InUse {
    pub(super) branch: String,
    pub(super) path: String,
    pub(super) main_clone: bool,
}

impl ReviewDesk {
    fn home(&self) -> Result<&str, String> {
        self.home_dir
            .as_deref()
            .ok_or_else(|| "This Home has no folder for PR worktrees".to_string())
    }

    fn repo_name(&self) -> String {
        segment(
            self.repo_dir
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .as_deref()
                .unwrap_or("repository"),
        )
    }

    /// Whether `target`'s head is in the repository the desk is for.
    pub(super) fn same_repo(&self, target: &CheckoutTarget) -> Result<bool, String> {
        let Some(head_repo) = &target.head_repo else {
            return Ok(false);
        };
        Ok(head_repo.eq_ignore_ascii_case(&self.github.repo()?))
    }

    /// The git branch a lone PR's worktree is on: its own when it is mine and
    /// in this repository, else `pr/<n>`.
    fn lone_branch(&self, target: &CheckoutTarget) -> Result<String, String> {
        Ok(if target.mine && self.same_repo(target)? {
            target.head_ref.clone()
        } else {
            git::managed_branch(target.number)
        })
    }

    /// Where a new worktree on `branch` goes: the repository's worktree
    /// directory, else `<home>/worktrees/<repository>/`. Every PR and stack
    /// worktree follows this one rule.
    pub(super) async fn new_worktree_path(&self, branch: &str) -> Result<String, String> {
        let settings = storage::read_repository_settings(&self.database, &self.repo_key()).await?;
        let group = match settings.worktree_dir {
            Some(dir) => dir,
            None => {
                let root = self.host.join_str(self.home()?, "worktrees");
                self.host.join_str(&root, &self.repo_name())
            }
        };
        Ok(self.host.join_str(&group, &segment(branch)))
    }

    /// The open PRs as the last inbox sync stored them; empty before one.
    pub(super) async fn inbox_snapshot(&self) -> Result<Vec<InboxPr>, String> {
        let slug = self
            .blocking("Reading the repository failed", |desk| desk.github.repo())
            .await?;
        Ok(
            storage::read_inbox_snapshot_any_case(&self.database, PROVIDER, &slug)
                .await?
                .and_then(|payload| serde_json::from_str(&payload).ok())
                .unwrap_or_default(),
        )
    }

    /// PR `number` as the desk works on it: head and author from the last
    /// inbox sync, else from GitHub, and whether it is mine from the account
    /// the repository runs as. Its stack is not looked up (`stack_of`).
    pub(crate) async fn target_for(&self, number: i64) -> Result<CheckoutTarget, String> {
        let known = self
            .inbox_snapshot()
            .await?
            .into_iter()
            .find(|pr| pr.number == number);
        let (author, head_ref, head_sha, head_repo) = match known {
            Some(pr) => (pr.author, pr.head_ref, pr.head_sha, pr.head_repo),
            None => {
                let detail = self
                    .blocking("Reading the pull request failed", move |desk| {
                        desk.github.pr_detail(number)
                    })
                    .await?;
                (
                    detail.summary.author,
                    detail.summary.head_ref,
                    detail.head_sha,
                    detail.summary.head_repo,
                )
            }
        };
        let viewer = self
            .repository_account()
            .await
            .ok()
            .and_then(|account| account.login);
        Ok(CheckoutTarget {
            number,
            mine: viewer.is_some_and(|viewer| viewer.eq_ignore_ascii_case(&author)),
            head_ref,
            head_sha,
            head_repo,
            stack: None,
            stack_branches: Vec::new(),
        })
    }

    /// The open stack PR `number` is an open member of, from GitHub's listing
    /// now. The backend keeps no copy of the listing: it is read when a PR is
    /// checked out, which is rare and explicit.
    pub(super) async fn stack_of(&self, number: i64) -> Result<Option<PrStack>, String> {
        let listing = self.stacks().await?;
        if !listing.supported {
            return Ok(None);
        }
        Ok(listing.stacks.into_iter().find(|stack| {
            stack.open
                && stack
                    .prs
                    .iter()
                    .any(|pr| pr.number == number && pr.state == "open" && !pr.merged)
        }))
    }

    /// `stack` as the stack operations take it: GitHub's listing joined with
    /// each PR's facts from the last inbox sync.
    pub(super) async fn stack_input(&self, stack: &PrStack) -> Result<StackInput, String> {
        let snapshot = self.inbox_snapshot().await?;
        let known = |number: i64| snapshot.iter().find(|pr| pr.number == number);
        let bottom = stack
            .prs
            .iter()
            .filter(|pr| pr.state == "open" && !pr.merged)
            .filter_map(|pr| known(pr.number))
            .find(|pr| pr.base_ref == stack.base_ref);
        Ok(StackInput {
            number: stack.number,
            name: stack_name(stack),
            base_ref: stack.base_ref.clone(),
            base_sha: bottom.map(|pr| pr.base_sha.clone()).unwrap_or_default(),
            prs: stack
                .prs
                .iter()
                .map(|pr| StackInputPr {
                    number: pr.number,
                    title: known(pr.number)
                        .map(|known| known.title.clone())
                        .unwrap_or_default(),
                    head_ref: pr.head_ref.clone(),
                    head_sha: known(pr.number)
                        .map(|known| known.head_sha.clone())
                        .unwrap_or_else(|| pr.head_sha.clone()),
                    open: pr.state == "open",
                    merged: pr.merged,
                })
                .collect(),
        })
    }

    /// Check out PR `number`: a lone PR in its own worktree, a stack's PR by
    /// checking out the whole stack. Safe to run again: a PR checked out
    /// already only catches up with its head where nothing can be lost.
    pub(crate) async fn check_out(&self, number: i64) -> Result<CheckOutOutcome, String> {
        super::pr_number(number)?;
        if let Some(stack) = self.stack_of(number).await? {
            let input = self.stack_input(&stack).await?;
            return self.check_out_stack_input(input).await;
        }
        let target = self.target_for(number).await?;
        self.check_out_lone(target).await
    }

    /// Check out every open PR of stack `stack` in one worktree.
    pub(crate) async fn check_out_stack(&self, stack: i64) -> Result<CheckOutOutcome, String> {
        let listing = self.stacks().await?;
        let found = listing
            .stacks
            .into_iter()
            .find(|candidate| candidate.number == stack && candidate.open)
            .ok_or_else(|| "That stack is not open on GitHub".to_string())?;
        let input = self.stack_input(&found).await?;
        self.check_out_stack_input(input).await
    }

    async fn check_out_lone(&self, target: CheckoutTarget) -> Result<CheckOutOutcome, String> {
        let key = self.repo_key();
        if let Some(row) = self.live_row(target.number).await? {
            let checkout = self.synced(row, Some(&target.head_sha)).await?;
            return Ok(checked_out(checkout, false));
        }
        let moving = target.clone();
        let (branch, in_use) = self
            .blocking("Reading worktrees failed", move |desk| {
                let branch = desk.lone_branch(&moving)?;
                let in_use = desk.branch_in_use(&[branch.as_str()])?;
                Ok((branch, in_use))
            })
            .await?;
        if let Some(found) = in_use {
            return Ok(found.outcome(target.number));
        }
        let path = self.new_worktree_path(&branch).await?;
        let (moving_path, moving_branch, moving) = (path.clone(), branch.clone(), target.clone());
        let new_branch = self
            .blocking("Creating the worktree failed", move |desk| {
                desk.ensure_commit(moving.number, &moving.head_sha)?;
                let new_branch = git::branch_tip(&desk.host, desk.dir(), &moving_branch)?.is_none();
                git::add_pr_worktree(
                    &desk.host,
                    desk.dir(),
                    &moving_path,
                    &moving_branch,
                    &moving.head_sha,
                )?;
                if is_managed_branch(&moving_branch, moving.number) {
                    return Ok(new_branch);
                }
                let tracked =
                    objects::remote_for_slug(&desk.host, desk.dir(), &desk.github.repo()?)
                        .and_then(|remote| match remote {
                            Some(remote) => git::track_remote_branch(
                                &desk.host,
                                desk.dir(),
                                &moving_branch,
                                &remote,
                            ),
                            None => Ok(()),
                        });
                if let Err(error) = tracked {
                    desk.undo_add(&moving_path, new_branch.then_some(moving_branch.as_str()));
                    return Err(error);
                }
                Ok(new_branch)
            })
            .await?;
        let setup_command = storage::read_repository_settings(&self.database, &key)
            .await?
            .setup_command;
        let row = StoredCheckout {
            setup_done: setup_command.is_none(),
            ..StoredCheckout::new(target.number, &path, &branch, &target.head_sha)
        };
        let made_branch = new_branch.then(|| branch.clone());
        self.write_rows_or_undo(std::slice::from_ref(&row), &path, made_branch)
            .await?;
        Ok(checked_out(self.describe(row).await?, true))
    }

    /// Store `rows`, or, when that fails, remove the worktree just made at
    /// `path` (and `new_branch`), so nothing is left that no row knows about.
    pub(super) async fn write_rows_or_undo(
        &self,
        rows: &[StoredCheckout],
        path: &str,
        new_branch: Option<String>,
    ) -> Result<(), String> {
        let key = self.repo_key();
        for row in rows {
            if let Err(error) = storage::write_checkout(&self.database, &key, row).await {
                let (made, branch) = (path.to_string(), new_branch.clone());
                let _ = self
                    .blocking("Removing the worktree failed", move |desk| {
                        desk.undo_add(&made, branch.as_deref());
                        Ok(())
                    })
                    .await;
                let _ = storage::delete_checkouts_at(&self.database, &key, path).await;
                return Err(error);
            }
        }
        Ok(())
    }

    /// Remove a worktree this call made, and its branch when that is new too.
    pub(super) fn undo_add(&self, path: &str, new_branch: Option<&str>) {
        let _ = git::remove_pr_worktree(&self.host, self.dir(), path, new_branch);
    }

    /// The first listed worktree on one of `branches`.
    pub(super) fn branch_in_use(&self, branches: &[&str]) -> Result<Option<InUse>, String> {
        let Some((path, branch)) = self.listed_worktrees()?.into_iter().find(|(_, branch)| {
            branch
                .as_deref()
                .is_some_and(|branch| branches.contains(&branch))
        }) else {
            return Ok(None);
        };
        Ok(Some(InUse {
            main_clone: !git::is_linked_worktree(&self.host, Path::new(&path))?,
            branch: branch.unwrap_or_default(),
            path,
        }))
    }

    /// PR `number`'s row when its worktree is still there. A row whose
    /// directory is gone is dropped; one from before PR worktrees is
    /// converted. The repair a read must not do happens here, on the way to
    /// a change.
    pub(super) async fn live_row(&self, number: i64) -> Result<Option<StoredCheckout>, String> {
        let key = self.repo_key();
        let Some(row) = storage::read_checkout(&self.database, &key, number).await? else {
            return Ok(None);
        };
        if !self.host.is_dir(&row.path) {
            storage::delete_checkout(&self.database, &key, number).await?;
            return Ok(None);
        }
        Ok(Some(self.adopt_legacy(row).await?))
    }

    /// `stored` as a PR worktree, if it is a review checkout from before they
    /// became worktrees. A detached one gets a branch at its HEAD, so its
    /// changes and commits come along; the path, and every thread's cwd in it,
    /// stay the same.
    pub(super) async fn adopt_legacy(
        &self,
        stored: StoredCheckout,
    ) -> Result<StoredCheckout, String> {
        if stored.step != LEGACY_REVIEW_CHECKOUT {
            return Ok(stored);
        }
        let (path, number) = (stored.path.clone(), stored.pr_number);
        let branch = self
            .blocking("Adopting the review checkout failed", move |desk| {
                git::adopt_review_checkout(&desk.host, desk.dir(), &path, number)
            })
            .await?;
        storage::adopt_checkout(&self.database, &self.repo_key(), number, &branch).await?;
        Ok(StoredCheckout {
            step: "worktree".to_string(),
            git_branch: Some(branch),
            ..stored
        })
    }

    /// `row`'s checkout, having recorded `head` as the PR's and brought the
    /// worktree level with it where that is safe. Best effort: a PR that
    /// can't be fetched just reports where it is.
    pub(super) async fn synced(
        &self,
        row: StoredCheckout,
        head: Option<&str>,
    ) -> Result<PrCheckout, String> {
        let row = match head.filter(|head| !head.is_empty()) {
            Some(head) if row.head_sha.as_deref() != Some(head) => {
                storage::update_checkout_head(
                    &self.database,
                    &self.repo_key(),
                    row.pr_number,
                    head,
                )
                .await?;
                StoredCheckout {
                    head_sha: Some(head.to_string()),
                    ..row
                }
            }
            _ => row,
        };
        if !row.adopted {
            if let Some(head) = row.head_sha.clone() {
                let moving = row.clone();
                let _ = self
                    .blocking("Updating the worktree failed", move |desk| {
                        desk.follow_head(&moving, &head)
                    })
                    .await;
            }
        }
        self.describe(row).await
    }

    /// PR `number`'s checkout brought level with `head` (or the head last
    /// seen) where that is safe; `None` when it is not checked out.
    pub(crate) async fn synced_pr_checkout(
        &self,
        number: i64,
        head: Option<String>,
    ) -> Result<Option<PrCheckout>, String> {
        match self.live_row(number).await? {
            Some(row) => Ok(Some(self.synced(row, head.as_deref()).await?)),
            None => Ok(None),
        }
    }

    /// Make sure PR `number`'s head commit `sha` is in the repository,
    /// fetching its ref. Takes the repository's lock, so never call it
    /// holding that.
    pub(super) fn ensure_commit(&self, number: i64, sha: &str) -> Result<(), String> {
        if git::has_commit(&self.host, self.dir(), sha)? {
            return Ok(());
        }
        let number = super::pr_number(number)?;
        let slug = self.github.repo()?;
        let remote = objects::remote_for_slug(&self.host, self.dir(), &slug)?
            .ok_or_else(|| format!("No remote in this repository points at {slug}"))?;
        objects::fetch_pr_heads(&self.host, self.dir(), &remote, &[number])?;
        if git::has_commit(&self.host, self.dir(), sha)? {
            Ok(())
        } else {
            Err("The pull request's head commit could not be fetched".to_string())
        }
    }

    /// Bring `row`'s worktree level with the PR head `sha` by fast-forwarding,
    /// when nothing can be lost by it: a linked worktree, on the PR's own
    /// branch, with no local changes, whose HEAD `sha` contains. Otherwise it
    /// stays where it is, and `PrCheckout` reports how it differs.
    fn follow_head(&self, row: &StoredCheckout, sha: &str) -> Result<(), String> {
        let dir = Path::new(&row.path);
        let Some(head) = git::head_of(&self.host, dir)? else {
            return Ok(());
        };
        let branch = git::branch_of(&self.host, dir)?;
        if head == sha
            || !git::is_linked_worktree(&self.host, dir)?
            || branch.is_none()
            || !is_own_branch(row, branch.as_deref())
            || !git::is_clean(&self.host, dir)?
        {
            return Ok(());
        }
        self.ensure_commit(row.pr_number, sha)?;
        if objects::is_ancestor(&self.host, dir, &head, sha)? {
            git::fast_forward(&self.host, self.dir(), dir, sha)?;
        }
        Ok(())
    }

    /// Make the worktree at `path`, found on the git branch Check out wanted,
    /// PR `number`'s: for a stack's PR, the whole stack's. Pingex records it
    /// as adopted and never switches, fast-forwards or removes it.
    pub(crate) async fn adopt_worktree(
        &self,
        number: i64,
        path: String,
    ) -> Result<PrCheckout, String> {
        super::pr_number(number)?;
        let moving = path.clone();
        let (path, branch) = self
            .blocking("Reading worktrees failed", move |desk| {
                let wanted = desk.host.canonical(&moving);
                desk.listed_worktrees()?
                    .into_iter()
                    .find(|(listed, _)| *listed == moving || desk.host.canonical(listed) == wanted)
                    .ok_or_else(|| format!("{moving} is not a worktree of this repository"))
            })
            .await?;
        let branch = branch.ok_or_else(|| format!("{path} has no git branch checked out"))?;
        if let Some(stack) = self.stack_of(number).await? {
            let input = self.stack_input(&stack).await?;
            if !input.open_prs().iter().any(|pr| pr.head_ref == branch) {
                return Err(format!("{path} is on {branch}, which is not in this stack"));
            }
            return self.adopt_stack_worktree(input, number, path).await;
        }
        let target = self.target_for(number).await?;
        let moving = target.clone();
        let wanted = self
            .blocking("Reading the repository failed", move |desk| {
                desk.lone_branch(&moving)
            })
            .await?;
        if branch != wanted {
            return Err(format!("{path} is on {branch}, not {wanted}"));
        }
        let row = StoredCheckout {
            adopted: true,
            ..StoredCheckout::new(number, &path, &branch, &target.head_sha)
        };
        storage::write_checkout(&self.database, &self.repo_key(), &row).await?;
        self.describe(row).await
    }

    /// Put the stack worktree PR `number` is checked out in on its git
    /// branch. Refuses one with changes or a rebase going on, an adopted one
    /// and the main clone.
    pub(crate) async fn switch_stack_branch(&self, number: i64) -> Result<PrCheckout, String> {
        let row = self
            .live_row(number)
            .await?
            .filter(|row| row.stack_number.is_some())
            .ok_or_else(|| format!("#{number} is not checked out in a stack's worktree"))?;
        let branch = row
            .git_branch
            .clone()
            .ok_or_else(|| format!("#{number} has no git branch"))?;
        if row.adopted {
            return Err(format!(
                "Pingex doesn't switch a worktree it found. Switch {} to {branch} there",
                row.path
            ));
        }
        let head = self.head_now(&row).await?;
        let (moving_path, moving_branch) = (row.path.clone(), branch.clone());
        self.blocking("Switching the worktree failed", move |desk| {
            let dir = Path::new(&moving_path);
            if !git::is_linked_worktree(&desk.host, dir)? {
                return Err(
                    "The repository's main clone is not a stack's worktree, so it is not switched"
                        .to_string(),
                );
            }
            if stack_git::rebase_in_progress(&desk.host, dir) {
                return Err(format!("A rebase is going on in {moving_path}"));
            }
            if !git::is_clean(&desk.host, dir)? {
                return Err(format!(
                    "{moving_path} has uncommitted changes. Commit or stash them, then switch"
                ));
            }
            let missing = stack_git::branch_tip(&desk.host, desk.dir(), &moving_branch)?.is_none();
            let head = match (missing, head) {
                (false, _) => None,
                (true, Some(head)) => Some(head),
                (true, None) => return Err(format!("{moving_branch} is not in this repository")),
            };
            // Fetching takes the repository's lock itself, so it comes first.
            if let Some(head) = &head {
                desk.ensure_commit(number, head)?;
            }
            stack_git::locked(&desk.host, desk.dir(), || {
                if let Some(head) = &head {
                    if stack_git::branch_tip(&desk.host, desk.dir(), &moving_branch)?.is_none() {
                        stack_git::create_branch(&desk.host, desk.dir(), &moving_branch, head)?;
                    }
                }
                stack_git::switch(&desk.host, dir, &moving_branch)
            })
        })
        .await?;
        self.describe(row).await
    }

    /// The PR's head as the last inbox sync has it, else as last seen.
    pub(super) async fn head_now(&self, row: &StoredCheckout) -> Result<Option<String>, String> {
        Ok(self
            .inbox_snapshot()
            .await?
            .into_iter()
            .find(|pr| pr.number == row.pr_number)
            .map(|pr| pr.head_sha)
            .or_else(|| row.head_sha.clone()))
    }

    /// Run the repository's setup command in PR `number`'s new worktree. Does
    /// nothing once it has run there.
    pub(crate) async fn run_checkout_setup(&self, number: i64) -> Result<(), String> {
        let key = self.repo_key();
        let Some(stored) = storage::read_checkout(&self.database, &key, number).await? else {
            return Err("This pull request has no worktree".to_string());
        };
        if stored.setup_done {
            return Ok(());
        }
        let settings = storage::read_repository_settings(&self.database, &key).await?;
        if let Some(command) = settings.setup_command {
            let path = stored.path.clone();
            self.blocking("Setup failed", move |desk| {
                let args = ["-c", command.as_str()];
                let output = run(Run::on(&desk.host, "sh", &path, &args, SETUP_TIMEOUT))
                    .map_err(|_| format!("Could not run `{command}`"))?;
                if output.ok {
                    Ok(())
                } else {
                    let tail: Vec<&str> = output.stderr.lines().rev().take(5).collect();
                    let tail: Vec<&str> = tail.into_iter().rev().collect();
                    Err(format!("`{command}` failed: {}", tail.join("\n")))
                }
            })
            .await?;
        }
        storage::mark_checkout_setup_done(&self.database, &key, &stored.path).await
    }

    /// Whether Pingex made the worktree at `path` (it carries the lock).
    pub(crate) async fn is_managed_worktree(&self, path: String) -> Result<bool, String> {
        self.blocking("Reading the worktree failed", move |desk| {
            git::is_managed(&desk.host, Path::new(&path))
        })
        .await
    }

    /// Remove the worktree at `path` that Pingex made, with its `pr/<n>`
    /// branch, if it holds no work; otherwise it stays. An adopted worktree,
    /// or one Pingex didn't make, is only forgotten. Every PR checked out
    /// there loses its row.
    pub(super) async fn try_remove_worktree(&self, path: &str) -> Result<Removal, String> {
        let key = self.repo_key();
        let rows: Vec<StoredCheckout> = storage::read_checkouts(&self.database, &key)
            .await?
            .into_iter()
            .filter(|row| row.path == path)
            .collect();
        let Some(first) = rows.first() else {
            return Ok(Removal::Removed);
        };
        let first = if self.host.is_dir(path) {
            self.adopt_legacy(first.clone()).await?
        } else {
            first.clone()
        };
        let pingex_made = !rows.iter().any(|row| row.adopted);
        if pingex_made {
            let (moving_path, moving_rows) = (path.to_string(), rows.clone());
            let removal = self
                .blocking("Removing the worktree failed", move |desk| {
                    let dir = Path::new(&moving_path);
                    let mut branch = None;
                    if desk.host.is_dir(&moving_path) {
                        if !git::is_managed(&desk.host, dir)? {
                            return Ok(Removal::Removed);
                        }
                        branch = git::branch_of(&desk.host, dir)?;
                        let last_head = moving_rows
                            .iter()
                            .find(|row| row.git_branch == branch)
                            .unwrap_or(&first)
                            .head_sha
                            .clone();
                        if git::has_local_work(
                            &desk.host,
                            dir,
                            branch.as_deref(),
                            last_head.as_deref(),
                        ) {
                            return Ok(Removal::KeptLocalWork);
                        }
                    }
                    let branch = branch.or(first.git_branch.clone());
                    let ours = branch.as_deref().filter(|branch| {
                        moving_rows
                            .iter()
                            .any(|row| is_managed_branch(branch, row.pr_number))
                    });
                    match git::remove_pr_worktree(&desk.host, desk.dir(), &moving_path, ours) {
                        Ok(()) => Ok(Removal::Removed),
                        // It changed since the check, or git has its own reason
                        // to keep it, such as submodules.
                        Err(_) => Ok(Removal::KeptLocalWork),
                    }
                })
                .await?;
            if removal == Removal::KeptLocalWork {
                return Ok(removal);
            }
        }
        storage::delete_checkouts_at(&self.database, &key, path).await?;
        storage::delete_stack_worktree(&self.database, path).await?;
        Ok(Removal::Removed)
    }

    /// Undo checking PR `number` out: remove its worktree (a stack's, for a
    /// stack's PR). Refused when it holds work.
    pub(crate) async fn remove_pr_worktree(&self, number: i64) -> Result<(), String> {
        let Some(row) = storage::read_checkout(&self.database, &self.repo_key(), number).await?
        else {
            return Ok(());
        };
        match self.try_remove_worktree(&row.path).await? {
            Removal::Removed => Ok(()),
            Removal::KeptLocalWork => {
                Err("This worktree has local changes or unpushed commits, so it was kept".into())
            }
        }
    }
}

impl InUse {
    pub(super) fn outcome(self, number: i64) -> CheckOutOutcome {
        CheckOutOutcome::BranchInUse {
            number,
            branch: self.branch,
            path: self.path,
            main_clone: self.main_clone,
        }
    }
}

/// A checkout that needed no stack work.
fn checked_out(checkout: PrCheckout, created: bool) -> CheckOutOutcome {
    CheckOutOutcome::CheckedOut {
        checkout,
        created,
        created_branches: Vec::new(),
        kept: Vec::new(),
        local_stack_warning: None,
    }
}

/// Whether `branch` is one Pingex made for PR `number`: `pr/<n>`, or the
/// fallback adoption uses when that name was taken.
pub(super) fn is_managed_branch(branch: &str, number: i64) -> bool {
    let name = git::managed_branch(number);
    branch == name || branch == format!("{name}-local")
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::super::test_support::{fixture, Fixture, SLUG};
    use super::Removal;
    use crate::git::checkouts::PR_WORKTREE_LOCK;
    use crate::git::worktrees::worktree_branches;
    use crate::review::checkout_types::CheckOutOutcome;
    use crate::review::types::InboxPr;
    use crate::storage::{self, StoredCheckout};
    use crate::util::host::Host;

    fn git_in(fixture: &Fixture, dir: &str, args: &[&str]) -> String {
        fixture.repo.git_in(Path::new(dir), args)
    }

    /// Commit a new file in the worktree at `dir`, as an agent would.
    fn commit_in(fixture: &Fixture, dir: &str, file: &str) -> String {
        std::fs::write(Path::new(dir).join(file), "work").unwrap();
        git_in(fixture, dir, &["add", file]);
        git_in(fixture, dir, &["commit", "-q", "-m", file]);
        git_in(fixture, dir, &["rev-parse", "HEAD"])
    }

    /// Whether `path` carries the lock Pingex marks the worktrees it makes with.
    fn listed_as_pingex_made(_fixture: &Fixture, path: &str) -> bool {
        crate::git::checkouts::lock_reason(&Host::Native, Path::new(path))
            .unwrap()
            .as_deref()
            == Some(PR_WORKTREE_LOCK)
    }

    async fn set_setup(fixture: &Fixture, setup: Option<&str>) {
        let key = fixture.repo.repo.display().to_string();
        let mut settings = storage::read_repository_settings(&fixture.desk.database, &key)
            .await
            .unwrap();
        settings.setup_command = setup.map(str::to_string);
        storage::write_repository_settings(&fixture.desk.database, &settings)
            .await
            .unwrap();
    }

    fn same_dir(a: &str, b: &Path) -> bool {
        std::fs::canonicalize(a).unwrap() == std::fs::canonicalize(b).unwrap()
    }

    #[tokio::test]
    async fn a_pr_is_not_checked_out_until_check_out_and_reads_change_nothing() {
        let Some(fixture) = fixture().await else {
            return;
        };
        fixture.open_my_pr(3, "feature", "src/lib.rs");
        // Its git branch is checked out by hand: that is not the PR's worktree.
        let place = fixture.repo.repo.parent().unwrap().join("by-hand");
        fixture
            .repo
            .git(&["worktree", "add", "-q", place.to_str().unwrap(), "feature"]);

        assert_eq!(fixture.desk.pr_checkout(3).await.unwrap(), None);
        assert!(fixture.desk.pr_checkouts().await.unwrap().is_empty());
        let rows = storage::read_checkouts(&fixture.desk.database, &fixture.desk.repo_key())
            .await
            .unwrap();
        assert!(rows.is_empty(), "wrote {rows:?}");
    }

    #[tokio::test]
    async fn someone_elses_pr_gets_a_locked_worktree_on_its_own_pr_branch() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(9, "feature", "src/lib.rs");

        let CheckOutOutcome::CheckedOut {
            checkout, created, ..
        } = fixture.check_out_now(9).await
        else {
            panic!("checked out");
        };

        assert!(created);
        assert!(!checkout.adopted);
        assert_eq!(checkout.git_branch.as_deref(), Some("pr/9"));
        let path = checkout.path.clone();
        assert!(path.ends_with("worktrees/repo/pr-9"), "{path}");
        assert_eq!(git_in(&fixture, &path, &["rev-parse", "HEAD"]), pr.head_sha);
        assert!(listed_as_pingex_made(&fixture, &path));
        assert_eq!(fixture.repo.git(&["status", "--porcelain"]), "");
        assert_eq!(
            fixture
                .repo
                .git(&["config", "--default", "none", "--get", "branch.pr/9.remote"]),
            "none",
            "a plain git push has nowhere to go"
        );
        assert_eq!(
            fixture.desk.pr_checkout(9).await.unwrap(),
            Some(checkout),
            "the checkout is remembered"
        );
    }

    #[tokio::test]
    async fn my_pr_gets_a_locked_worktree_on_its_git_branch_that_a_plain_push_updates() {
        let Some(fixture) = fixture().await else {
            return;
        };
        fixture.open_my_pr(12, "jo-r/focus-rings", "src/lib.rs");
        fixture.repo.git(&["branch", "-D", "jo-r/focus-rings"]);
        let pr_head = fixture.repo.git(&["rev-parse", "origin/jo-r/focus-rings"]);

        let checkout = fixture.check_out(12).await;

        assert!(!checkout.adopted);
        let path = checkout.path;
        assert!(path.ends_with("worktrees/repo/jo-r-focus-rings"), "{path}");
        assert_eq!(
            git_in(&fixture, &path, &["branch", "--show-current"]),
            "jo-r/focus-rings"
        );
        assert_eq!(git_in(&fixture, &path, &["rev-parse", "HEAD"]), pr_head);
        assert!(listed_as_pingex_made(&fixture, &path), "the same kind");

        let tip = commit_in(&fixture, &path, "measure.txt");
        git_in(&fixture, &path, &["push", "--quiet"]);
        let remote = git_in(
            &fixture,
            &path,
            &["ls-remote", "origin", "refs/heads/jo-r/focus-rings"],
        );
        assert!(remote.starts_with(&tip), "{remote}");
    }

    #[tokio::test]
    async fn my_fork_pr_from_main_does_not_land_on_my_main() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_my_pr(16, "patch", "src/lib.rs");
        fixture.relist(&InboxPr {
            head_ref: "main".into(),
            head_repo: Some("jo-r/desk".into()),
            ..pr
        });
        let local_main = fixture.repo.git(&["rev-parse", "main"]);

        let checkout = fixture.check_out(16).await;

        assert_eq!(checkout.git_branch.as_deref(), Some("pr/16"));
        assert_eq!(fixture.repo.git(&["rev-parse", "main"]), local_main);
        assert_eq!(fixture.repo.git(&["branch", "--show-current"]), "main");
    }

    #[tokio::test]
    async fn a_pr_the_inbox_has_not_synced_is_read_from_github() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(23, "feature", "src/lib.rs");
        fixture.github.put_detail(
            23,
            crate::review::types::PrDetail {
                summary: crate::review::types::PrSummary {
                    number: 23,
                    author: pr.author.clone(),
                    head_ref: pr.head_ref.clone(),
                    head_repo: Some(SLUG.to_string()),
                    ..Default::default()
                },
                head_sha: pr.head_sha.clone(),
                ..Default::default()
            },
        );

        let outcome = fixture.desk.check_out(23).await.unwrap();

        let CheckOutOutcome::CheckedOut { checkout, .. } = outcome else {
            panic!("checked out");
        };
        assert_eq!(checkout.git_branch.as_deref(), Some("pr/23"));
    }

    #[tokio::test]
    async fn setup_runs_in_a_new_worktree_only_once_whoever_owns_the_pr() {
        let Some(fixture) = fixture().await else {
            return;
        };
        set_setup(&fixture, Some("echo ran >> ../setup.log")).await;
        for (number, branch, mine) in [(4, "theirs", false), (5, "ours", true)] {
            if mine {
                fixture.open_my_pr(number, branch, "src/lib.rs");
            } else {
                fixture.open_pr(number, branch, "src/lib.rs");
            }
            let number = i64::from(number);

            let created = fixture.check_out(number).await;
            assert_eq!(
                created.setup_pending.as_deref(),
                Some("echo ran >> ../setup.log")
            );
            fixture.desk.run_checkout_setup(number).await.unwrap();
            fixture.desk.run_checkout_setup(number).await.unwrap();
            let again = fixture.check_out(number).await;

            assert_eq!(again.setup_pending, None);
            let log = Path::new(&created.path).parent().unwrap().join("setup.log");
            assert_eq!(std::fs::read_to_string(&log).unwrap(), "ran\n");
            std::fs::remove_file(log).unwrap();
        }
    }

    #[tokio::test]
    async fn checking_out_again_fast_forwards_a_clean_worktree_and_leaves_a_dirty_one() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(5, "feature", "src/lib.rs");
        let path = fixture.check_out(5).await.path;

        let second = fixture.push_to_pr(5, "feature", "src/two.rs");
        fixture.relist(&InboxPr {
            head_sha: second.clone(),
            ..pr.clone()
        });
        let CheckOutOutcome::CheckedOut {
            checkout: followed,
            created,
            ..
        } = fixture.check_out_now(5).await
        else {
            panic!("checked out");
        };
        assert!(!created);
        assert_eq!(followed.head_sha.as_deref(), Some(second.as_str()));
        assert_eq!((followed.ahead, followed.behind), (0, 0));

        std::fs::write(Path::new(&path).join("scratch.txt"), "mine").unwrap();
        let third = fixture.push_to_pr(5, "feature", "src/three.rs");
        fixture.relist(&InboxPr {
            head_sha: third,
            ..pr.clone()
        });
        let stayed = fixture.check_out(5).await;
        assert_eq!(stayed.head_sha.as_deref(), Some(second.as_str()));
        assert_eq!(stayed.behind, 1);
        assert!(stayed.dirty);
        assert!(Path::new(&path).join("scratch.txt").exists());
    }

    #[tokio::test]
    async fn a_commit_in_a_worktree_survives_the_pr_moving_on() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(6, "feature", "src/lib.rs");
        let path = fixture.check_out(6).await.path;
        let mine = commit_in(&fixture, &path, "measure.txt");

        let second = fixture.push_to_pr(6, "feature", "src/two.rs");
        fixture.relist(&InboxPr {
            head_sha: second,
            ..pr.clone()
        });
        let state = fixture.check_out(6).await;

        assert_eq!(state.head_sha.as_deref(), Some(mine.as_str()));
        assert_eq!((state.ahead, state.behind), (1, 1));
        assert_eq!(git_in(&fixture, &path, &["rev-parse", "pr/6"]), mine);
    }

    #[tokio::test]
    async fn a_stale_local_branch_fast_forwards_and_a_diverged_one_is_not_moved() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_my_pr(17, "stale", "src/lib.rs");
        let new_head = fixture.push_to_pr(17, "stale", "src/two.rs");
        fixture.repo.git(&["branch", "-f", "stale", &pr.head_sha]);
        fixture.relist(&InboxPr {
            head_sha: new_head.clone(),
            ..pr
        });
        let stale = fixture.check_out(17).await;
        assert_eq!(stale.head_sha.as_deref(), Some(new_head.as_str()));

        let other = fixture.open_my_pr(18, "diverged", "src/other.rs");
        let remote_head = fixture.push_to_pr(18, "diverged", "src/three.rs");
        fixture
            .repo
            .git(&["branch", "-f", "diverged", &other.head_sha]);
        fixture.repo.switch("diverged");
        let local = fixture.repo.commit("src/local.rs", "local", "local");
        fixture.repo.switch("main");
        fixture.relist(&InboxPr {
            head_sha: remote_head,
            ..other
        });

        let state = fixture.check_out(18).await;

        assert_eq!(state.head_sha.as_deref(), Some(local.as_str()));
        assert_eq!((state.ahead, state.behind), (1, 1));
        assert_eq!(fixture.repo.git(&["rev-parse", "diverged"]), local);
    }

    #[tokio::test]
    async fn a_worktree_whose_directory_was_deleted_is_made_again() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(19, "feature", "src/lib.rs");
        let path = fixture.check_out(19).await.path;
        std::fs::remove_dir_all(&path).unwrap();
        assert_eq!(fixture.desk.pr_checkout(19).await.unwrap(), None);

        let again = fixture.check_out(19).await;

        assert_eq!(again.path, path);
        assert_eq!(git_in(&fixture, &path, &["rev-parse", "HEAD"]), pr.head_sha);
    }

    #[tokio::test]
    async fn a_review_checkout_from_before_is_adopted_with_its_commits_and_changes() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(20, "feature", "src/lib.rs");
        let place = fixture.repo.repo.parent().unwrap().join("old-checkout");
        let path = place.to_str().unwrap().to_string();
        fixture.repo.git(&[
            "worktree",
            "add",
            "-q",
            "--detach",
            "--lock",
            "--reason",
            "pingex review checkout",
            &path,
            &pr.head_sha,
        ]);
        let key = fixture.repo.repo.display().to_string();
        storage::write_checkout(
            &fixture.desk.database,
            &key,
            &StoredCheckout {
                step: "review_checkout".into(),
                git_branch: None,
                ..StoredCheckout::new(20, &path, "", &pr.head_sha)
            },
        )
        .await
        .unwrap();
        let committed = commit_in(&fixture, &path, "measure.txt");
        std::fs::write(place.join("scratch.txt"), "output").unwrap();

        let state = fixture.check_out(20).await;

        assert_eq!(state.path, path);
        assert!(!state.adopted);
        assert!(state.dirty);
        assert_eq!(state.ahead, 1);
        assert_eq!(state.git_branch.as_deref(), Some("pr/20"));
        assert_eq!(git_in(&fixture, &path, &["rev-parse", "pr/20"]), committed);
        assert!(place.join("scratch.txt").exists());
        assert_eq!(
            crate::git::checkouts::lock_reason(&Host::Native, &place)
                .unwrap()
                .as_deref(),
            Some(PR_WORKTREE_LOCK)
        );
    }

    #[tokio::test]
    async fn my_branch_in_another_worktree_is_offered_for_adoption_and_never_moved() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_my_pr(8, "feature", "src/lib.rs");
        let place = fixture.repo.repo.parent().unwrap().join("mine");
        fixture
            .repo
            .git(&["worktree", "add", "-q", place.to_str().unwrap(), "feature"]);

        let CheckOutOutcome::BranchInUse {
            number,
            branch,
            path,
            main_clone,
        } = fixture.check_out_now(8).await
        else {
            panic!("the branch is in use");
        };
        assert_eq!((number, branch.as_str(), main_clone), (8, "feature", false));
        assert!(same_dir(&path, &place));
        assert_eq!(fixture.desk.pr_checkout(8).await.unwrap(), None);

        let adopted = fixture.desk.adopt_worktree(8, path.clone()).await.unwrap();
        assert!(adopted.adopted);
        assert!(adopted.on_pr_branch);
        assert!(!listed_as_pingex_made(&fixture, &path));

        // GitHub moves on; an adopted worktree stays where it is.
        let second = fixture.github_moves(8, "feature");
        fixture.relist(&InboxPr {
            head_sha: second,
            ..pr.clone()
        });
        let again = fixture.check_out(8).await;
        assert_eq!(again.head_sha.as_deref(), Some(pr.head_sha.as_str()));
        assert_eq!(again.behind, 1);

        assert!(fixture.desk.remove_pr_worktree(8).await.is_ok());
        assert!(place.exists(), "an adopted worktree is only forgotten");
    }

    #[tokio::test]
    async fn my_branch_in_the_main_clone_can_be_adopted_and_is_never_moved() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_my_pr(21, "feature", "src/lib.rs");
        fixture.repo.switch("feature");

        let CheckOutOutcome::BranchInUse {
            path, main_clone, ..
        } = fixture.check_out_now(21).await
        else {
            panic!("the branch is in use");
        };
        assert!(main_clone);
        let adopted = fixture.desk.adopt_worktree(21, path).await.unwrap();
        assert!(adopted.main_checkout && adopted.adopted);

        let second = fixture.push_to_pr(21, "feature", "src/two.rs");
        fixture.repo.git(&["branch", "-f", "feature", &pr.head_sha]);
        fixture.repo.switch("feature");
        fixture.relist(&InboxPr {
            head_sha: second,
            ..pr.clone()
        });
        let state = fixture.check_out(21).await;

        assert_eq!(state.head_sha.as_deref(), Some(pr.head_sha.as_str()));
        assert_eq!(state.behind, 1);
    }

    #[tokio::test]
    async fn adopting_a_worktree_on_another_branch_is_refused() {
        let Some(fixture) = fixture().await else {
            return;
        };
        fixture.open_my_pr(24, "feature", "src/lib.rs");
        fixture.desk.sync_inbox().await.unwrap();

        let main = fixture.desk.repo_key();
        let refused = fixture.desk.adopt_worktree(24, main).await;

        assert!(refused.unwrap_err().contains("not feature"));
    }

    #[tokio::test]
    async fn undoing_a_check_out_removes_the_worktree_and_its_branch() {
        let Some(fixture) = fixture().await else {
            return;
        };
        fixture.open_pr(10, "feature", "src/lib.rs");
        let path = fixture.check_out(10).await.path;

        fixture.desk.remove_pr_worktree(10).await.unwrap();

        assert!(!Path::new(&path).exists());
        assert_eq!(fixture.desk.pr_checkout(10).await.unwrap(), None);
        assert_eq!(
            worktree_branches(&Host::Native, &fixture.repo.repo)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(fixture.repo.git(&["branch", "--list", "pr/10"]), "");
    }

    #[tokio::test]
    async fn undo_refuses_a_worktree_that_holds_work() {
        let Some(fixture) = fixture().await else {
            return;
        };
        fixture.open_pr(11, "feature", "src/lib.rs");
        let path = fixture.check_out(11).await.path;
        let committed = commit_in(&fixture, &path, "measure.txt");

        assert_eq!(
            fixture.desk.try_remove_worktree(&path).await.unwrap(),
            Removal::KeptLocalWork
        );
        assert!(fixture.desk.remove_pr_worktree(11).await.is_err());

        assert_eq!(git_in(&fixture, &path, &["rev-parse", "HEAD"]), committed);
        assert!(fixture.desk.pr_checkout(11).await.unwrap().is_some());
        assert!(listed_as_pingex_made(&fixture, &path), "still locked");
    }

    #[tokio::test]
    async fn gitignored_build_output_does_not_keep_a_worktree_alive() {
        let Some(fixture) = fixture().await else {
            return;
        };
        fixture.repo.commit(".gitignore", "target/\n", "ignore");
        fixture.open_pr(22, "feature", "src/lib.rs");
        let path = fixture.check_out(22).await.path;
        let build = Path::new(&path).join("target");
        std::fs::create_dir_all(&build).unwrap();
        std::fs::write(build.join("output.bin"), "built").unwrap();

        fixture.desk.remove_pr_worktree(22).await.unwrap();

        assert!(!Path::new(&path).exists());
    }

    #[tokio::test]
    async fn a_read_keeps_the_row_of_a_worktree_whose_directory_is_gone() {
        let Some(fixture) = fixture().await else {
            return;
        };
        fixture.open_pr(25, "feature", "src/lib.rs");
        let path = fixture.check_out(25).await.path;
        std::fs::remove_dir_all(&path).unwrap();

        assert!(fixture.desk.pr_checkouts().await.unwrap().is_empty());

        let row = storage::read_checkout(&fixture.desk.database, &fixture.desk.repo_key(), 25)
            .await
            .unwrap();
        assert!(row.is_some(), "a read writes nothing");
    }
}
