//! Checking out a whole stack: a local git branch for each open PR, tracking
//! GitHub's, and one worktree, on the top one when it is new.
//!
//! One worktree, not one per PR: a restack rebases every git branch inside one
//! directory, and git refuses a branch checked out in another worktree. Each
//! open PR gets a row naming that worktree and the stack; switching PR
//! switches the worktree's branch (`switch_stack_branch`).
//!
//! Nothing here moves work. A local git branch that has diverged from GitHub,
//! or is checked out and behind it, stays where it is and is reported. Safe to
//! run again: it only fills in what is missing.
//!
//! The provider's own stack tool (`gh stack`) is then made to track the stack
//! in its worktree, when it is installed, so it works there too. That is best
//! effort: a failure is reported, never fatal.

use std::path::Path;

use super::ReviewDesk;
use crate::git::{checkouts as checkouts_git, stack as git};
use crate::review::checkout_types::{CheckOutOutcome, PrCheckout};
use crate::review::provider::LocalStack;
use crate::review::stack_types::{KeptBranch, StackInput, StackInputPr};
use crate::review::worktree_context::{stack_position, worktree_context, WorktreeContext};
use crate::review::worktree_purpose::WorktreePurpose;
use crate::storage::{self, StoredCheckout};

impl ReviewDesk {
    /// Check out every open PR of `input`: make their local git branches,
    /// then give the stack a worktree unless it has one. A stack whose git
    /// branch is checked out somewhere Pingex doesn't know of is
    /// `BranchInUse`.
    pub(super) async fn check_out_stack_input(
        &self,
        input: StackInput,
    ) -> Result<CheckOutOutcome, String> {
        let Some(top) = input.open_prs().last().map(|pr| (*pr).clone()) else {
            return Err("This stack has no open pull requests".to_string());
        };
        // A stopped restack leaves its worktree detached, where it can't be
        // found by branch, and needs every git branch left free.
        if let Some(run) = self.stored_restack(input.number).await? {
            return Err(format!(
                "A restack is stopped in {}. Continue or abort it first",
                run.worktree
            ));
        }
        let existing = self.stack_row(&input).await?;
        if existing.is_none() {
            let moving = input.clone();
            let in_use = self
                .blocking("Reading worktrees failed", move |desk| {
                    let open = moving.open_prs();
                    let branches: Vec<&str> = open.iter().map(|pr| pr.head_ref.as_str()).collect();
                    desk.branch_in_use(&branches)
                })
                .await?;
            if let Some(found) = in_use {
                let number = input
                    .open_prs()
                    .iter()
                    .find(|pr| pr.head_ref == found.branch)
                    .map_or(top.number, |pr| pr.number);
                return Ok(found.outcome(number));
            }
        }
        let (created_branches, kept) = self.stack_branches(&input).await?;
        let (path, adopted, setup_done, created) = match existing {
            Some(row) => (row.path, row.adopted, row.setup_done, false),
            None => {
                let path = self.new_worktree_path(&top.head_ref).await?;
                let (moving_path, moving_top) = (path.clone(), top.clone());
                self.blocking("Creating the worktree failed", move |desk| {
                    checkouts_git::add_pr_worktree(
                        &desk.host,
                        desk.dir(),
                        &moving_path,
                        &moving_top.head_ref,
                        &moving_top.head_sha,
                    )
                })
                .await?;
                let setup = storage::read_repository_settings(&self.database, &self.repo_key())
                    .await?
                    .setup_command;
                (path, false, setup.is_none(), true)
            }
        };
        let rows = stack_rows(&input, &path, adopted, setup_done);
        if created {
            self.write_rows_or_undo(&rows, &path, None).await?;
        } else {
            for row in &rows {
                storage::write_checkout(&self.database, &self.repo_key(), row).await?;
            }
        }
        let checkout = self
            .settle_stack(&input, &path, adopted, top.number)
            .await?;
        let (local_stack, local_stack_warning) = if adopted {
            (false, None)
        } else {
            self.track_locally(path.clone(), input.clone()).await
        };
        let purpose = WorktreePurpose::stack(&self.repo_key(), &input).tracked_locally(local_stack);
        self.record_stack_worktree(&path, &purpose).await?;
        Ok(CheckOutOutcome::CheckedOut {
            checkout,
            created,
            created_branches,
            kept,
            local_stack_warning,
        })
    }

