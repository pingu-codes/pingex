//! Getting a PR into a worktree: where it goes, which branch it is on, and
//! keeping it level with the PR without ever discarding work.
//!
//! A PR's worktree is stored per (repository, PR) in `review_checkouts`; No
//! checkout is the absence of a row. Two facts on disk override the store: my
//! own PR whose git branch is already checked out somewhere is at the Worktree
//! step, and a stored worktree whose directory is gone is not one.
//!
//! Who owns the PR decides the branch and who looks after the worktree, never
//! what an agent may do there:
//!
//! - My same-repository PR, or a stack's PR, goes on its real git branch in the
//!   repository's worktree directory. It is the user's, and a plain `git push`
//!   updates the PR.
//! - Anyone else's PR, and any fork, goes on `pr/<n>` with no upstream under
//!   `<home>/pr-worktrees/<repository>/`. Pingex manages it: `git push` has
//!   nowhere to go, `push_to_pr` is the way back, and cleanup removes it once
//!   the PR has closed and it holds no work.
//!
//! The repository's setup command runs once per worktree Pingex creates.
//! `plan_step` applies the repository's policy and says what to confirm;
//! callers then call the operation it names.

use std::path::Path;

use super::ReviewDesk;
use crate::git::{checkouts as git, objects, worktrees};
use crate::review::checkout_types::{CheckoutTarget, PrCheckout, StepConfirm, StepPlan};
use crate::review::requirements::{CheckoutStep, DeskAction};
use crate::storage::{self, ReviewCheckoutPolicy, StoredCheckout};
use crate::util::process::{run, Run};

const WORKTREE: &str = "worktree";
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

/// Which local branch a PR's new worktree goes on, and who looks after it.
struct Placement {
    branch: String,
    managed: bool,
}

