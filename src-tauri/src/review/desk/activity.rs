//! Activity seen state, per profile: which inbox activity the viewer has
//! looked at ("N new", the unseen dot and the "Since you last looked"
//! feed), and PRs muted until new commits. The Seen head, which "changes
//! since" measures from, stays a git ref (`seen`).

use super::{ReviewDesk, PROVIDER};
use crate::storage;

impl ReviewDesk {
    /// Mark the activity on PRs `numbers` seen: their feed entries go and
    /// their rows stop showing "N new".
    pub(crate) async fn mark_activity_seen(&self, numbers: Vec<i64>) -> Result<(), String> {
        let repo = self.repo_slug().await?;
        storage::mark_prs_seen(&self.database, PROVIDER, &repo, &numbers).await
    }

    /// Leave PR `number` out of Needs me, and its activity out of the feed,
    /// until its head moves off `head_sha`.
    pub(crate) async fn mute_until_new_commits(
        &self,
        number: i64,
        head_sha: String,
    ) -> Result<(), String> {
        let repo = self.repo_slug().await?;
        storage::mute_pr(&self.database, PROVIDER, &repo, number, &head_sha).await
    }

    async fn repo_slug(&self) -> Result<String, String> {
        self.blocking("Could not read the repository", |desk| desk.github.repo())
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{fixture, VIEWER};
    use crate::review::types::{Inbox, InboxPr, InboxReview, NeedsMe};

    fn new_activity(inbox: &Inbox, number: i64) -> i64 {
        inbox
            .triage
            .iter()
            .find(|triage| triage.number == number)
            .map_or(0, |triage| triage.new_activity)
    }

    #[tokio::test]
    async fn activity_builds_up_across_syncs_until_marked_seen() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(4, "feature", "src/lib.rs");
        let first = fixture.desk.sync_inbox().await.unwrap();
        assert_eq!(new_activity(&first, 4), 0, "the first sync is the baseline");
        assert!(first.feed.is_empty());

        fixture.github.put_pr(InboxPr {
            head_sha: "f".repeat(40),
            ..pr.clone()
        });
        fixture.desk.sync_inbox().await.unwrap();
        fixture.github.put_pr(InboxPr {
            head_sha: "f".repeat(40),
            checks: Some("FAILURE".into()),
            ..pr
        });
        fixture.desk.sync_inbox().await.unwrap();
        let unchanged = fixture.desk.sync_inbox().await.unwrap();
        let feed: Vec<(i64, &str)> = unchanged
            .feed
            .iter()
            .map(|entry| (entry.number, entry.kind.as_str()))
            .collect();
        assert_eq!(feed, [(4, "checksFailed"), (4, "pushed")], "newest first");
        assert_eq!(new_activity(&unchanged, 4), 2);

        fixture.desk.mark_activity_seen(vec![4]).await.unwrap();
        let seen = fixture.desk.sync_inbox().await.unwrap();
        assert!(seen.feed.is_empty());
        assert_eq!(new_activity(&seen, 4), 0);
    }

    #[tokio::test]
    async fn a_muted_pr_stays_out_of_needs_me_until_new_commits() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = InboxPr {
            number: 7,
            author: VIEWER.into(),
            head_sha: "a".repeat(40),
            checks: Some("FAILURE".into()),
            ..InboxPr::default()
        };
        fixture.github.put_pr(pr.clone());
        let before = fixture.desk.sync_inbox().await.unwrap();
        assert_eq!(before.counts.needs_me, 1);

        fixture
            .desk
            .mute_until_new_commits(7, pr.head_sha.clone())
            .await
            .unwrap();
        fixture.github.put_pr(InboxPr {
            reviews: vec![InboxReview {
                author: "mira-k".into(),
                state: "COMMENTED".into(),
                commit: None,
            }],
            ..pr.clone()
        });
        let muted = fixture.desk.sync_inbox().await.unwrap();
        assert_eq!(muted.counts.needs_me, 0);
        assert!(muted.triage[0].muted);
        assert!(muted.feed.is_empty(), "a muted PR's activity stays quiet");

        fixture.github.put_pr(InboxPr {
            head_sha: "b".repeat(40),
            ..pr
        });
        let pushed = fixture.desk.sync_inbox().await.unwrap();
        assert_eq!(pushed.triage[0].needs_me, Some(NeedsMe::ChecksFailing));
        assert!(!pushed.triage[0].muted);
        assert_eq!(pushed.feed.len(), 1);
    }
}