    /// Make `input`'s worktree at `path`, found on one of its git branches,
    /// the stack's, as adopted. Returns PR `number`'s checkout.
    pub(super) async fn adopt_stack_worktree(
        &self,
        input: StackInput,
        number: i64,
        path: String,
    ) -> Result<PrCheckout, String> {
        self.stack_branches(&input).await?;
        let rows = stack_rows(&input, &path, true, true);
        for row in &rows {
            storage::write_checkout(&self.database, &self.repo_key(), row).await?;
        }
        let purpose = WorktreePurpose::stack(&self.repo_key(), &input);
        self.record_stack_worktree(&path, &purpose).await?;
        self.settle_stack(&input, &path, true, number).await
    }

    /// The stack's worktree as stored: a row of one of its open PRs whose
    /// directory is there, on that PR's own git branch for a row from before
    /// rows named their stack.
    async fn stack_row(&self, input: &StackInput) -> Result<Option<StoredCheckout>, String> {
        for pr in input.open_prs() {
            let Some(row) = self.live_row(pr.number).await? else {
                continue;
            };
            let ours = match row.stack_number {
                Some(stack) => stack == input.number,
                None => row.git_branch.as_deref() == Some(pr.head_ref.as_str()),
            };
            if ours {
                return Ok(Some(row));
            }
        }
        Ok(None)
    }

    /// Make or catch up every open PR's local git branch, tracking GitHub's.
    async fn stack_branches(
        &self,
        input: &StackInput,
    ) -> Result<(Vec<String>, Vec<KeptBranch>), String> {
        let moving = input.clone();
        self.blocking("Checking out the stack failed", move |desk| {
            let open = moving.open_prs();
            desk.ensure_heads(&open)?;
            let (created, kept) =
                git::locked(&desk.host, desk.dir(), || desk.local_branches(&open))?;
            let remote = desk.remote()?;
            for pr in &open {
                checkouts_git::track_remote_branch(&desk.host, desk.dir(), &pr.head_ref, &remote)?;
            }
            Ok((created, kept))
        })
        .await
    }

    /// Bring the stack worktree at `path` level with GitHub for the PR whose
    /// branch it is on, unless it is adopted, and return PR `number`'s
    /// checkout.
    async fn settle_stack(
        &self,
        input: &StackInput,
        path: &str,
        adopted: bool,
        number: i64,
    ) -> Result<PrCheckout, String> {
        if !adopted {
            let moving_path = path.to_string();
            let current = self
                .blocking("Reading the worktree failed", move |desk| {
                    checkouts_git::branch_of(&desk.host, Path::new(&moving_path))
                })
                .await
                .ok()
                .flatten();
            if let Some(pr) = input
                .open_prs()
                .into_iter()
                .find(|pr| Some(&pr.head_ref) == current.as_ref())
            {
                self.synced_pr_checkout(pr.number, Some(pr.head_sha.clone()))
                    .await?;
            }
        }
        let row = self
            .live_row(number)
            .await?
            .ok_or_else(|| "The stack's worktree is gone".to_string())?;
        self.describe(row).await
    }

    /// Have the provider's stack tool track `input`'s stack in the worktree at
    /// `path`, keeping the worktree on its git branch. Returns whether it
    /// tracks it, and why not when it should have. A worktree that is detached
    /// or on a branch outside the stack is left alone: the tool would switch
    /// it.
    async fn track_locally(&self, path: String, input: StackInput) -> (bool, Option<String>) {
        self.blocking("Tracking the stack with gh stack failed", move |desk| {
            Ok(desk.track_locally_now(Path::new(&path), &input))
        })
        .await
        .unwrap_or_else(|error| (false, Some(error)))
    }

