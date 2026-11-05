//! Moving a PR up the checkout steps: a review checkout, a worktree, and
//! promoting the one into the other.
//!
//! A PR's step is stored per (repository, PR) in `review_checkouts`; No
//! checkout is the absence of a row. Two facts on disk override the store: my
//! own PR whose git branch is already checked out in a worktree is at the
//! Worktree step, and a stored checkout whose directory is gone is not one.
//!
//! A review checkout lives under `<home>/review-checkouts/<repository>/`, as
//! `pr-<n>` or, for a stack, `stack-<name>` shared by its PRs. The
//! repository's setup command runs once per checkout directory. A checkout
//! follows a new PR head only while it is clean. `plan_step` applies the
//! repository's review checkout policy and says what to confirm; callers
//! then call the step operation it names.

use std::path::Path;

use super::ReviewDesk;
use crate::git::{checkouts as git, objects, worktrees};
use crate::review::checkout_types::{CheckoutTarget, PrCheckout, StepConfirm, StepPlan};
use crate::review::requirements::{CheckoutStep, DeskAction};
use crate::storage::{self, ReviewCheckoutPolicy, StoredCheckout};
use crate::util::process::{run, Run};

const REVIEW_CHECKOUT: &str = "review_checkout";
const WORKTREE: &str = "worktree";

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

