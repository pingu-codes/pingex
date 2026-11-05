//! The PR link on threads: which PR a thread is about, and what was checked
//! out when it started (step, checkout, git branch, start commit).
//!
//! The PR decides where the sidebar groups a thread; the checkout decides
//! where it runs. "Move to PR" changes only the PR, never the recorded start.

use std::path::Path;

use super::ReviewDesk;
use crate::git::{checkouts as git, objects};
use crate::review::checkout_types::{BranchProgress, CheckoutTarget};
use crate::review::requirements::CheckoutStep;
use crate::storage::{self, PrLink};
use crate::util::time::unix_secs;

impl ReviewDesk {
    /// Record that `thread_id` is about `target`, starting from whatever the
    /// PR has checked out now.
    pub(crate) async fn link_thread(
        &self,
        thread_id: String,
        target: CheckoutTarget,
    ) -> Result<PrLink, String> {
        let state = self.checkout_state(target.clone()).await?;
        let link = PrLink {
            thread_id,
            repo_key: self.repo_key(),
            pr_number: target.number,
            mine: target.mine,
            step: state.step,
            checkout_path: state.path,
            git_branch: state.git_branch.or(Some(target.head_ref)),
            start_sha: state.head_sha.or(Some(target.head_sha)),
            created_at: unix_secs(),
            closed_at: None,
        };
        storage::write_pr_link(&self.database, &link).await?;
        Ok(link)
    }

    /// File `thread_id` under open PR `number` instead, or under no PR.
    /// Whether the new PR is mine comes from GitHub.
    pub(crate) async fn move_thread_to_pr(
        &self,
        thread_id: String,
        number: Option<i64>,
    ) -> Result<(), String> {
        let Some(number) = number else {
            return storage::delete_pr_link(&self.database, &thread_id).await;
        };
        self.link(&thread_id).await?;
        let mine = self
            .blocking("Reading the pull request failed", move |desk| {
                let inbox = desk.github.inbox()?;
                let pr = inbox
                    .prs
                    .iter()
                    .find(|pr| pr.number == number)
                    .ok_or_else(|| format!("#{number} is not an open pull request"))?;
                Ok(pr.author == inbox.viewer)
            })
            .await?;
        storage::move_pr_link(&self.database, &thread_id, number, mine).await
    }

    async fn link(&self, thread_id: &str) -> Result<PrLink, String> {
        storage::read_pr_link(&self.database, thread_id)
            .await?
            .ok_or_else(|| "This thread is not filed under a pull request".to_string())
    }

    /// The tip of a link's git branch: the local branch, else the PR head.
    fn tip(&self, link: &PrLink) -> Result<Option<String>, String> {
        if let Some(branch) = &link.git_branch {
            if let Some(tip) = git::branch_tip(&self.host, self.dir(), branch)? {
                return Ok(Some(tip));
            }
        }
        let number = super::pr_number(link.pr_number)?;
        objects::resolve(&self.host, self.dir(), &objects::head_ref(number))
    }

    /// How far the thread's git branch has moved since its start commit.
    pub(crate) async fn branch_progress(
        &self,
        thread_id: String,
    ) -> Result<BranchProgress, String> {
        let link = self.link(&thread_id).await?;
        self.blocking("Reading the git branch failed", move |desk| {
            let start = link
                .start_sha
                .clone()
                .ok_or_else(|| "This thread has no start commit".to_string())?;
            let tip = desk.tip(&link)?;
            let (ahead, behind) = match &tip {
                Some(tip) => git::ahead_behind(&desk.host, desk.dir(), &start, tip)?,
                None => (0, 0),
            };
            Ok(BranchProgress {
                start_sha: start,
                tip_sha: tip,
                ahead,
                behind,
            })
        })
        .await
    }