    fn track_locally_now(&self, dir: &Path, input: &StackInput) -> (bool, Option<String>) {
        let open = input.open_prs();
        let before = match checkouts_git::branch_of(&self.host, dir) {
            Ok(branch) => branch,
            Err(error) => return (false, Some(error)),
        };
        let Some(current) = before
            .as_deref()
            .and_then(|branch| open.iter().find(|pr| pr.head_ref == branch))
        else {
            return (false, None);
        };
        let branches: Vec<String> = open.iter().map(|pr| pr.head_ref.clone()).collect();
        let result = self.github.track_stack_locally(&LocalStack {
            worktree: dir,
            number: input.number,
            current_pr: current.number,
            branches: &branches,
        });
        let mut warnings = Vec::new();
        let after = checkouts_git::branch_of(&self.host, dir).ok().flatten();
        if after.as_deref() != Some(current.head_ref.as_str()) {
            match git::switch(&self.host, dir, &current.head_ref) {
                Ok(()) => warnings.push(format!(
                    "gh stack moved the worktree off `{}`; it is back on it",
                    current.head_ref
                )),
                Err(error) => warnings.push(format!(
                    "gh stack moved the worktree off `{}` and switching back failed: {error}",
                    current.head_ref
                )),
            }
        }
        let tracked = match result {
            Ok(tracked) => tracked,
            Err(error) => {
                warnings.push(format!("gh stack doesn't track this stack: {error}"));
                false
            }
        };
        (tracked, (!warnings.is_empty()).then(|| warnings.join(". ")))
    }

    /// Remember that the worktree at `path` holds this stack, for listings and
    /// worktree context. The main clone is never a Stack worktree.
    ///
    /// A purpose that doesn't say the stack tool tracks the stack keeps the
    /// stored word that it does while the git branches are the same: a read
    /// of the stack (`stack_overview`) doesn't ask the tool.
    pub(super) async fn record_stack_worktree(
        &self,
        path: &str,
        purpose: &WorktreePurpose,
    ) -> Result<(), String> {
        if path == self.repo_key() {
            return Ok(());
        }
        let stored = storage::read_worktree_purpose(&self.database, path).await?;
        let purpose = match &stored {
            Some(
                stored @ WorktreePurpose::Stack {
                    local_stack: true, ..
                },
            ) if stored.same_branches(purpose) => purpose.clone().tracked_locally(true),
            _ => purpose.clone(),
        };
        if stored.as_ref() == Some(&purpose) {
            return Ok(());
        }
        storage::write_stack_worktree(&self.database, path, &purpose).await
    }

    /// What to tell an agent starting in the worktree at `path`, when it is a
    /// PR worktree or a Stack worktree now. The stored purpose is
    /// checked against git, so a worktree since put to other use gets none.
    pub(crate) async fn worktree_context(
        &self,
        path: String,
    ) -> Result<Option<WorktreeContext>, String> {
        let Some(purpose) = storage::read_worktree_purpose(&self.database, &path).await? else {
            return Ok(None);
        };
        if !self.host.is_dir(&path) {
            return Ok(None);
        }
        // A PR's worktree holds it while it is on the PR's git branch.
        let pr_branch = match &purpose {
            WorktreePurpose::ManagedPr { number, .. } => {
                storage::read_checkout(&self.database, &self.repo_key(), *number)
                    .await?
                    .and_then(|row| row.git_branch)
            }
            WorktreePurpose::Stack { .. } => None,
        };
        self.blocking("Reading the worktree failed", move |desk| {
            let dir = Path::new(&path);
            let branch = checkouts_git::branch_of(&desk.host, dir)?;
            let fits = match &purpose {
                WorktreePurpose::ManagedPr { .. } => {
                    pr_branch.is_none() || branch.is_none() || branch == pr_branch
                }
                WorktreePurpose::Stack { .. } => {
                    checkouts_git::is_linked_worktree(&desk.host, dir)?
                        && purpose.holds_on(branch.as_deref())
                }
            };
            Ok(fits.then(|| WorktreeContext {
                text: worktree_context(&purpose, branch.as_deref()),
                position: stack_position(&purpose, branch.as_deref()),
                purpose,
            }))
        })
        .await
    }

