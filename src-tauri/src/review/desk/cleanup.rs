//! Cleanup: what a closed PR leaves behind. Its refs go when it is first
//! seen closed, and the worktree Pingex made for it 7 days later (a stack's
//! once every PR in it has closed), unless it holds work; an adopted worktree
//! is only forgotten. Threads linked to it leave the sidebar's Reviews group.

use std::collections::{BTreeMap, HashSet};

use super::check_out::Removal;
use super::{pr_number, ReviewDesk};
use crate::git::{checkouts as git, objects};
use crate::review::checkout_types::PruneReport;
use crate::storage::{self, StoredCheckout};
use crate::util::time::unix_secs;

/// How long a closed PR's worktree is kept.
const KEEP_CLOSED_CHECKOUT_SECS: i64 = 7 * 24 * 60 * 60;

impl ReviewDesk {
    /// Compare what Pingex keeps for this repository with `open`, the PRs a
    /// complete inbox sync just listed, and drop what closed PRs left behind.
    /// The caller passes the sync's own answer: fetching the inbox again here
    /// doubled every sync's GitHub reads (seconds per page on a busy repo).
    /// `complete` says whether that sync listed every open PR: a partial list
    /// cannot say which PRs closed, so it prunes nothing.
    pub(crate) async fn prune_closed(
        &self,
        open: HashSet<i64>,
        complete: bool,
    ) -> Result<PruneReport, String> {
        if !complete {
            return Ok(PruneReport::default());
        }
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
            } else if checkout.closed_at.is_none() {
                storage::mark_checkout_closed(&self.database, &key, checkout.pr_number, now)
                    .await?;
                report.closed.push(checkout.pr_number);
            }
        }

        // A worktree goes once every PR checked out in it (one, or a whole
        // stack) has been closed for seven days.
        let mut by_path: BTreeMap<String, Vec<StoredCheckout>> = BTreeMap::new();
        for checkout in storage::read_checkouts(&self.database, &key).await? {
            by_path
                .entry(checkout.path.clone())
                .or_default()
                .push(checkout);
        }
        for (path, rows) in by_path {
            let closed_long_ago = rows.iter().all(|row| {
                row.closed_at
                    .is_some_and(|closed| now - closed >= KEEP_CLOSED_CHECKOUT_SECS)
            });
            if !closed_long_ago {
                continue;
            }
            // Whether it can go is only proven by looking, so any doubt, or
            // any failure, leaves it for the user to see in the Worktrees
            // list. An adopted worktree loses only its rows.
            let made = !rows.iter().any(|row| row.adopted)
                && self.host.is_dir(&path)
                && self
                    .is_managed_worktree(path.clone())
                    .await
                    .unwrap_or(false);
            let removal = self.try_remove_worktree(&path).await;
            if made && matches!(removal, Ok(Removal::Removed)) {
                report
                    .removed_checkouts
                    .extend(rows.iter().map(|row| row.pr_number));
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
    use std::path::Path;

    use super::super::stack_support::StackFixture;
    use super::super::test_support::{fixture, Fixture};
    use super::KEEP_CLOSED_CHECKOUT_SECS;
    use crate::git::objects;
    use crate::review::checkout_types::{CheckOutOutcome, PruneReport};

    #[tokio::test]
    async fn a_closed_prs_refs_go_at_once_and_its_worktree_after_seven_days() {
        let Some(fixture) = fixture().await else {
            return;
        };
        fixture.open_pr(21, "feature", "src/lib.rs");
        let still_open = fixture.open_pr(22, "other", "src/other.rs");
        fixture.desk.fetch_prs(vec![21, 22]).await.unwrap();
        let path = fixture.check_out(21).await.path;
        let link = fixture
            .desk
            .link_thread("thread-1".into(), 21)
            .await
            .unwrap()
            .link;
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
        assert!(Path::new(&path).exists());
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
        assert!(!Path::new(&path).exists());
        assert_eq!(fixture.desk.pr_checkout(21).await.unwrap(), None);
        let _ = still_open;
    }

    #[tokio::test]
    async fn a_reopened_pr_starts_its_seven_days_again_when_it_closes_again() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(21, "feature", "src/lib.rs");
        let path = fixture.check_out(21).await.path;
        let day = 24 * 60 * 60;
        fixture.github.close_pr(21);
        fixture.desk.prune_closed_at(0).await.unwrap();
        fixture.github.put_pr(pr.clone());
        fixture.desk.prune_closed_at(day).await.unwrap();
        fixture.github.close_pr(21);

        let closed_again = fixture.desk.prune_closed_at(8 * day).await.unwrap();

        assert_eq!(closed_again.closed, [21]);
        assert!(closed_again.removed_checkouts.is_empty());
        assert!(Path::new(&path).exists());
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
        fixture.open_pr(21, "feature", "src/lib.rs");
        fixture.desk.fetch_prs(vec![21]).await.unwrap();
        fixture.check_out(21).await;

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
    async fn a_list_from_an_incomplete_sync_prunes_nothing() {
        let Some(fixture) = fixture().await else {
            return;
        };
        fixture.open_pr(21, "feature", "src/lib.rs");
        fixture.desk.fetch_prs(vec![21]).await.unwrap();
        fixture.check_out(21).await;

        let report = fixture
            .desk
            .prune_closed(std::collections::HashSet::new(), false)
            .await
            .unwrap();

        assert!(report.closed.is_empty(), "{report:?}");
        assert_eq!(
            fixture
                .repo
                .git(&["for-each-ref", "--format=%(refname)", "refs/pingex/"]),
            objects::head_ref(21)
        );
        let row =
            crate::storage::read_checkout(&fixture.desk.database, &fixture.desk.repo_key(), 21)
                .await
                .unwrap()
                .unwrap();
        assert_eq!(row.closed_at, None);
    }

    /// PR `number`'s worktree seven days after the PR closed, once `prepare`
    /// has run in it: its path, and what pruning then reported.
    async fn prune_after_a_week(
        fixture: &Fixture,
        number: u32,
        mine: bool,
        prepare: impl FnOnce(&Path),
    ) -> (String, PruneReport) {
        if mine {
            fixture.open_my_pr(number, "feature", "src/lib.rs");
        } else {
            fixture.open_pr(number, "feature", "src/lib.rs");
        }
        let path = fixture.check_out(i64::from(number)).await.path;
        prepare(Path::new(&path));
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
    async fn pruning_removes_a_clean_worktree_and_its_pr_branch() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let (path, report) = prune_after_a_week(&fixture, 41, false, |_| {}).await;

        assert_eq!(report.removed_checkouts, [41]);
        assert!(!Path::new(&path).exists());
        assert_eq!(fixture.repo.git(&["branch", "--list", "pr/41"]), "");
    }

    #[tokio::test]
    async fn pruning_removes_my_prs_worktree_too_and_keeps_its_branch() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let (path, report) = prune_after_a_week(&fixture, 45, true, |_| {}).await;

        assert_eq!(report.removed_checkouts, [45]);
        assert!(!Path::new(&path).exists());
        assert_ne!(fixture.repo.git(&["branch", "--list", "feature"]), "");
    }

    #[tokio::test]
    async fn pruning_keeps_my_prs_worktree_with_unpushed_commits() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let (path, report) = prune_after_a_week(&fixture, 46, true, |dir| {
            std::fs::write(dir.join("measure.txt"), "1.2s").unwrap();
            fixture.repo.git_in(dir, &["add", "measure.txt"]);
            fixture.repo.git_in(dir, &["commit", "-q", "-m", "measure"]);
        })
        .await;

        assert!(report.removed_checkouts.is_empty(), "{report:?}");
        assert!(Path::new(&path).join("measure.txt").exists());
    }

    #[tokio::test]
    async fn pruning_keeps_a_worktree_with_unpushed_commits() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let (path, report) = prune_after_a_week(&fixture, 42, false, |dir| {
            std::fs::write(dir.join("measure.txt"), "1.2s").unwrap();
            fixture.repo.git_in(dir, &["add", "measure.txt"]);
            fixture.repo.git_in(dir, &["commit", "-q", "-m", "measure"]);
        })
        .await;

        assert!(report.removed_checkouts.is_empty(), "{report:?}");
        let dir = Path::new(&path);
        assert!(dir.join("measure.txt").exists());
        assert_eq!(
            fixture.repo.git_in(dir, &["log", "-1", "--format=%s"]),
            "measure"
        );
    }

    #[tokio::test]
    async fn pruning_keeps_a_worktree_with_untracked_output() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let (path, report) = prune_after_a_week(&fixture, 43, false, |dir| {
            std::fs::write(dir.join("results.csv"), "1,2").unwrap();
        })
        .await;

        assert!(report.removed_checkouts.is_empty(), "{report:?}");
        assert!(Path::new(&path).join("results.csv").exists());
    }

    #[tokio::test]
    async fn pruning_keeps_a_worktree_whose_last_head_it_cannot_find() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(44, "feature", "src/lib.rs");
        let path = fixture.check_out(44).await.path;
        // The PR moved to a commit this repository never fetched.
        fixture.relist(&crate::review::types::InboxPr {
            head_sha: "a".repeat(40),
            ..pr
        });
        fixture.check_out(44).await;
        fixture.github.close_pr(44);
        fixture.desk.prune_closed_at(0).await.unwrap();

        let report = fixture
            .desk
            .prune_closed_at(KEEP_CLOSED_CHECKOUT_SECS)
            .await
            .unwrap();

        assert!(report.removed_checkouts.is_empty(), "{report:?}");
        assert!(Path::new(&path).exists());
    }

    #[tokio::test]
    async fn an_adopted_worktree_is_only_forgotten() {
        let Some(fixture) = fixture().await else {
            return;
        };
        fixture.open_my_pr(47, "feature", "src/lib.rs");
        let place = fixture.repo.repo.parent().unwrap().join("by-hand");
        let path = place.to_str().unwrap().to_string();
        fixture
            .repo
            .git(&["worktree", "add", "-q", &path, "feature"]);
        let CheckOutOutcome::BranchInUse { path: found, .. } = fixture.check_out_now(47).await
        else {
            panic!("the branch is in use");
        };
        fixture.desk.adopt_worktree(47, found).await.unwrap();
        fixture.github.close_pr(47);
        fixture.desk.prune_closed_at(0).await.unwrap();

        let report = fixture
            .desk
            .prune_closed_at(KEEP_CLOSED_CHECKOUT_SECS)
            .await
            .unwrap();

        assert!(report.removed_checkouts.is_empty(), "{report:?}");
        assert!(place.exists());
        assert_eq!(fixture.desk.pr_checkout(47).await.unwrap(), None);
    }

    #[tokio::test]
    async fn a_stack_worktree_goes_once_every_pr_in_it_has_closed() {
        let Some(stack) = StackFixture::new().await else {
            return;
        };
        let path = stack.checked_out().await.path;
        stack.inner.github.close_pr(412);
        stack.desk().prune_closed_at(0).await.unwrap();

        let some = stack
            .desk()
            .prune_closed_at(KEEP_CLOSED_CHECKOUT_SECS)
            .await
            .unwrap();
        assert!(some.removed_checkouts.is_empty(), "{some:?}");
        assert!(Path::new(&path).exists());

        stack.inner.github.close_pr(415);
        stack.inner.github.close_pr(418);
        let day = 24 * 60 * 60;
        stack.desk().prune_closed_at(day).await.unwrap();
        let all = stack
            .desk()
            .prune_closed_at(day + KEEP_CLOSED_CHECKOUT_SECS)
            .await
            .unwrap();

        assert_eq!(all.removed_checkouts, [412, 415, 418]);
        assert!(!Path::new(&path).exists());
        assert_ne!(stack.git(&["branch", "--list", "feat/maki"]), "");
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