    /// Check out the thread's start commit (`start`) or its git branch's tip
    /// in the thread's checkout. Refused when that checkout has changes.
    pub(crate) async fn check_out_thread_commit(
        &self,
        thread_id: String,
        start: bool,
    ) -> Result<(), String> {
        let link = self.link(&thread_id).await?;
        let path = match (&link.step, &link.checkout_path) {
            (CheckoutStep::NoCheckout, _) | (_, None) => {
                return Err("This thread started without a checkout".to_string())
            }
            (_, Some(path)) => path.clone(),
        };
        self.blocking("Checking out failed", move |desk| {
            let dir = Path::new(&path);
            if !git::is_clean(&desk.host, dir)? {
                return Err("The checkout has uncommitted changes".to_string());
            }
            let sha = if start {
                link.start_sha.clone()
            } else {
                desk.tip(&link)?
            }
            .ok_or_else(|| "That commit is not in this repository".to_string())?;
            match (&link.step, &link.git_branch, start) {
                (CheckoutStep::Worktree, Some(branch), false) => {
                    git::promote_in_place(&desk.host, desk.dir(), &path, branch, &sha)
                }
                _ => git::detach_at(&desk.host, desk.dir(), dir, &sha),
            }
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::fixture;
    use crate::review::checkout_types::CheckoutTarget;
    use crate::review::requirements::CheckoutStep;
    use crate::review::types::InboxPr;

    fn target(pr: &InboxPr) -> CheckoutTarget {
        CheckoutTarget {
            number: pr.number,
            head_ref: pr.head_ref.clone(),
            head_sha: pr.head_sha.clone(),
            ..CheckoutTarget::default()
        }
    }

    #[tokio::test]
    async fn a_thread_records_the_pr_and_what_was_checked_out_when_it_started() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(31, "feature", "src/lib.rs");

        let diff = fixture
            .desk
            .link_thread("diff".into(), target(&pr))
            .await
            .unwrap();
        let path = fixture
            .desk
            .ensure_review_checkout(target(&pr))
            .await
            .unwrap()
            .path;
        let tests = fixture
            .desk
            .link_thread("tests".into(), target(&pr))
            .await
            .unwrap();

        assert_eq!(diff.step, CheckoutStep::NoCheckout);
        assert_eq!(diff.checkout_path, None);
        assert_eq!(diff.git_branch.as_deref(), Some("feature"));
        assert_eq!(diff.start_sha.as_deref(), Some(pr.head_sha.as_str()));
        assert_eq!(tests.step, CheckoutStep::ReviewCheckout);
        assert_eq!(tests.checkout_path, path);
        assert_eq!(tests.start_sha.as_deref(), Some(pr.head_sha.as_str()));
        let links = crate::storage::read_pr_links(&fixture.desk.database)
            .await
            .unwrap();
        let ids: Vec<&str> = links.iter().map(|link| link.thread_id.as_str()).collect();
        assert_eq!(ids, ["diff", "tests"]);
    }

    #[tokio::test]
    async fn moving_a_thread_to_another_pr_keeps_its_recorded_start() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(32, "feature", "src/lib.rs");
        let mut my_pr = fixture.open_pr(40, "mine", "src/mine.rs");
        my_pr.author = super::super::test_support::VIEWER.to_string();
        fixture.github.put_pr(my_pr);
        let before = fixture
            .desk
            .link_thread("t".into(), target(&pr))
            .await
            .unwrap();

        fixture
            .desk
            .move_thread_to_pr("t".into(), Some(40))
            .await
            .unwrap();

        let after = crate::storage::read_pr_links(&fixture.desk.database)
            .await
            .unwrap()
            .remove(0);
        assert_eq!(after.pr_number, 40);
        assert!(after.mine);
        assert_eq!(
            (
                after.step,
                after.git_branch,
                after.start_sha,
                after.created_at
            ),
            (
                before.step,
                before.git_branch,
                before.start_sha,
                before.created_at
            )
        );

        assert!(fixture
            .desk
            .move_thread_to_pr("t".into(), Some(77))
            .await
            .is_err());
        fixture
            .desk
            .move_thread_to_pr("t".into(), None)
            .await
            .unwrap();
        assert!(crate::storage::read_pr_links(&fixture.desk.database)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn the_branch_chip_counts_how_far_the_git_branch_moved_and_returns_to_the_start() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(33, "feature", "src/lib.rs");
        let path = fixture
            .desk
            .create_pr_worktree(target(&pr), None)
            .await
            .unwrap()
            .path
            .unwrap();
        fixture
            .desk
            .link_thread("t".into(), target(&pr))
            .await
            .unwrap();
        let dir = std::path::Path::new(&path);
        std::fs::write(dir.join("more.rs"), "more").unwrap();
        fixture.repo.git_in(dir, &["add", "more.rs"]);
        fixture.repo.git_in(dir, &["commit", "-q", "-m", "more"]);
        let tip = fixture.repo.git_in(dir, &["rev-parse", "HEAD"]);

        let progress = fixture.desk.branch_progress("t".into()).await.unwrap();
        assert_eq!(progress.start_sha, pr.head_sha);
        assert_eq!(progress.tip_sha.as_deref(), Some(tip.as_str()));
        assert_eq!((progress.ahead, progress.behind), (1, 0));

        fixture
            .desk
            .check_out_thread_commit("t".into(), true)
            .await
            .unwrap();
        assert_eq!(
            fixture.repo.git_in(dir, &["rev-parse", "HEAD"]),
            pr.head_sha
        );
        fixture
            .desk
            .check_out_thread_commit("t".into(), false)
            .await
            .unwrap();
        assert_eq!(
            fixture.repo.git_in(dir, &["branch", "--show-current"]),
            "feature"
        );
        assert_eq!(fixture.repo.git_in(dir, &["rev-parse", "HEAD"]), tip);
    }
}
