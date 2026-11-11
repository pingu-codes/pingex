//! Cleanup: what a closed PR leaves behind. Its refs go when it is first
//! seen closed, its managed worktree 7 days later unless it holds work, and
//! threads linked to it leave the sidebar's Reviews group.

use std::collections::HashSet;

use super::steps::Removal;
use super::{pr_number, ReviewDesk};
use crate::git::{checkouts as git, objects};
use crate::review::checkout_types::PruneReport;
use crate::storage;
use crate::util::time::unix_secs;

/// How long a closed PR's managed worktree is kept.
const KEEP_CLOSED_CHECKOUT_SECS: i64 = 7 * 24 * 60 * 60;

impl ReviewDesk {
    /// Compare what Pingex keeps for this repository with `open`, the PRs a
    /// complete inbox sync just listed, and drop what closed PRs left behind.
    /// The caller passes the sync's own answer: fetching the inbox again here
    /// doubled every sync's GitHub reads (seconds per page on a busy repo).
    pub(crate) async fn prune_closed(&self, open: HashSet<i64>) -> Result<PruneReport, String> {
        self.prune_closed_with(open, unix_secs()).await
    }

    /// `prune_closed` against a fresh inbox read, for tests that close PRs
    /// on the fake GitHub.
    #[cfg(test)]
    pub(super) async fn prune_closed_at(&self, now: i64) -> Result<PruneReport, String> {
        let inbox = self
            .blocking("Cleaning up failed", |desk| desk.github.inbox())
            .await?;
        // A partial read cannot say which PRs closed.
        if !inbox.complete {
            return Ok(PruneReport::default());
        }
        let open = inbox.prs.iter().map(|pr| pr.number).collect();
        self.prune_closed_with(open, now).await
    }

    pub(super) async fn prune_closed_with(
        &self,
        open: HashSet<i64>,
        now: i64,
    ) -> Result<PruneReport, String> {
        let kept = self
            .blocking("Cleaning up failed", |desk| {
                git::kept_pr_numbers(&desk.host, desk.dir())
            })
            .await?;
        let key = self.repo_key();
        let mut report = PruneReport::default();

        let closed_refs: Vec<u32> = kept
            .into_iter()
            .filter(|number| !open.contains(&i64::from(*number)))
            .collect();
        if !closed_refs.is_empty() {
            self.blocking("Cleaning up failed", move |desk| {
                closed_refs
                    .into_iter()
                    .try_for_each(|number| objects::prune_pr_refs(&desk.host, desk.dir(), number))
            })
            .await?;
        }

        for checkout in storage::read_checkouts(&self.database, &key).await? {
            if open.contains(&checkout.pr_number) {
                if checkout.closed_at.is_some() {
                    // Reopened: its seven days start again if it closes again.
                    storage::mark_checkout_open(&self.database, &key, checkout.pr_number).await?;
                }
                continue;
            }
            match checkout.closed_at {
                None => {
                    storage::mark_checkout_closed(&self.database, &key, checkout.pr_number, now)
                        .await?;
                    report.closed.push(checkout.pr_number);
                }
                Some(closed) if now - closed >= KEEP_CLOSED_CHECKOUT_SECS => {
                    // Whether it can go is only proven by looking, so any
                    // doubt, or any failure, leaves it for the user to see in
                    // the Worktrees list. A user's worktree loses only its row.
                    let managed = self.host.is_dir(&checkout.path)
                        && self
                            .is_managed_worktree(checkout.path.clone())
                            .await
                            .unwrap_or(false);
                    let removal = self.try_remove_pr_worktree(checkout.pr_number).await;
                    if managed && matches!(removal, Ok(Removal::Removed)) {
                        report.removed_checkouts.push(checkout.pr_number);
                    }
                }
                Some(_) => {}
            }
        }

        storage::prune_stack_worktrees(&self.database, &key, |number| open.contains(&number))
            .await?;

        for link in storage::read_pr_links(&self.database).await? {
            if link.repo_key == key && link.closed_at.is_none() && !open.contains(&link.pr_number) {
                storage::mark_pr_links_closed(&self.database, &key, link.pr_number, now).await?;
            }
        }
        Ok(report)
    }