impl ReviewDesk {
    fn home(&self) -> Result<&str, String> {
        self.home_dir
            .as_deref()
            .ok_or_else(|| "This Home has no folder for review checkouts".to_string())
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

    /// Where `target`'s review checkout goes.
    pub(crate) fn review_checkout_path(&self, target: &CheckoutTarget) -> Result<String, String> {
        let root = self.host.join_str(self.home()?, "review-checkouts");
        let group = self.host.join_str(&root, &self.repo_name());
        let name = match &target.stack {
            Some(stack) => format!("stack-{}", segment(stack)),
            None => format!("pr-{}", target.number),
        };
        Ok(self.host.join_str(&group, &name))
    }

    /// Where a new worktree for `target` goes: the repository's worktree
    /// directory, or `<home>/worktrees/<repository>/`.
    async fn worktree_path(&self, target: &CheckoutTarget) -> Result<String, String> {
        let settings = storage::read_repository_settings(&self.database, &self.repo_key()).await?;
        let group = match settings.worktree_dir {
            Some(dir) => dir,
            None => {
                let root = self.host.join_str(self.home()?, "worktrees");
                self.host.join_str(&root, &self.repo_name())
            }
        };
        Ok(self.host.join_str(&group, &segment(&target.head_ref)))
    }

    /// Where `target`'s code is now.
    pub(crate) async fn checkout_state(
        &self,
        target: CheckoutTarget,
    ) -> Result<PrCheckout, String> {
        let key = self.repo_key();
        if let Some(stored) = storage::read_checkout(&self.database, &key, target.number).await? {
            if self.host.is_dir(&stored.path) {
                return self.describe(&target, stored).await;
            }
            storage::delete_checkout(&self.database, &key, target.number).await?;
        }
        if !target.mine {
            return Ok(PrCheckout::none(target.number));
        }
        // My PR whose git branch already lives on my disk starts at Worktree.
        let branch = target.head_ref.clone();
        let found = self
            .blocking("Reading worktrees failed", move |desk| {
                Ok(worktrees::worktree_branches(&desk.host, desk.dir())?
                    .into_iter()
                    .find(|(_, checked_out)| checked_out.as_deref() == Some(branch.as_str()))
                    .map(|(path, _)| path))
            })
            .await?;
        let Some(path) = found else {
            return Ok(PrCheckout::none(target.number));
        };
        let stored = StoredCheckout {
            pr_number: target.number,
            step: WORKTREE.to_string(),
            path,
            head_sha: None,
            git_branch: Some(target.head_ref.clone()),
            setup_done: true,
            closed_at: None,
        };
        storage::write_checkout(&self.database, &key, &stored).await?;
        self.describe(&target, stored).await
    }

    /// A stored checkout as the caller sees it, with what is on disk now.
    async fn describe(
        &self,
        target: &CheckoutTarget,
        stored: StoredCheckout,
    ) -> Result<PrCheckout, String> {
        let path = stored.path.clone();
        let head = self
            .blocking("Reading the checkout failed", move |desk| {
                git::head_of(&desk.host, Path::new(&path))
            })
            .await?;
        let review = stored.step == REVIEW_CHECKOUT;
        let setup_pending = if review && !stored.setup_done {
            storage::read_repository_settings(&self.database, &self.repo_key())
                .await?
                .setup_command
        } else {
            None
        };
        Ok(PrCheckout {
            number: target.number,
            step: if review {
                CheckoutStep::ReviewCheckout
            } else {
                CheckoutStep::Worktree
            },
            behind_head: review && head.as_deref() != Some(target.head_sha.as_str()),
            path: Some(stored.path),
            head_sha: head,
            git_branch: stored.git_branch,
            setup_pending,
        })
    }

    /// Make sure the PR head commit is in the repository, fetching its ref.
    fn ensure_commit(&self, target: &CheckoutTarget) -> Result<(), String> {
        if git::has_commit(&self.host, self.dir(), &target.head_sha)? {
            return Ok(());
        }
        let number = super::pr_number(target.number)?;
        let slug = self.github.repo()?;
        let remote = objects::remote_for_slug(&self.host, self.dir(), &slug)?
            .ok_or_else(|| format!("No remote in this repository points at {slug}"))?;
        objects::fetch_pr_heads(&self.host, self.dir(), &remote, &[number])?;
        if git::has_commit(&self.host, self.dir(), &target.head_sha)? {
            Ok(())
        } else {
            Err("The pull request's head commit could not be fetched".to_string())
        }
    }

    /// Move a clean review checkout at `path` to the PR head. Returns false,
    /// leaving it where it is, when it has local changes.
    fn follow_head(&self, path: &str, target: &CheckoutTarget) -> Result<bool, String> {
        let dir = Path::new(path);
        if git::head_of(&self.host, dir)?.as_deref() == Some(target.head_sha.as_str()) {
            return Ok(true);
        }
        if !git::is_clean(&self.host, dir)? {
            return Ok(false);
        }
        self.ensure_commit(target)?;
        git::detach_at(&self.host, self.dir(), dir, &target.head_sha)?;
        Ok(true)
    }

    /// Give `target` a review checkout at its head, unless it already has one
    /// or a worktree. An existing clean checkout follows the head.
    pub(crate) async fn ensure_review_checkout(
        &self,
        target: CheckoutTarget,
    ) -> Result<PrCheckout, String> {
        let key = self.repo_key();
        let current = self.checkout_state(target.clone()).await?;
        if current.step == CheckoutStep::Worktree {
            return Ok(current);
        }
        let path = match current.path.clone() {
            Some(path) => path,
            None => self.review_checkout_path(&target)?,
        };
        let others = storage::read_checkouts(&self.database, &key).await?;
        // Setup ran if any PR's checkout in this directory ran it.
        let shared_setup = others
            .iter()
            .any(|other| other.path == path && other.setup_done);
        let setup_command = storage::read_repository_settings(&self.database, &key)
            .await?
            .setup_command;
        let (moving_path, moving_target) = (path.clone(), target.clone());
        let created = self
            .blocking("Creating the review checkout failed", move |desk| {
                if desk.host.is_dir(&moving_path) {
                    desk.follow_head(&moving_path, &moving_target)?;
                    Ok(false)
                } else {
                    desk.ensure_commit(&moving_target)?;
                    git::add_review_checkout(
                        &desk.host,
                        desk.dir(),
                        &moving_path,
                        &moving_target.head_sha,
                    )?;
                    Ok(true)
                }
            })
            .await?;
        let setup_done = if created {
            setup_command.is_none()
        } else {
            shared_setup || setup_command.is_none()
        };
        let stored = StoredCheckout {
            pr_number: target.number,
            step: REVIEW_CHECKOUT.to_string(),
            path,
            head_sha: Some(target.head_sha.clone()),
            git_branch: None,
            setup_done,
            closed_at: None,
        };
        storage::write_checkout(&self.database, &key, &stored).await?;
        self.describe(&target, stored).await
    }

    /// Run the repository's setup command in PR `number`'s new review
    /// checkout. Does nothing once it has run there.
    pub(crate) async fn run_checkout_setup(&self, number: i64) -> Result<(), String> {
        let key = self.repo_key();
        let Some(stored) = storage::read_checkout(&self.database, &key, number).await? else {
            return Err("This pull request has no review checkout".to_string());
        };
        if stored.step != REVIEW_CHECKOUT || stored.setup_done {
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

    /// Turn `target`'s review checkout into a worktree on its git branch, in
    /// place: the path stays, so threads running there keep their cwd.
    pub(crate) async fn promote_review_checkout(
        &self,
        target: CheckoutTarget,
    ) -> Result<PrCheckout, String> {
        let key = self.repo_key();
        let current = self.checkout_state(target.clone()).await?;
        if current.step != CheckoutStep::ReviewCheckout {
            return Err("This pull request has no review checkout to promote".to_string());
        }
        let path = current.path.clone().unwrap_or_default();
        let (moving_path, moving_target) = (path.clone(), target.clone());
        self.blocking("Promoting the review checkout failed", move |desk| {
            desk.ensure_commit(&moving_target)?;
            git::promote_in_place(
                &desk.host,
                desk.dir(),
                &moving_path,
                &moving_target.head_ref,
                &moving_target.head_sha,
            )
        })
        .await?;
        let stored = StoredCheckout {
            pr_number: target.number,
            step: WORKTREE.to_string(),
            path,
            head_sha: None,
            git_branch: Some(target.head_ref.clone()),
            setup_done: true,
            closed_at: None,
        };
        storage::write_checkout(&self.database, &key, &stored).await?;
        self.describe(&target, stored).await
    }

    /// A new named worktree on `target`'s git branch, at `path` or the
    /// repository's default location.
    pub(crate) async fn create_pr_worktree(
        &self,
        target: CheckoutTarget,
        path: Option<String>,
    ) -> Result<PrCheckout, String> {
        let key = self.repo_key();
        let current = self.checkout_state(target.clone()).await?;
        if current.step == CheckoutStep::Worktree {
            return Ok(current);
        }
        let path = match path {
            Some(path) => path,
            None => self.worktree_path(&target).await?,
        };
        let (moving_path, moving_target) = (path.clone(), target.clone());
        self.blocking("Creating the worktree failed", move |desk| {
            desk.ensure_commit(&moving_target)?;
            git::add_branch_worktree(
                &desk.host,
                desk.dir(),
                &moving_path,
                &moving_target.head_ref,
                &moving_target.head_sha,
            )
        })
        .await?;
        let stored = StoredCheckout {
            pr_number: target.number,
            step: WORKTREE.to_string(),
            path,
            head_sha: None,
            git_branch: Some(target.head_ref.clone()),
            setup_done: true,
            closed_at: None,
        };
        storage::write_checkout(&self.database, &key, &stored).await?;
        self.describe(&target, stored).await
    }

    /// Remove PR `number`'s review checkout (Undo, Storage settings). A
    /// stack's checkout stays while another of its PRs still uses it.
    pub(crate) async fn remove_review_checkout(&self, number: i64) -> Result<(), String> {
        let key = self.repo_key();
        let Some(stored) = storage::read_checkout(&self.database, &key, number).await? else {
            return Ok(());
        };
        if stored.step != REVIEW_CHECKOUT {
            return Err("Only a review checkout can be removed here".to_string());
        }
        let shared = storage::read_checkouts(&self.database, &key)
            .await?
            .iter()
            .any(|other| other.pr_number != number && other.path == stored.path);
        if !shared {
            let path = stored.path.clone();
            self.blocking("Removing the review checkout failed", move |desk| {
                git::remove_review_checkout(&desk.host, desk.dir(), &path)
            })
            .await?;
        }
        storage::delete_checkout(&self.database, &key, number).await
    }

    /// What it takes to get `target` to the step `action` needs: the step to
    /// move up to, if any, and what to confirm first.
    pub(crate) async fn plan_step(
        &self,
        target: CheckoutTarget,
        action: DeskAction,
    ) -> Result<StepPlan, String> {
        let current = self.checkout_state(target.clone()).await?;
        let settings = storage::read_repository_settings(&self.database, &self.repo_key()).await?;
        let needed = action.needs();
        let step_up = (needed > current.step).then_some(needed);
        let mut confirm = None;
        let mut refused = None;
        match step_up {
            Some(CheckoutStep::ReviewCheckout) => match settings.review_checkout_policy {
                ReviewCheckoutPolicy::Never => {
                    refused = Some("Review checkouts are turned off for this repository".into());
                }
                ReviewCheckoutPolicy::Ask if !settings.skip_checkout_confirm => {
                    confirm = Some(StepConfirm::CreateReviewCheckout);
                }
                _ => {}
            },
            Some(CheckoutStep::Worktree) => {
                confirm = Some(if current.step == CheckoutStep::ReviewCheckout {
                    StepConfirm::PromoteOrCreateWorktree
                } else {
                    StepConfirm::CreateWorktree
                });
            }
            _ if action == DeskAction::Push && !target.mine => {
                confirm = Some(StepConfirm::PushToTheirBranch);
            }
            _ => {}
        }
        Ok(StepPlan {
            review_checkout_path: self.review_checkout_path(&target)?,
            current,
            step_up,
            confirm,
            refused,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{fixture, Fixture};
    use crate::git::worktrees::worktree_branches;
    use crate::review::checkout_types::{CheckoutTarget, StepConfirm};
    use crate::review::requirements::{CheckoutStep, DeskAction};
    use crate::review::types::InboxPr;
    use crate::storage::{self, ReviewCheckoutPolicy};
    use crate::util::host::Host;

    fn target(pr: &InboxPr, mine: bool) -> CheckoutTarget {
        CheckoutTarget {
            number: pr.number,
            head_ref: pr.head_ref.clone(),
            head_sha: pr.head_sha.clone(),
            mine,
            stack: None,
        }
    }

    fn git_in(fixture: &Fixture, dir: &str, args: &[&str]) -> String {
        fixture.repo.git_in(std::path::Path::new(dir), args)
    }

    /// Whether the worktree listing (what the Worktrees list reads) flags
    /// `path` as a review checkout.
    fn listed_as_review_checkout(fixture: &Fixture, path: &str) -> bool {
        let home = std::path::Path::new("/nowhere");
        crate::git::worktrees::read_worktrees(&Host::Native, &fixture.repo.repo, home)
            .unwrap()
            .into_iter()
            .find(|entry| {
                std::fs::canonicalize(&entry.path).ok() == std::fs::canonicalize(path).ok()
            })
            .expect("listed")
            .is_review_checkout
    }

    async fn set_policy(
        fixture: &Fixture,
        policy: ReviewCheckoutPolicy,
        skip: bool,
        setup: Option<&str>,
    ) {
        let key = fixture.repo.repo.display().to_string();
        let mut settings = storage::read_repository_settings(&fixture.desk.database, &key)
            .await
            .unwrap();
        settings.review_checkout_policy = policy;
        settings.skip_checkout_confirm = skip;
        settings.setup_command = setup.map(str::to_string);
        storage::write_repository_settings(&fixture.desk.database, &settings)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn every_pr_starts_with_no_checkout() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(3, "feature", "src/lib.rs");
        let state = fixture
            .desk
            .checkout_state(target(&pr, false))
            .await
            .unwrap();
        assert_eq!(state.step, CheckoutStep::NoCheckout);
        assert_eq!(state.path, None);
    }

    #[tokio::test]
    async fn a_review_checkout_is_detached_at_the_head_and_kept_out_of_the_worktrees_list() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(9, "feature", "src/lib.rs");

        let state = fixture
            .desk
            .ensure_review_checkout(target(&pr, false))
            .await
            .unwrap();

        assert_eq!(state.step, CheckoutStep::ReviewCheckout);
        let path = state.path.clone().unwrap();
        assert!(path.ends_with("review-checkouts/repo/pr-9"), "{path}");
        assert_eq!(git_in(&fixture, &path, &["rev-parse", "HEAD"]), pr.head_sha);
        assert_eq!(git_in(&fixture, &path, &["branch", "--show-current"]), "");
        assert!(listed_as_review_checkout(&fixture, &path));
        assert_eq!(fixture.repo.git(&["status", "--porcelain"]), "");
        assert_eq!(
            fixture
                .desk
                .checkout_state(target(&pr, false))
                .await
                .unwrap(),
            state,
            "the step is remembered"
        );
    }

    #[tokio::test]
    async fn setup_runs_in_a_new_review_checkout_only_once() {
        let Some(fixture) = fixture().await else {
            return;
        };
        set_policy(
            &fixture,
            ReviewCheckoutPolicy::WhenNeeded,
            false,
            Some("echo ran >> ../setup.log"),
        )
        .await;
        let pr = fixture.open_pr(4, "feature", "src/lib.rs");

        let created = fixture
            .desk
            .ensure_review_checkout(target(&pr, false))
            .await
            .unwrap();
        assert_eq!(
            created.setup_pending.as_deref(),
            Some("echo ran >> ../setup.log")
        );
        fixture.desk.run_checkout_setup(4).await.unwrap();
        fixture.desk.run_checkout_setup(4).await.unwrap();
        let again = fixture
            .desk
            .ensure_review_checkout(target(&pr, false))
            .await
            .unwrap();

        assert_eq!(again.setup_pending, None);
        let path = created.path.unwrap();
        let log = std::path::Path::new(&path)
            .parent()
            .unwrap()
            .join("setup.log");
        assert_eq!(std::fs::read_to_string(log).unwrap(), "ran\n");
    }

    #[tokio::test]
    async fn a_clean_review_checkout_follows_a_new_head_and_a_dirty_one_stays() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(5, "feature", "src/lib.rs");
        let path = fixture
            .desk
            .ensure_review_checkout(target(&pr, false))
            .await
            .unwrap()
            .path
            .unwrap();

        let second = fixture.push_to_pr(5, "feature", "src/two.rs");
        let moved = InboxPr {
            head_sha: second.clone(),
            ..pr.clone()
        };
        let followed = fixture
            .desk
            .ensure_review_checkout(target(&moved, false))
            .await
            .unwrap();
        assert_eq!(followed.head_sha.as_deref(), Some(second.as_str()));
        assert!(!followed.behind_head);

        std::fs::write(std::path::Path::new(&path).join("scratch.txt"), "mine").unwrap();
        let third = fixture.push_to_pr(5, "feature", "src/three.rs");
        let moved_again = InboxPr {
            head_sha: third,
            ..pr.clone()
        };
        let stayed = fixture
            .desk
            .ensure_review_checkout(target(&moved_again, false))
            .await
            .unwrap();
        assert_eq!(stayed.head_sha.as_deref(), Some(second.as_str()));
        assert!(stayed.behind_head);
        assert!(std::path::Path::new(&path).join("scratch.txt").exists());
    }

    #[tokio::test]
    async fn a_stack_shares_one_review_checkout_that_switches_heads() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let first = fixture.open_pr(1, "one", "src/one.rs");
        let second = fixture.open_pr(2, "two", "src/two.rs");
        let in_stack = |pr: &InboxPr| CheckoutTarget {
            stack: Some("Driver Trait".into()),
            ..target(pr, false)
        };

        let a = fixture
            .desk
            .ensure_review_checkout(in_stack(&first))
            .await
            .unwrap();
        let b = fixture
            .desk
            .ensure_review_checkout(in_stack(&second))
            .await
            .unwrap();

        assert_eq!(a.path, b.path);
        assert!(a.path.unwrap().ends_with("stack-driver-trait"));
        assert_eq!(b.head_sha.as_deref(), Some(second.head_sha.as_str()));
    }

    #[tokio::test]
    async fn promotion_keeps_the_path_checks_out_the_git_branch_and_lists_the_worktree() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(6, "feature", "src/lib.rs");
        let review = fixture
            .desk
            .ensure_review_checkout(target(&pr, false))
            .await
            .unwrap();

        let promoted = fixture
            .desk
            .promote_review_checkout(target(&pr, false))
            .await
            .unwrap();

        assert_eq!(promoted.step, CheckoutStep::Worktree);
        assert_eq!(promoted.path, review.path);
        let path = promoted.path.unwrap();
        assert_eq!(
            git_in(&fixture, &path, &["branch", "--show-current"]),
            "feature"
        );
        assert!(!listed_as_review_checkout(&fixture, &path));
    }

    #[tokio::test]
    async fn my_pr_whose_git_branch_is_on_disk_starts_at_the_worktree_step() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(8, "feature", "src/lib.rs");
        let place = fixture.repo.repo.parent().unwrap().join("mine");
        fixture
            .repo
            .git(&["worktree", "add", "-q", place.to_str().unwrap(), "feature"]);

        let mine = fixture
            .desk
            .checkout_state(target(&pr, true))
            .await
            .unwrap();
        let theirs = fixture.desk.checkout_state(target(&pr, false)).await;

        assert_eq!(mine.step, CheckoutStep::Worktree);
        assert_eq!(mine.git_branch.as_deref(), Some("feature"));
        assert_eq!(
            std::fs::canonicalize(mine.path.unwrap()).unwrap(),
            std::fs::canonicalize(&place).unwrap()
        );
        assert_eq!(
            theirs.unwrap().step,
            CheckoutStep::Worktree,
            "the stored step holds for whoever asks"
        );
    }

    #[tokio::test]
    async fn a_worktree_for_a_pr_goes_on_its_git_branch() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(12, "jo-r/focus-rings", "src/lib.rs");
        fixture.repo.git(&["branch", "-D", "jo-r/focus-rings"]);

        let state = fixture
            .desk
            .create_pr_worktree(target(&pr, false), None)
            .await
            .unwrap();

        let path = state.path.unwrap();
        assert!(path.ends_with("worktrees/repo/jo-r-focus-rings"), "{path}");
        assert_eq!(
            git_in(&fixture, &path, &["branch", "--show-current"]),
            "jo-r/focus-rings"
        );
        assert_eq!(git_in(&fixture, &path, &["rev-parse", "HEAD"]), pr.head_sha);
        let listed = worktree_branches(&Host::Native, &fixture.repo.repo).unwrap();
        assert!(listed
            .iter()
            .any(|(_, branch)| branch.as_deref() == Some("jo-r/focus-rings")));
    }