    /// Catch each git branch up with GitHub where nothing can be lost, and make
    /// the missing ones. Returns the ones made and the ones left as they are.
    /// Call after `ensure_heads`, holding the repository's lock.
    fn local_branches(
        &self,
        open: &[&StackInputPr],
    ) -> Result<(Vec<String>, Vec<KeptBranch>), String> {
        let mut created = Vec::new();
        let mut kept = Vec::new();
        for pr in open {
            if git::branch_tip(&self.host, self.dir(), &pr.head_ref)?.is_none() {
                git::create_branch(&self.host, self.dir(), &pr.head_ref, &pr.head_sha)?;
                created.push(pr.head_ref.clone());
            } else if let Err(reason) = self.catch_up_branches(&[*pr]) {
                kept.push(KeptBranch {
                    branch: pr.head_ref.clone(),
                    reason,
                });
            }
        }
        Ok((created, kept))
    }
}

/// A row per open PR of `input`, all checked out in the stack's worktree at
/// `path`, each on its own git branch.
fn stack_rows(
    input: &StackInput,
    path: &str,
    adopted: bool,
    setup_done: bool,
) -> Vec<StoredCheckout> {
    input
        .open_prs()
        .into_iter()
        .map(|pr: &StackInputPr| StoredCheckout {
            setup_done,
            adopted,
            stack_number: Some(input.number),
            ..StoredCheckout::new(pr.number, path, &pr.head_ref, &pr.head_sha)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::super::stack_support::StackFixture;
    use crate::review::checkout_types::{CheckOutOutcome, PrCheckout};
    use crate::review::fake::{LocalStackTool, TrackedLocally};
    use crate::review::stack_types::KeptBranch;
    use crate::review::worktree_context::StackPosition;
    use crate::review::worktree_purpose::WorktreePurpose;
    use crate::storage;
    use crate::util::host::Host;

    fn upstream(stack: &StackFixture, branch: &str) -> String {
        stack.git(&["config", "--get", &format!("branch.{branch}.remote")])
    }

    /// A fixture whose stack exists only on GitHub, as for a teammate's stack.
    async fn remote_only() -> Option<StackFixture> {
        let stack = StackFixture::new().await?;
        stack.git(&[
            "branch",
            "-q",
            "-D",
            "feat/driver",
            "feat/claude",
            "feat/maki",
        ]);
        Some(stack)
    }

    struct Checked {
        checkout: PrCheckout,
        created: bool,
        created_branches: Vec<String>,
        kept: Vec<KeptBranch>,
        warning: Option<String>,
    }

    fn checked(outcome: CheckOutOutcome) -> Checked {
        match outcome {
            CheckOutOutcome::CheckedOut {
                checkout,
                created,
                created_branches,
                kept,
                local_stack_warning,
            } => Checked {
                checkout,
                created,
                created_branches,
                kept,
                warning: local_stack_warning,
            },
            other => panic!("not checked out: {other:?}"),
        }
    }

    #[tokio::test]
    async fn checking_out_a_stack_pr_checks_out_the_whole_stack_in_one_worktree_on_top() {
        let Some(stack) = remote_only().await else {
            return;
        };

        let result = checked(stack.check_out(412).await);

        assert!(result.created);
        assert_eq!(
            result.created_branches,
            vec!["feat/driver", "feat/claude", "feat/maki"]
        );
        assert!(result.kept.is_empty());
        assert_eq!(result.checkout.number, 418, "the stack's top PR");
        assert_eq!(result.checkout.git_branch.as_deref(), Some("feat/maki"));
        assert_eq!(result.checkout.stack, Some(7));
        assert!(result.checkout.on_pr_branch);
        assert!(!result.checkout.adopted);
        for branch in ["feat/driver", "feat/claude", "feat/maki"] {
            assert_eq!(stack.tip(branch), stack.remote_tip(branch));
            assert_eq!(upstream(&stack, branch), "origin");
        }
        let worktrees = stack.git(&["worktree", "list", "--porcelain"]);
        assert_eq!(worktrees.matches("worktree ").count(), 2, "{worktrees}");
        let path = result.checkout.path;
        assert!(path.ends_with("worktrees/repo/feat-maki"), "{path}");
        assert_eq!(
            crate::git::checkouts::lock_reason(&Host::Native, Path::new(&path))
                .unwrap()
                .as_deref(),
            Some(crate::git::checkouts::PR_WORKTREE_LOCK)
        );
        for number in [412, 415, 418] {
            let lower = stack.desk().pr_checkout(number).await.unwrap().unwrap();
            assert_eq!(lower.path, path);
            assert_eq!(lower.on_pr_branch, number == 418);
            assert_eq!((lower.ahead, lower.behind), (0, 0));
        }
    }

    #[tokio::test]
    async fn checking_out_the_stack_by_its_number_does_the_same() {
        let Some(stack) = remote_only().await else {
            return;
        };
        stack.desk().sync_inbox().await.unwrap();

        let result = checked(stack.desk().check_out_stack(7).await.unwrap());

        assert!(result.created);
        assert_eq!(result.checkout.git_branch.as_deref(), Some("feat/maki"));
        assert!(stack.desk().check_out_stack(8).await.is_err());
    }

    #[tokio::test]
    async fn a_checked_out_stack_gives_its_worktree_context_until_it_leaves_the_stack() {
        let Some(stack) = remote_only().await else {
            return;
        };
        let path = stack.checked_out().await.path;

        let context = stack
            .desk()
            .worktree_context(path.clone())
            .await
            .unwrap()
            .expect("a stack worktree has context");
        assert_eq!(
            context.purpose,
            WorktreePurpose::stack(&stack.desk().repo_key(), &stack.input())
        );
        assert!(
            context
                .text
                .contains("branch `feat/maki`, based on #415 ← the worktree is on this branch"),
            "{}",
            context.text
        );
        assert_eq!(
            context.position,
            Some(StackPosition {
                position: 3,
                total: 3
            })
        );

        // On a git branch outside the stack, it is no longer the stack's.
        stack.git_in(Path::new(&path), &["switch", "-q", "-c", "elsewhere"]);
        assert_eq!(stack.desk().worktree_context(path).await.unwrap(), None);
        // The main clone is never a Stack worktree.
        let main = stack.desk().repo_key();
        assert_eq!(stack.desk().worktree_context(main).await.unwrap(), None);
    }

    #[tokio::test]
    async fn without_gh_stack_a_checkout_says_nothing_about_it() {
        let Some(stack) = remote_only().await else {
            return;
        };
        let result = checked(stack.check_out(418).await);
        let path = result.checkout.path;

        assert_eq!(result.warning, None);
        assert_eq!(
            stack.inner.github.tracked_locally(),
            vec![TrackedLocally {
                worktree: Path::new(&path).to_path_buf(),
                number: 7,
                current_pr: 418,
                branches: vec![
                    "feat/driver".into(),
                    "feat/claude".into(),
                    "feat/maki".into()
                ],
            }]
        );
        let context = stack.desk().worktree_context(path).await.unwrap().unwrap();
        assert!(!context.text.contains("gh stack"), "{}", context.text);
    }

    #[tokio::test]
    async fn gh_stack_tracks_the_checked_out_stack_and_a_read_keeps_that() {
        let Some(stack) = remote_only().await else {
            return;
        };
        stack
            .inner
            .github
            .set_local_stack_tool(LocalStackTool::Tracks);
        let result = checked(stack.check_out(418).await);
        let path = result.checkout.path;
        assert_eq!(result.warning, None);

        // A read of the stack doesn't ask gh stack, and keeps its word.
        let read = WorktreePurpose::stack(&stack.desk().repo_key(), &stack.input());
        stack
            .desk()
            .record_stack_worktree(&path, &read)
            .await
            .unwrap();

        let context = stack
            .desk()
            .worktree_context(path.clone())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(context.purpose, read.clone().tracked_locally(true));
        assert!(context.text.contains("`gh stack` tracks this stack here"));

        // Different branches are a different stack to gh stack.
        let mut fewer = stack.input();
        fewer.prs.pop();
        let changed = WorktreePurpose::stack(&stack.desk().repo_key(), &fewer);
        stack
            .desk()
            .record_stack_worktree(&path, &changed)
            .await
            .unwrap();
        let purpose = storage::read_worktree_purpose(&stack.desk().database, &path)
            .await
            .unwrap();
        assert_eq!(purpose, Some(changed));
    }

    #[tokio::test]
    async fn a_gh_stack_failure_is_a_warning_and_the_checkout_goes_ahead() {
        let Some(stack) = remote_only().await else {
            return;
        };
        stack
            .inner
            .github
            .set_local_stack_tool(LocalStackTool::Fails(
                "gh stack could not import the stack".into(),
            ));
        let result = checked(stack.check_out(418).await);

        assert!(result.created);
        assert_eq!(
            result.warning.as_deref(),
            Some("gh stack doesn't track this stack: gh stack could not import the stack")
        );
        let context = stack
            .desk()
            .worktree_context(result.checkout.path)
            .await
            .unwrap()
            .unwrap();
        assert!(!context.text.contains("`gh stack` tracks"));
    }

    #[tokio::test]
    async fn gh_stack_moving_the_worktree_is_undone() {
        let Some(stack) = remote_only().await else {
            return;
        };
        stack
            .inner
            .github
            .set_local_stack_tool(LocalStackTool::SwitchesTo("feat/claude".into()));
        let result = checked(stack.check_out(418).await);
        let path = result.checkout.path;

        assert_eq!(
            stack.git_in(Path::new(&path), &["branch", "--show-current"]),
            "feat/maki"
        );
        assert_eq!(
            result.warning.as_deref(),
            Some("gh stack moved the worktree off `feat/maki`; it is back on it")
        );
    }

    #[tokio::test]
    async fn a_worktree_off_the_stack_is_not_handed_to_gh_stack() {
        let Some(stack) = StackFixture::new().await else {
            return;
        };
        stack
            .inner
            .github
            .set_local_stack_tool(LocalStackTool::Tracks);
        let worktree = stack.worktree_on("feat/driver");
        stack.git_in(&worktree, &["switch", "-q", "--detach"]);

        let (tracked, warning) = stack.desk().track_locally_now(&worktree, &stack.input());

        assert!(!tracked);
        assert_eq!(warning, None);
        assert!(stack.inner.github.tracked_locally().is_empty());
    }

    #[tokio::test]
    async fn checking_out_a_stack_again_changes_nothing() {
        let Some(stack) = remote_only().await else {
            return;
        };
        let first = stack.checked_out().await;

        let second = checked(stack.check_out(415).await);

        assert!(!second.created);
        assert!(second.created_branches.is_empty());
        assert_eq!(second.checkout.path, first.path);
    }

    #[tokio::test]
    async fn a_stack_branch_checked_out_by_hand_is_offered_for_adoption_and_kept_as_it_is() {
        let Some(stack) = StackFixture::new().await else {
            return;
        };
        let worktree = stack.worktree_on("feat/driver");
        stack.git(&["branch", "-q", "-D", "feat/maki"]);

        let CheckOutOutcome::BranchInUse {
            number,
            branch,
            path,
            main_clone,
        } = stack.check_out(418).await
        else {
            panic!("the stack's branch is in use");
        };
        assert_eq!(
            (number, branch.as_str(), main_clone),
            (412, "feat/driver", false)
        );
        assert_eq!(stack.desk().pr_checkout(418).await.unwrap(), None);

        let adopted = stack
            .desk()
            .adopt_worktree(418, path.clone())
            .await
            .unwrap();

        assert!(adopted.adopted);
        assert_eq!(adopted.stack, Some(7));
        assert!(!adopted.on_pr_branch);
        assert!(
            stack.tip("feat/maki").len() == 40,
            "missing branches are filled in"
        );
        assert_eq!(
            stack.git_in(&worktree, &["branch", "--show-current"]),
            "feat/driver"
        );
        // Pingex doesn't switch it, and checking out again keeps it.
        assert!(stack.desk().switch_stack_branch(418).await.is_err());
        let again = checked(stack.check_out(415).await);
        assert_eq!(again.checkout.path, path);
        assert!(!again.created);
        assert!(stack.inner.github.tracked_locally().is_empty());
    }

    #[tokio::test]
    async fn a_stack_branch_in_the_main_clone_is_offered_and_never_switched() {
        let Some(stack) = StackFixture::new().await else {
            return;
        };
        stack.git(&["switch", "-q", "feat/maki"]);

        let CheckOutOutcome::BranchInUse {
            path, main_clone, ..
        } = stack.check_out(412).await
        else {
            panic!("the stack's branch is in use");
        };
        assert!(main_clone);
        stack.desk().adopt_worktree(412, path).await.unwrap();

        assert!(stack.desk().switch_stack_branch(412).await.is_err());
        assert_eq!(stack.git(&["branch", "--show-current"]), "feat/maki");
    }

    #[tokio::test]
    async fn a_diverged_branch_is_kept_and_reported_and_the_rest_go_ahead() {
        let Some(stack) = StackFixture::new().await else {
            return;
        };
        // A local commit on #415's branch, and a teammate's on GitHub.
        stack.git(&["switch", "-q", "feat/claude"]);
        stack.git(&["commit", "-q", "--allow-empty", "-m", "mine"]);
        stack.git(&["switch", "-q", "main"]);
        let local = stack.tip("feat/claude");
        let head = stack.teammate_pushes(415, "feat/claude");
        stack.relist(415, &head);

        let result = checked(stack.check_out(418).await);

        assert_eq!(result.kept.len(), 1);
        assert_eq!(result.kept[0].branch, "feat/claude");
        assert_eq!(stack.tip("feat/claude"), local);
        assert!(result.created);
    }

    #[tokio::test]
    async fn a_branch_behind_github_fast_forwards() {
        let Some(stack) = StackFixture::new().await else {
            return;
        };
        let head = stack.teammate_pushes(412, "feat/driver");
        stack.relist(412, &head);

        let result = checked(stack.check_out(418).await);

        assert!(result.kept.is_empty());
        assert_eq!(stack.tip("feat/driver"), head);
    }

    #[tokio::test]
    async fn a_stack_with_no_open_prs_is_refused() {
        let Some(stack) = StackFixture::new().await else {
            return;
        };
        for number in [412, 415, 418] {
            stack.mark_merged(number);
        }

        assert!(stack
            .desk()
            .check_out_stack_input(stack.input())
            .await
            .is_err());
        assert!(stack.desk().check_out_stack(7).await.is_err());
    }

    #[tokio::test]
    async fn a_restack_works_in_the_worktree_of_a_checked_out_stack() {
        let Some(stack) = remote_only().await else {
            return;
        };
        stack.checked_out().await;
        let moved = stack.advance_main("main.rs", "the base moved\n");

        stack.desk().restack(stack.input(), None).await.unwrap();

        for branch in ["feat/driver", "feat/claude", "feat/maki"] {
            assert!(stack.has_ancestor(branch, &moved), "{branch}");
        }
    }

    #[tokio::test]
    async fn checking_out_a_stack_is_refused_while_a_restack_is_stopped() {
        let Some(stack) = remote_only().await else {
            return;
        };
        stack.checked_out().await;
        // main and #415 both change claude.rs, so the restack stops there.
        stack.advance_main("claude.rs", "main's version\n");
        stack.desk().restack(stack.input(), None).await.unwrap();

        let refused = stack.desk().check_out(418).await;

        assert!(refused.unwrap_err().contains("restack"));
    }

    #[tokio::test]
    async fn switching_to_a_lower_pr_switches_the_stack_worktree() {
        let Some(stack) = remote_only().await else {
            return;
        };
        let path = stack.checked_out().await.path;
        let (lower, top) = (stack.tip("feat/driver"), stack.tip("feat/maki"));

        let state = stack.desk().switch_stack_branch(412).await.unwrap();

        assert_eq!(state.path, path);
        assert_eq!(state.git_branch.as_deref(), Some("feat/driver"));
        assert!(state.on_pr_branch);
        assert_eq!(
            stack.git_in(Path::new(&path), &["branch", "--show-current"]),
            "feat/driver"
        );
        assert_eq!(stack.tip("feat/driver"), lower);
        assert_eq!(stack.tip("feat/maki"), top);
        let top = stack.desk().pr_checkout(418).await.unwrap().unwrap();
        assert!(!top.on_pr_branch);
    }

    #[tokio::test]
    async fn switching_to_a_pr_with_no_local_branch_and_an_unfetched_head_does_not_hang() {
        let Some(stack) = remote_only().await else {
            return;
        };
        let path = stack.checked_out().await.path;
        stack.git(&["branch", "-q", "-D", "feat/driver"]);
        let head = stack.teammate_pushes(412, "feat/driver");
        stack.relist(412, &head);
        stack.desk().sync_inbox().await.unwrap();

        let switched = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            stack.desk().switch_stack_branch(412),
        )
        .await
        .expect("the switch finishes")
        .unwrap();

        assert_eq!(switched.git_branch.as_deref(), Some("feat/driver"));
        assert_eq!(stack.git_in(Path::new(&path), &["rev-parse", "HEAD"]), head);
    }

    #[tokio::test]
    async fn a_stack_edit_never_runs_in_the_main_clone() {
        let Some(stack) = StackFixture::new().await else {
            return;
        };
        stack.git(&["switch", "-q", "feat/claude"]);
        let tips: Vec<String> = ["feat/driver", "feat/claude", "feat/maki"]
            .iter()
            .map(|branch| stack.tip(branch))
            .collect();
        stack.advance_main("main.rs", "the base moved\n");
        stack.git(&["switch", "-q", "feat/claude"]);

        let restacked = stack.desk().restack(stack.input(), None).await;
        let _ = stack
            .desk()
            .add_stack_branch(stack.input(), "feat/extra".into(), None)
            .await;

        assert!(restacked.is_err(), "{restacked:?}");
        let after: Vec<String> = ["feat/driver", "feat/claude", "feat/maki"]
            .iter()
            .map(|branch| stack.tip(branch))
            .collect();
        assert_eq!(after, tips);
        assert_eq!(stack.git(&["branch", "--show-current"]), "feat/claude");
    }

    #[tokio::test]
    async fn a_dirty_stack_worktree_refuses_to_switch_and_stays_put() {
        let Some(stack) = remote_only().await else {
            return;
        };
        let path = stack.checked_out().await.path;
        std::fs::write(Path::new(&path).join("scratch.txt"), "dirty\n").unwrap();

        let refused = stack.desk().switch_stack_branch(412).await;

        assert!(refused.is_err());
        assert_eq!(
            stack.git_in(Path::new(&path), &["branch", "--show-current"]),
            "feat/maki"
        );
    }

    #[tokio::test]
    async fn a_stack_worktree_mid_rebase_refuses_to_switch() {
        let Some(stack) = remote_only().await else {
            return;
        };
        let path = stack.checked_out().await.path;
        let dir = Path::new(&path);
        // Stop a rebase at a conflict in maki.rs.
        stack.git(&["switch", "-q", "-c", "side", "feat/claude"]);
        stack.inner.repo.commit("maki.rs", "other\n", "other");
        stack.git(&["switch", "-q", "main"]);
        assert!(!crate::git::test_repo::git_ok(dir, &["rebase", "side"]));
        assert!(crate::git::stack::rebase_in_progress(&Host::Native, dir));

        let refused = stack.desk().switch_stack_branch(412).await;

        assert!(refused.unwrap_err().contains("rebase"));
    }

    #[tokio::test]
    async fn undoing_a_stack_check_out_removes_its_worktree_and_every_row() {
        let Some(stack) = remote_only().await else {
            return;
        };
        let path = stack.checked_out().await.path;

        stack.desk().remove_pr_worktree(415).await.unwrap();

        assert!(!Path::new(&path).exists());
        for number in [412, 415, 418] {
            assert_eq!(stack.desk().pr_checkout(number).await.unwrap(), None);
        }
        assert_ne!(stack.git(&["branch", "--list", "feat/maki"]), "");
    }
}