    /// Drop every ref kept for PR `number` (its head and Seen head).
    pub(crate) async fn forget_pr(&self, number: i64) -> Result<(), String> {
        self.blocking("Cleaning up failed", move |desk| {
            objects::prune_pr_refs(&desk.host, desk.dir(), pr_number(number)?)
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::fixture;
    use super::KEEP_CLOSED_CHECKOUT_SECS;
    use crate::git::objects;
    use crate::review::checkout_types::CheckoutTarget;
    use crate::review::requirements::CheckoutStep;

    #[tokio::test]
    async fn a_closed_prs_refs_go_at_once_and_its_managed_worktree_after_seven_days() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(21, "feature", "src/lib.rs");
        let still_open = fixture.open_pr(22, "other", "src/other.rs");
        fixture.desk.fetch_prs(vec![21, 22]).await.unwrap();
        let target = CheckoutTarget {
            number: 21,
            head_ref: pr.head_ref.clone(),
            head_sha: pr.head_sha.clone(),
            ..CheckoutTarget::default()
        };
        let path = fixture
            .desk
            .ensure_pr_worktree(target.clone())
            .await
            .unwrap()
            .path
            .unwrap();
        let link = fixture
            .desk
            .link_thread("thread-1".into(), target.clone())
            .await
            .unwrap();
        assert_eq!(link.closed_at, None);
        fixture.github.close_pr(21);
        let closed_at = 1_000_000;

        let first = fixture.desk.prune_closed_at(closed_at).await.unwrap();
        assert_eq!(first.closed, [21]);
        assert!(first.removed_checkouts.is_empty());
        assert_eq!(
            fixture
                .repo
                .git(&["for-each-ref", "--format=%(refname)", "refs/pingex/"]),
            objects::head_ref(22)
        );
        assert!(std::path::Path::new(&path).exists());
        let links = crate::storage::read_pr_links(&fixture.desk.database)
            .await
            .unwrap();
        assert_eq!(links[0].closed_at, Some(closed_at));

        let early = closed_at + KEEP_CLOSED_CHECKOUT_SECS - 60;
        let waiting = fixture.desk.prune_closed_at(early).await.unwrap();
        assert!(waiting.removed_checkouts.is_empty());

        let week = closed_at + KEEP_CLOSED_CHECKOUT_SECS;
        let removed = fixture.desk.prune_closed_at(week).await.unwrap();
        assert_eq!(removed.removed_checkouts, [21]);
        assert!(!std::path::Path::new(&path).exists());
        let state = fixture.desk.checkout_state(target).await.unwrap();
        assert_eq!(state.step, CheckoutStep::NoCheckout);
        let _ = still_open;
    }

    #[tokio::test]
    async fn a_reopened_pr_starts_its_seven_days_again_when_it_closes_again() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(21, "feature", "src/lib.rs");
        let target = CheckoutTarget {
            number: 21,
            head_ref: pr.head_ref.clone(),
            head_sha: pr.head_sha.clone(),
            ..CheckoutTarget::default()
        };
        let path = fixture
            .desk
            .ensure_pr_worktree(target)
            .await
            .unwrap()
            .path
            .unwrap();
        let day = 24 * 60 * 60;
        fixture.github.close_pr(21);
        fixture.desk.prune_closed_at(0).await.unwrap();
        fixture.github.put_pr(pr.clone());
        fixture.desk.prune_closed_at(day).await.unwrap();
        fixture.github.close_pr(21);

        let closed_again = fixture.desk.prune_closed_at(8 * day).await.unwrap();

        assert_eq!(closed_again.closed, [21]);
        assert!(closed_again.removed_checkouts.is_empty());
        assert!(std::path::Path::new(&path).exists());
        let later = fixture.desk.prune_closed_at(14 * day).await.unwrap();
        assert!(later.removed_checkouts.is_empty());
        let week = fixture.desk.prune_closed_at(15 * day).await.unwrap();
        assert_eq!(week.removed_checkouts, [21]);
    }

    #[tokio::test]
    async fn pruning_trusts_the_open_prs_it_is_given_rather_than_reading_github_again() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(21, "feature", "src/lib.rs");
        fixture.desk.fetch_prs(vec![21]).await.unwrap();
        fixture
            .desk
            .ensure_pr_worktree(CheckoutTarget {
                number: 21,
                head_ref: pr.head_ref.clone(),
                head_sha: pr.head_sha.clone(),
                ..CheckoutTarget::default()
            })
            .await
            .unwrap();

        // GitHub still lists #21 as open; the sync that ran just before said
        // otherwise, and that answer is the one pruning acts on.
        let report = fixture
            .desk
            .prune_closed_with(std::collections::HashSet::new(), 0)
            .await
            .unwrap();

        assert_eq!(report.closed, [21]);
        assert_eq!(
            fixture
                .repo
                .git(&["for-each-ref", "--format=%(refname)", "refs/pingex/"]),
            ""
        );
    }

    #[tokio::test]
    async fn pruning_never_removes_a_worktree_that_is_the_users() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(1, "one", "src/one.rs");
        let path = fixture
            .desk
            .ensure_pr_worktree(CheckoutTarget {
                number: pr.number,
                head_ref: pr.head_ref.clone(),
                head_sha: pr.head_sha.clone(),
                stack: Some("foo".into()),
                ..CheckoutTarget::default()
            })
            .await
            .unwrap()
            .path
            .unwrap();
        fixture.github.close_pr(1);