/// What removing a managed worktree did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Removal {
    Removed,
    /// It holds work nothing else has, so it stays.
    KeptLocalWork,
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
    fn same_repo(&self, target: &CheckoutTarget) -> Result<bool, String> {
        let Some(head_repo) = &target.head_repo else {
            return Ok(false);
        };
        Ok(head_repo.eq_ignore_ascii_case(&self.github.repo()?))
    }

    fn placement(&self, target: &CheckoutTarget) -> Result<Placement, String> {
        Ok(
            if target.stack.is_some() || (target.mine && self.same_repo(target)?) {
                Placement {
                    branch: target.head_ref.clone(),
                    managed: false,
                }
            } else {
                Placement {
                    branch: git::managed_branch(target.number),
                    managed: true,
                }
            },
        )
    }

    /// Where a new worktree for `target` goes: the repository's worktree
    /// directory or `<home>/worktrees/<repository>/` when it is the user's,
    /// `<home>/pr-worktrees/<repository>/pr-<n>` when Pingex manages it.
    async fn new_worktree_path(&self, target: &CheckoutTarget) -> Result<String, String> {
        let moving = target.clone();
        let placement = self
            .blocking("Reading the repository failed", move |desk| {
                desk.placement(&moving)
            })
            .await?;
        if placement.managed {
            let root = self.host.join_str(self.home()?, "pr-worktrees");
            let group = self.host.join_str(&root, &self.repo_name());
            return Ok(self.host.join_str(&group, &format!("pr-{}", target.number)));
        }
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
                let stored = self.adopt_legacy(stored).await?;
                let stored = self.remember_head(stored, &target).await?;
                return self.describe(&target, stored).await;
            }
            storage::delete_checkout(&self.database, &key, target.number).await?;
        }
        if !target.mine {
            return Ok(PrCheckout::none(target.number));
        }
        // My PR whose git branch already lives on my disk starts at Worktree.
        // A fork's branch name says nothing about my local branch of that name.
        let branch = target.head_ref.clone();
        let moving = target.clone();
        let found = self
            .blocking("Reading worktrees failed", move |desk| {
                if !desk.same_repo(&moving).unwrap_or(false) {
                    return Ok(None);
                }
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
            head_sha: Some(target.head_sha.clone()),
            git_branch: Some(target.head_ref.clone()),
            setup_done: true,
            closed_at: None,
        };
        storage::write_checkout(&self.database, &key, &stored).await?;
        self.describe(&target, stored).await
    }

    /// `stored` as a managed worktree, if it is a review checkout from before
    /// they became worktrees. A detached one gets a branch at its HEAD, so its
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
            step: WORKTREE.to_string(),
            git_branch: Some(branch),
            ..stored
        })
    }

    /// `stored`, having recorded the head the PR is at now.
    async fn remember_head(
        &self,
        stored: StoredCheckout,
        target: &CheckoutTarget,
    ) -> Result<StoredCheckout, String> {
        if target.head_sha.is_empty()
            || stored.head_sha.as_deref() == Some(target.head_sha.as_str())
        {
            return Ok(stored);
        }
        storage::update_checkout_head(
            &self.database,
            &self.repo_key(),
            target.number,
            &target.head_sha,
        )
        .await?;
        Ok(StoredCheckout {
            head_sha: Some(target.head_sha.clone()),
            ..stored
        })
    }

    /// A stored worktree as the caller sees it, with what is on disk now.
    async fn describe(
        &self,
        target: &CheckoutTarget,
        stored: StoredCheckout,
    ) -> Result<PrCheckout, String> {
        let path = stored.path.clone();
        let sha = target.head_sha.clone();
        let (head, branch, dirty, managed, (ahead, behind)) = self
            .blocking("Reading the checkout failed", move |desk| {
                let dir = Path::new(&path);
                let head = git::head_of(&desk.host, dir)?;
                // A PR head not fetched yet can't be counted against.
                let counts = match &head {
                    Some(head) if git::has_commit(&desk.host, dir, &sha)? => {
                        git::ahead_behind(&desk.host, dir, &sha, head).unwrap_or((0, 0))
                    }
                    _ => (0, 0),
                };
                Ok((
                    head,
                    git::branch_of(&desk.host, dir)?,
                    !git::is_clean(&desk.host, dir).unwrap_or(true),
                    git::is_managed(&desk.host, dir).unwrap_or(false),
                    counts,
                ))
            })
            .await?;
        let setup_pending = if stored.setup_done {
            None
        } else {
            storage::read_repository_settings(&self.database, &self.repo_key())
                .await?
                .setup_command
        };
        Ok(PrCheckout {
            number: target.number,
            step: CheckoutStep::Worktree,
            path: Some(stored.path),
            head_sha: head,
            git_branch: branch.or(stored.git_branch),
            setup_pending,
            ahead,
            behind,
            dirty,
            managed,
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

    /// Bring the worktree at `path` level with the PR by fast-forwarding, when
    /// nothing can be lost by it: a linked worktree, on a branch, with no local
    /// changes, whose HEAD the new head contains. Otherwise it stays where it
    /// is, and `PrCheckout` reports how it differs.
    fn follow_head(&self, path: &str, target: &CheckoutTarget) -> Result<(), String> {
        let dir = Path::new(path);
        let Some(head) = git::head_of(&self.host, dir)? else {
            return Ok(());
        };
        if head == target.head_sha
            || !git::is_linked_worktree(&self.host, dir)?
            || git::branch_of(&self.host, dir)?.is_none()
            || !git::is_clean(&self.host, dir)?
        {
            return Ok(());
        }
        self.ensure_commit(target)?;
        if objects::is_ancestor(&self.host, dir, &head, &target.head_sha)? {
            git::fast_forward(&self.host, self.dir(), dir, &target.head_sha)?;
        }
        Ok(())
    }

    /// `target`'s checkout, brought level with the PR where that is safe.
    /// Best effort: a PR that can't be fetched just reports where it is.
    pub(crate) async fn synced_checkout(
        &self,
        target: CheckoutTarget,
    ) -> Result<PrCheckout, String> {
        let current = self.checkout_state(target.clone()).await?;
        let Some(path) = current.path else {
            return Ok(current);
        };
        let moving = target.clone();
        let _ = self
            .blocking("Updating the worktree failed", move |desk| {
                desk.follow_head(&path, &moving)
            })
            .await;
        self.checkout_state(target).await
    }

    /// Give `target` a worktree at its head, unless it already has one, which
    /// then follows the head where it safely can.
    pub(crate) async fn ensure_pr_worktree(
        &self,
        target: CheckoutTarget,
    ) -> Result<PrCheckout, String> {
        let key = self.repo_key();
        if self.checkout_state(target.clone()).await?.path.is_some() {
            return self.synced_checkout(target).await;
        }
        let path = self.new_worktree_path(&target).await?;
        let (moving_path, moving_target) = (path.clone(), target.clone());
        let branch = self
            .blocking("Creating the worktree failed", move |desk| {
                let placement = desk.placement(&moving_target)?;
                desk.ensure_commit(&moving_target)?;
                git::add_pr_worktree(
                    &desk.host,
                    desk.dir(),
                    &moving_path,
                    &placement.branch,
                    &moving_target.head_sha,
                    placement.managed,
                )?;
                if !placement.managed {
                    let slug = desk.github.repo()?;
                    if let Some(remote) = objects::remote_for_slug(&desk.host, desk.dir(), &slug)? {
                        git::track_remote_branch(
                            &desk.host,
                            desk.dir(),
                            &placement.branch,
                            &remote,
                        )?;
                    }
                }
                Ok(placement.branch)
            })
            .await?;
        let setup_command = storage::read_repository_settings(&self.database, &key)
            .await?
            .setup_command;
        let stored = StoredCheckout {
            pr_number: target.number,
            step: WORKTREE.to_string(),
            path,
            head_sha: Some(target.head_sha.clone()),
            git_branch: Some(branch),
            setup_done: setup_command.is_none(),
            closed_at: None,
        };
        storage::write_checkout(&self.database, &key, &stored).await?;
        self.describe(&target, stored).await
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

    /// Whether Pingex manages the worktree at `path`.
    pub(crate) async fn is_managed_worktree(&self, path: String) -> Result<bool, String> {
        self.blocking("Reading the worktree failed", move |desk| {
            git::is_managed(&desk.host, Path::new(&path))
        })
        .await
    }

    /// Remove PR `number`'s managed worktree and its `pr/<n>` branch if it
    /// holds no work; otherwise it stays. A worktree that is the user's only
    /// loses its record. Another PR still using the directory keeps it too.
    pub(super) async fn try_remove_pr_worktree(&self, number: i64) -> Result<Removal, String> {
        let key = self.repo_key();
        let Some(stored) = storage::read_checkout(&self.database, &key, number).await? else {
            return Ok(Removal::Removed);
        };
        let stored = if self.host.is_dir(&stored.path) {
            self.adopt_legacy(stored).await?
        } else {
            stored
        };
        let shared = storage::read_checkouts(&self.database, &key)
            .await?
            .iter()
            .any(|other| other.pr_number != number && other.path == stored.path);
        if !shared {
            let (path, branch, head) = (
                stored.path.clone(),
                stored.git_branch.clone(),
                stored.head_sha.clone(),
            );
            let removal = self
                .blocking("Removing the worktree failed", move |desk| {
                    let dir = Path::new(&path);
                    if desk.host.is_dir(&path) {
                        if !git::is_managed(&desk.host, dir)? {
                            return Ok(Removal::Removed);
                        }
                        if git::has_local_work(&desk.host, dir, branch.as_deref(), head.as_deref())
                        {
                            return Ok(Removal::KeptLocalWork);
                        }
                    }
                    let ours = branch
                        .as_deref()
                        .filter(|branch| is_managed_branch(branch, number));
                    match git::remove_pr_worktree(&desk.host, desk.dir(), &path, ours) {
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
        storage::delete_checkout(&self.database, &key, number).await?;
        Ok(Removal::Removed)
    }

    /// Undo creating PR `number`'s worktree. Refused when it holds work.
    pub(crate) async fn remove_pr_worktree(&self, number: i64) -> Result<(), String> {
        match self.try_remove_pr_worktree(number).await? {
            Removal::Removed => Ok(()),
            Removal::KeptLocalWork => {
                Err("This worktree has local changes or unpushed commits, so it was kept".into())
            }
        }
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
            Some(CheckoutStep::Worktree) if action.follows_policy() => {
                match settings.review_checkout_policy {
                    ReviewCheckoutPolicy::Never => {
                        refused = Some("PR worktrees are turned off for this repository".into());
                    }
                    ReviewCheckoutPolicy::Ask if !settings.skip_checkout_confirm => {
                        confirm = Some(StepConfirm::CreateWorktree);
                    }
                    _ => {}
                }
            }
            Some(CheckoutStep::Worktree) => confirm = Some(StepConfirm::CreateWorktree),
            _ if action == DeskAction::Push && !target.mine => {
                confirm = Some(StepConfirm::PushToTheirBranch);
            }
            _ => {}
        }
        // Asked every time, after any step up's own confirmation.
        let confirm_push = action == DeskAction::Push && !target.mine;
        let worktree_path = match &current.path {
            Some(path) => path.clone(),
            None => self.new_worktree_path(&target).await?,
        };
        Ok(StepPlan {
            worktree_path,
            current,
            step_up,
            confirm,
            confirm_push,
            refused,
        })
    }
}

/// Whether `branch` is one Pingex made for PR `number`: `pr/<n>`, or the
/// fallback adoption uses when that name was taken.
fn is_managed_branch(branch: &str, number: i64) -> bool {
    let name = git::managed_branch(number);
    branch == name || branch == format!("{name}-local")
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{fixture, Fixture, SLUG};
    use super::Removal;
    use crate::git::checkouts::PR_WORKTREE_LOCK;
    use crate::git::worktrees::worktree_branches;
    use crate::review::checkout_types::{CheckoutTarget, StepConfirm};
    use crate::review::requirements::{CheckoutStep, DeskAction};
    use crate::review::types::InboxPr;
    use crate::storage::{self, ReviewCheckoutPolicy, StoredCheckout};
    use crate::util::host::Host;

    fn target(pr: &InboxPr, mine: bool) -> CheckoutTarget {
        CheckoutTarget {
            number: pr.number,
            head_ref: pr.head_ref.clone(),
            head_sha: pr.head_sha.clone(),
            mine,
            stack: None,
            head_repo: Some(SLUG.to_string()),
        }
    }

    fn git_in(fixture: &Fixture, dir: &str, args: &[&str]) -> String {
        fixture.repo.git_in(std::path::Path::new(dir), args)
    }

    /// Commit a new file in the worktree at `dir`, as an agent would.
    fn commit_in(fixture: &Fixture, dir: &str, file: &str) -> String {
        std::fs::write(std::path::Path::new(dir).join(file), "work").unwrap();
        git_in(fixture, dir, &["add", file]);
        git_in(fixture, dir, &["commit", "-q", "-m", file]);
        git_in(fixture, dir, &["rev-parse", "HEAD"])
    }

    /// Whether the worktree listing (what the Worktrees list reads) flags
    /// `path` as a managed PR worktree.
    fn listed_as_managed(fixture: &Fixture, path: &str) -> bool {
        let home = std::path::Path::new("/nowhere");
        crate::git::worktrees::read_worktrees(&Host::Native, &fixture.repo.repo, home)
            .unwrap()
            .into_iter()
            .find(|entry| {
                std::fs::canonicalize(&entry.path).ok() == std::fs::canonicalize(path).ok()
            })
            .expect("listed")
            .is_managed_pr_worktree
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
    async fn someone_elses_pr_gets_a_managed_worktree_on_its_own_branch() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(9, "feature", "src/lib.rs");

        let state = fixture
            .desk
            .ensure_pr_worktree(target(&pr, false))
            .await
            .unwrap();

        assert_eq!(state.step, CheckoutStep::Worktree);
        assert!(state.managed);
        assert_eq!(state.git_branch.as_deref(), Some("pr/9"));
        let path = state.path.clone().unwrap();
        assert!(path.ends_with("pr-worktrees/repo/pr-9"), "{path}");
        assert_eq!(git_in(&fixture, &path, &["rev-parse", "HEAD"]), pr.head_sha);
        assert_eq!(
            git_in(&fixture, &path, &["branch", "--show-current"]),
            "pr/9"
        );
        assert!(listed_as_managed(&fixture, &path));
        assert_eq!(fixture.repo.git(&["status", "--porcelain"]), "");
        assert_eq!(
            fixture
                .repo
                .git(&["config", "--default", "none", "--get", "branch.pr/9.remote"]),
            "none",
            "a plain git push has nowhere to go"
        );
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
    async fn my_pr_gets_a_worktree_on_its_git_branch_that_a_plain_push_updates() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(12, "jo-r/focus-rings", "src/lib.rs");
        fixture.repo.git(&["branch", "-D", "jo-r/focus-rings"]);

        let state = fixture
            .desk
            .ensure_pr_worktree(target(&pr, true))
            .await
            .unwrap();

        assert!(!state.managed);
        let path = state.path.unwrap();
        assert!(path.ends_with("worktrees/repo/jo-r-focus-rings"), "{path}");
        assert_eq!(
            git_in(&fixture, &path, &["branch", "--show-current"]),
            "jo-r/focus-rings"
        );
        assert_eq!(git_in(&fixture, &path, &["rev-parse", "HEAD"]), pr.head_sha);
        assert!(!listed_as_managed(&fixture, &path));
        let listed = worktree_branches(&Host::Native, &fixture.repo.repo).unwrap();
        assert!(listed
            .iter()
            .any(|(_, branch)| branch.as_deref() == Some("jo-r/focus-rings")));

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
    async fn a_stack_stays_on_real_git_branches_whoever_wrote_it() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(2, "two", "src/two.rs");

        let state = fixture
            .desk
            .ensure_pr_worktree(CheckoutTarget {
                stack: Some("Driver Trait".into()),
                head_repo: None,
                ..target(&pr, false)
            })
            .await
            .unwrap();

        assert!(!state.managed);
        assert_eq!(state.git_branch.as_deref(), Some("two"));
    }

    #[tokio::test]
    async fn my_fork_pr_from_main_does_not_land_on_my_main() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(16, "patch", "src/lib.rs");
        let fork = CheckoutTarget {
            head_ref: "main".into(),
            head_repo: Some("jo-r/desk".into()),
            ..target(&pr, true)
        };
        let local_main = fixture.repo.git(&["rev-parse", "main"]);

        let before = fixture.desk.checkout_state(fork.clone()).await.unwrap();
        assert_eq!(before.step, CheckoutStep::NoCheckout, "main is not the PR");

        let state = fixture.desk.ensure_pr_worktree(fork).await.unwrap();
        assert_eq!(state.git_branch.as_deref(), Some("pr/16"));
        assert!(state.managed);
        assert_eq!(fixture.repo.git(&["rev-parse", "main"]), local_main);
    }

    #[tokio::test]
    async fn setup_runs_in_a_new_worktree_only_once_whoever_owns_the_pr() {
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
        for (number, branch, mine) in [(4, "theirs", false), (5, "ours", true)] {
            let pr = fixture.open_pr(number, branch, "src/lib.rs");

            let created = fixture
                .desk
                .ensure_pr_worktree(target(&pr, mine))
                .await
                .unwrap();
            assert_eq!(
                created.setup_pending.as_deref(),
                Some("echo ran >> ../setup.log")
            );
            let number = i64::from(number);
            fixture.desk.run_checkout_setup(number).await.unwrap();
            fixture.desk.run_checkout_setup(number).await.unwrap();
            let again = fixture
                .desk
                .ensure_pr_worktree(target(&pr, mine))
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
    }

    #[tokio::test]
    async fn a_clean_worktree_fast_forwards_to_a_new_head_and_a_dirty_one_stays() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(5, "feature", "src/lib.rs");
        let path = fixture
            .desk
            .ensure_pr_worktree(target(&pr, false))
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
            .ensure_pr_worktree(target(&moved, false))
            .await
            .unwrap();
        assert_eq!(followed.head_sha.as_deref(), Some(second.as_str()));
        assert_eq!((followed.ahead, followed.behind), (0, 0));

        std::fs::write(std::path::Path::new(&path).join("scratch.txt"), "mine").unwrap();
        let third = fixture.push_to_pr(5, "feature", "src/three.rs");
        let moved_again = InboxPr {
            head_sha: third,
            ..pr.clone()
        };
        let stayed = fixture
            .desk
            .ensure_pr_worktree(target(&moved_again, false))
            .await
            .unwrap();
        assert_eq!(stayed.head_sha.as_deref(), Some(second.as_str()));
        assert_eq!(stayed.behind, 1);
        assert!(stayed.dirty);
        assert!(std::path::Path::new(&path).join("scratch.txt").exists());
    }

    #[tokio::test]
    async fn a_commit_in_a_worktree_survives_the_pr_moving_on() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(6, "feature", "src/lib.rs");
        let path = fixture
            .desk
            .ensure_pr_worktree(target(&pr, false))
            .await
            .unwrap()
            .path
            .unwrap();
        let mine = commit_in(&fixture, &path, "measure.txt");

        let second = fixture.push_to_pr(6, "feature", "src/two.rs");
        let moved = InboxPr {
            head_sha: second,
            ..pr.clone()
        };
        let state = fixture
            .desk
            .synced_checkout(target(&moved, false))
            .await
            .unwrap();

        assert_eq!(state.head_sha.as_deref(), Some(mine.as_str()));
        assert_eq!((state.ahead, state.behind), (1, 1));
        assert_eq!(git_in(&fixture, &path, &["rev-parse", "pr/6"]), mine);
    }

    #[tokio::test]
    async fn a_stale_local_branch_fast_forwards_and_a_diverged_one_is_not_moved() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(17, "stale", "src/lib.rs");
        let new_head = fixture.push_to_pr(17, "stale", "src/two.rs");
        fixture.repo.git(&["branch", "-f", "stale", &pr.head_sha]);
        let stale = fixture
            .desk
            .ensure_pr_worktree(CheckoutTarget {
                head_sha: new_head.clone(),
                ..target(&pr, true)
            })
            .await
            .unwrap();
        assert_eq!(stale.head_sha.as_deref(), Some(new_head.as_str()));

        let other = fixture.open_pr(18, "diverged", "src/other.rs");
        let remote_head = fixture.push_to_pr(18, "diverged", "src/three.rs");
        fixture
            .repo
            .git(&["branch", "-f", "diverged", &other.head_sha]);
        fixture.repo.switch("diverged");
        let local = fixture.repo.commit("src/local.rs", "local", "local");
        fixture.repo.switch("main");

        let state = fixture
            .desk
            .ensure_pr_worktree(CheckoutTarget {
                head_sha: remote_head,
                ..target(&other, true)
            })
            .await
            .unwrap();

        assert_eq!(state.head_sha.as_deref(), Some(local.as_str()));
        assert_eq!((state.ahead, state.behind), (1, 1));
        assert_eq!(fixture.repo.git(&["rev-parse", "diverged"]), local);
    }

    #[tokio::test]
    async fn a_managed_worktree_whose_directory_was_deleted_can_be_made_again() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(19, "feature", "src/lib.rs");
        let path = fixture
            .desk
            .ensure_pr_worktree(target(&pr, false))
            .await
            .unwrap()
            .path
            .unwrap();
        std::fs::remove_dir_all(&path).unwrap();

        let again = fixture
            .desk
            .ensure_pr_worktree(target(&pr, false))
            .await
            .unwrap();

        assert_eq!(again.path.as_deref(), Some(path.as_str()));
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
                pr_number: 20,
                step: "review_checkout".into(),
                path: path.clone(),
                head_sha: Some(pr.head_sha.clone()),
                git_branch: None,
                setup_done: true,
                closed_at: None,
            },
        )
        .await
        .unwrap();
        let committed = commit_in(&fixture, &path, "measure.txt");
        std::fs::write(place.join("scratch.txt"), "output").unwrap();

        let state = fixture
            .desk
            .checkout_state(target(&pr, false))
            .await
            .unwrap();

        assert_eq!(state.step, CheckoutStep::Worktree);
        assert_eq!(state.path.as_deref(), Some(path.as_str()));
        assert!(state.managed);
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
        assert!(!mine.managed);
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
    async fn the_main_clone_is_never_moved_when_my_pr_is_checked_out_there() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(21, "feature", "src/lib.rs");
        let second = fixture.push_to_pr(21, "feature", "src/two.rs");
        fixture.repo.git(&["branch", "-f", "feature", &pr.head_sha]);
        fixture.repo.switch("feature");

        let state = fixture
            .desk
            .synced_checkout(CheckoutTarget {
                head_sha: second,
                ..target(&pr, true)
            })
            .await
            .unwrap();

        assert_eq!(state.head_sha.as_deref(), Some(pr.head_sha.as_str()));
        assert_eq!(state.behind, 1);
    }

    #[tokio::test]
    async fn undoing_a_worktree_removes_it_and_its_branch() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(10, "feature", "src/lib.rs");
        let path = fixture
            .desk
            .ensure_pr_worktree(target(&pr, false))
            .await
            .unwrap()
            .path
            .unwrap();

        fixture.desk.remove_pr_worktree(10).await.unwrap();

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
        assert_eq!(fixture.repo.git(&["branch", "--list", "pr/10"]), "");
    }

    #[tokio::test]
    async fn undo_refuses_a_worktree_that_holds_work() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(11, "feature", "src/lib.rs");
        let path = fixture
            .desk
            .ensure_pr_worktree(target(&pr, false))
            .await
            .unwrap()
            .path
            .unwrap();
        let committed = commit_in(&fixture, &path, "measure.txt");

        assert_eq!(
            fixture.desk.try_remove_pr_worktree(11).await.unwrap(),
            Removal::KeptLocalWork
        );
        assert!(fixture.desk.remove_pr_worktree(11).await.is_err());

        assert_eq!(git_in(&fixture, &path, &["rev-parse", "HEAD"]), committed);
        assert_eq!(
            fixture
                .desk
                .checkout_state(target(&pr, false))
                .await
                .unwrap()
                .step,
            CheckoutStep::Worktree
        );
        assert!(listed_as_managed(&fixture, &path), "still locked");
    }

    #[tokio::test]
    async fn gitignored_build_output_does_not_keep_a_worktree_alive() {
        let Some(fixture) = fixture().await else {
            return;
        };
        fixture.repo.commit(".gitignore", "target/\n", "ignore");
        let pr = fixture.open_pr(22, "feature", "src/lib.rs");
        let path = fixture
            .desk
            .ensure_pr_worktree(target(&pr, false))
            .await
            .unwrap()
            .path
            .unwrap();
        let build = std::path::Path::new(&path).join("target");
        std::fs::create_dir_all(&build).unwrap();
        std::fs::write(build.join("output.bin"), "built").unwrap();

        fixture.desk.remove_pr_worktree(22).await.unwrap();

        assert!(!std::path::Path::new(&path).exists());
    }

    #[tokio::test]
    async fn the_plan_goes_up_when_the_action_needs_it_and_asks_per_policy() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(13, "feature", "src/lib.rs");
        let plan = |action| fixture.desk.plan_step(target(&pr, false), action);

        let read = plan(DeskAction::Read).await.unwrap();
        assert_eq!((read.step_up, read.confirm), (None, None));

        let run = plan(DeskAction::RunLocally).await.unwrap();
        assert_eq!(run.step_up, Some(CheckoutStep::Worktree));
        assert_eq!(run.confirm, Some(StepConfirm::CreateWorktree));
        assert!(run.worktree_path.ends_with("pr-13"));

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

        for explicit in [DeskAction::FixCi, DeskAction::WorkOnPr] {
            let plan = plan(explicit).await.unwrap();
            assert_eq!(plan.step_up, Some(CheckoutStep::Worktree));
            assert_eq!(plan.confirm, Some(StepConfirm::CreateWorktree));
            assert_eq!(plan.refused, None);
        }
    }

    #[tokio::test]
    async fn pushing_to_their_branch_asks_every_time_even_when_a_worktree_comes_first() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(15, "feature", "src/lib.rs");

        let theirs = fixture
            .desk
            .plan_step(target(&pr, false), DeskAction::Push)
            .await
            .unwrap();
        assert_eq!(theirs.step_up, Some(CheckoutStep::Worktree));
        assert_eq!(theirs.confirm, Some(StepConfirm::CreateWorktree));
        assert!(theirs.confirm_push);

        let mine = fixture
            .desk
            .plan_step(target(&pr, true), DeskAction::Push)
            .await
            .unwrap();
        assert!(!mine.confirm_push);
        let read = fixture
            .desk
            .plan_step(target(&pr, false), DeskAction::FixCi)
            .await
            .unwrap();
        assert!(!read.confirm_push);

        fixture
            .desk
            .ensure_pr_worktree(target(&pr, false))
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
            assert!(push.confirm_push);
        }
        let mine = fixture
            .desk
            .plan_step(target(&pr, true), DeskAction::Push)
            .await
            .unwrap();
        assert_eq!(mine.confirm, None);
    }
}