    #[tokio::test]
    async fn undoing_a_review_checkout_removes_it() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(10, "feature", "src/lib.rs");
        let path = fixture
            .desk
            .ensure_review_checkout(target(&pr, false))
            .await
            .unwrap()
            .path
            .unwrap();

        fixture.desk.remove_review_checkout(10).await.unwrap();

        assert!(!std::path::Path::new(&path).exists());
        let state = fixture
            .desk
            .checkout_state(target(&pr, false))
            .await
            .unwrap();
        assert_eq!(state.step, CheckoutStep::NoCheckout);
        assert_eq!(
            worktree_branches(&Host::Native, &fixture.repo.repo)
                .unwrap()
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn the_plan_goes_up_only_as_far_as_the_action_needs_and_asks_per_policy() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(13, "feature", "src/lib.rs");
        let plan = |action| fixture.desk.plan_step(target(&pr, false), action);

        let read = plan(DeskAction::Read).await.unwrap();
        assert_eq!((read.step_up, read.confirm), (None, None));

        let run = plan(DeskAction::RunLocally).await.unwrap();
        assert_eq!(run.step_up, Some(CheckoutStep::ReviewCheckout));
        assert_eq!(run.confirm, Some(StepConfirm::CreateReviewCheckout));
        assert!(run.review_checkout_path.ends_with("pr-13"));

        set_policy(&fixture, ReviewCheckoutPolicy::Ask, true, None).await;
        assert_eq!(plan(DeskAction::RunLocally).await.unwrap().confirm, None);
        set_policy(&fixture, ReviewCheckoutPolicy::WhenNeeded, false, None).await;
        assert_eq!(plan(DeskAction::OpenInEditor).await.unwrap().confirm, None);
        set_policy(&fixture, ReviewCheckoutPolicy::Never, false, None).await;
        assert!(plan(DeskAction::RunLocally)
            .await
            .unwrap()
            .refused
            .is_some());

        let fix = plan(DeskAction::FixCi).await.unwrap();
        assert_eq!(fix.step_up, Some(CheckoutStep::Worktree));
        assert_eq!(fix.confirm, Some(StepConfirm::CreateWorktree));
    }

    #[tokio::test]
    async fn a_pr_with_a_review_checkout_offers_promotion_and_their_branch_asks_on_every_push() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(14, "feature", "src/lib.rs");
        fixture
            .desk
            .ensure_review_checkout(target(&pr, false))
            .await
            .unwrap();

        let fix = fixture
            .desk
            .plan_step(target(&pr, false), DeskAction::AddressReview)
            .await
            .unwrap();
        assert_eq!(fix.confirm, Some(StepConfirm::PromoteOrCreateWorktree));

        fixture
            .desk
            .promote_review_checkout(target(&pr, false))
            .await
            .unwrap();
        for _ in 0..2 {
            let push = fixture
                .desk
                .plan_step(target(&pr, false), DeskAction::Push)
                .await
                .unwrap();
            assert_eq!(push.step_up, None);
            assert_eq!(push.confirm, Some(StepConfirm::PushToTheirBranch));
        }
        let mine = fixture
            .desk
            .plan_step(target(&pr, true), DeskAction::Push)
            .await
            .unwrap();
        assert_eq!(mine.confirm, None);
    }
}