        fixture.desk.prune_closed_at(0).await.unwrap();
        let week = fixture
            .desk
            .prune_closed_at(KEEP_CLOSED_CHECKOUT_SECS)
            .await
            .unwrap();

        assert!(week.removed_checkouts.is_empty(), "{week:?}");
        assert!(std::path::Path::new(&path).exists());
    }

    /// A managed worktree seven days after its PR closed, once `prepare` has
    /// run in it: its path, and what pruning then reported.
    async fn prune_after_a_week(
        fixture: &super::super::test_support::Fixture,
        number: u32,
        prepare: impl FnOnce(&std::path::Path),
    ) -> (String, crate::review::checkout_types::PruneReport) {
        let pr = fixture.open_pr(number, "feature", "src/lib.rs");
        let path = fixture
            .desk
            .ensure_pr_worktree(CheckoutTarget {
                number: pr.number,
                head_ref: pr.head_ref.clone(),
                head_sha: pr.head_sha.clone(),
                ..CheckoutTarget::default()
            })
            .await
            .unwrap()
            .path
            .unwrap();
        prepare(std::path::Path::new(&path));
        fixture.github.close_pr(i64::from(number));
        fixture.desk.prune_closed_at(0).await.unwrap();
        let report = fixture
            .desk
            .prune_closed_at(KEEP_CLOSED_CHECKOUT_SECS)
            .await
            .unwrap();
        (path, report)
    }

    #[tokio::test]
    async fn pruning_removes_a_clean_managed_worktree_and_its_branch() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let (path, report) = prune_after_a_week(&fixture, 41, |_| {}).await;

        assert_eq!(report.removed_checkouts, [41]);
        assert!(!std::path::Path::new(&path).exists());
        assert_eq!(fixture.repo.git(&["branch", "--list", "pr/41"]), "");
    }

    #[tokio::test]
    async fn pruning_keeps_a_managed_worktree_with_unpushed_commits() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let (path, report) = prune_after_a_week(&fixture, 42, |dir| {
            std::fs::write(dir.join("measure.txt"), "1.2s").unwrap();
            fixture.repo.git_in(dir, &["add", "measure.txt"]);
            fixture.repo.git_in(dir, &["commit", "-q", "-m", "measure"]);
        })
        .await;

        assert!(report.removed_checkouts.is_empty(), "{report:?}");
        let dir = std::path::Path::new(&path);
        assert!(dir.join("measure.txt").exists());
        assert_eq!(
            fixture.repo.git_in(dir, &["log", "-1", "--format=%s"]),
            "measure"
        );
    }

    #[tokio::test]
    async fn pruning_keeps_a_managed_worktree_with_untracked_output() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let (path, report) = prune_after_a_week(&fixture, 43, |dir| {
            std::fs::write(dir.join("results.csv"), "1,2").unwrap();
        })
        .await;

        assert!(report.removed_checkouts.is_empty(), "{report:?}");
        assert!(std::path::Path::new(&path).join("results.csv").exists());
    }

    #[tokio::test]
    async fn pruning_keeps_a_managed_worktree_whose_last_head_it_cannot_find() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(44, "feature", "src/lib.rs");
        let target = CheckoutTarget {
            number: pr.number,
            head_ref: pr.head_ref.clone(),
            head_sha: pr.head_sha.clone(),
            ..CheckoutTarget::default()
        };
        let path = fixture
            .desk
            .ensure_pr_worktree(target.clone())
            .await
            .unwrap()
            .path
            .unwrap();
        // The PR moved to a commit this repository never fetched.
        fixture
            .desk
            .checkout_state(CheckoutTarget {
                head_sha: "a".repeat(40),
                ..target
            })
            .await
            .unwrap();
        fixture.github.close_pr(44);
        fixture.desk.prune_closed_at(0).await.unwrap();

        let report = fixture
            .desk
            .prune_closed_at(KEEP_CLOSED_CHECKOUT_SECS)
            .await
            .unwrap();

        assert!(report.removed_checkouts.is_empty(), "{report:?}");
        assert!(std::path::Path::new(&path).exists());
    }

    #[tokio::test]
    async fn a_forgotten_pr_leaves_no_refs() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(11, "feature", "src/lib.rs");
        fixture.desk.fetch_prs(vec![11]).await.unwrap();
        fixture.desk.mark_seen(11, pr.head_sha).await.unwrap();

        fixture.desk.forget_pr(11).await.unwrap();

        assert_eq!(
            fixture.repo.git(&["for-each-ref", "refs/pingex/"]),
            "",
            "{:?}",
            objects::head_ref(11)
        );
    }
}
