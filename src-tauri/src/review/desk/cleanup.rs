//! Cleanup: what a closed PR leaves behind. Its refs go when it is first
//! seen closed, its review checkout 7 days later, and threads linked to it
//! leave the sidebar's Reviews group.

use std::collections::HashSet;

use super::{pr_number, ReviewDesk};
use crate::git::{checkouts as git, objects};
use crate::review::checkout_types::PruneReport;
use crate::storage;
use crate::util::time::unix_secs;

/// How long a closed PR's review checkout is kept.
const KEEP_CLOSED_CHECKOUT_SECS: i64 = 7 * 24 * 60 * 60;

impl ReviewDesk {
    /// Compare what Pingex keeps for this repository with its open PRs, and
    /// drop what closed PRs left behind. Call after a sync.
    pub(crate) async fn prune_closed(&self) -> Result<PruneReport, String> {
        self.prune_closed_at(unix_secs()).await
    }

    pub(super) async fn prune_closed_at(&self, now: i64) -> Result<PruneReport, String> {
        let (open, kept) = self
            .blocking("Cleaning up failed", |desk| {
                let inbox = desk.github.inbox()?;
                let kept = git::kept_pr_numbers(&desk.host, desk.dir())?;
                let open: Option<HashSet<i64>> = inbox
                    .complete
                    .then(|| inbox.prs.iter().map(|pr| pr.number).collect());
                Ok((open, kept))
            })
            .await?;
        // A partial read cannot say which PRs closed.
        let Some(open) = open else {
            return Ok(PruneReport::default());
        };
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
                continue;
            }
            match checkout.closed_at {
                None => {
                    storage::mark_checkout_closed(&self.database, &key, checkout.pr_number, now)
                        .await?;
                    report.closed.push(checkout.pr_number);
                }
                Some(closed) if now - closed >= KEEP_CLOSED_CHECKOUT_SECS => {
                    if checkout.step == "review_checkout" {
                        self.remove_review_checkout(checkout.pr_number).await?;
                        report.removed_checkouts.push(checkout.pr_number);
                    } else {
                        // A worktree is the user's: forget only the record.
                        storage::delete_checkout(&self.database, &key, checkout.pr_number).await?;
                    }
                }
                Some(_) => {}
            }
        }

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
    async fn a_closed_prs_refs_go_at_once_and_its_review_checkout_after_seven_days() {
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
            .ensure_review_checkout(target.clone())
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
