//! The pending review: the reviewer's unsubmitted review of one PR, kept in
//! the profile database across reloads and restarts, and posted to GitHub
//! as one review.
//!
//! Stored in `review_drafts`, one row per (repository, PR), tagged with the
//! head the entries were written against.

use super::{ReviewDesk, PROVIDER};
use crate::review::pending::PendingReview;
use crate::storage;
use crate::util::time::unix_secs;

impl ReviewDesk {
    /// The repository's `owner/name`, which stored rows are keyed by.
    pub(super) async fn slug(&self) -> Result<String, String> {
        self.blocking("Reading the repository failed", |desk| desk.github.repo())
            .await
    }

    /// PR `number`'s pending review, if the reviewer has started one.
    pub(crate) async fn pending_review(
        &self,
        number: i64,
    ) -> Result<Option<PendingReview>, String> {
        let slug = self.slug().await?;
        let stored = storage::read_review_draft(&self.database, PROVIDER, &slug, number).await?;
        Ok(stored.and_then(|draft| {
            let mut review = PendingReview::from_stored(&draft.head_sha, &draft.payload)?;
            review.updated_at = draft.updated_at;
            Some(review)
        }))
    }

    /// Replace PR `number`'s pending review. An empty one is dropped.
    pub(crate) async fn save_pending_review(
        &self,
        number: i64,
        mut review: PendingReview,
    ) -> Result<(), String> {
        if review.is_empty() {
            return self.discard_pending_review(number).await;
        }
        let slug = self.slug().await?;
        review.updated_at = unix_secs();
        let payload = serde_json::to_string(&review).map_err(|error| error.to_string())?;
        storage::write_review_draft(
            &self.database,
            PROVIDER,
            &slug,
            number,
            &review.head_sha,
            &payload,
        )
        .await
    }

    pub(crate) async fn discard_pending_review(&self, number: i64) -> Result<(), String> {
        let slug = self.slug().await?;
        storage::delete_review_draft(&self.database, PROVIDER, &slug, number).await
    }

    /// Post PR `number`'s pending review as one GitHub review, then forget
    /// it. On failure it stays, so nothing the reviewer wrote is lost.
    pub(crate) async fn submit_pending_review(&self, number: i64) -> Result<(), String> {
        let review = self
            .pending_review(number)
            .await?
            .ok_or_else(|| "There is no pending review to submit".to_string())?;
        let submission = review.submission()?;
        self.submit_review(number, submission).await?;
        self.discard_pending_review(number).await
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::fixture;
    use crate::review::fake::Sent;
    use crate::review::pending::{DiffAnchor, PendingEntry, PendingReview};
    use crate::review::types::{PendingComment, ReviewReply, ReviewSubmission, ReviewVerdict};

    fn anchor(path: &str, line: i64, start_line: Option<i64>) -> DiffAnchor {
        DiffAnchor {
            path: path.into(),
            line,
            side: "RIGHT".into(),
            start_line,
        }
    }

    #[tokio::test]
    async fn a_pending_review_reaches_github_as_one_review_and_is_then_gone() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(4, "feature", "src/lib.rs");
        let pending = PendingReview {
            head_sha: pr.head_sha.clone(),
            summary: "Close, two things".into(),
            verdict: ReviewVerdict::RequestChanges,
            entries: vec![
                PendingEntry::Comment {
                    id: "c1".into(),
                    anchor: anchor("src/lib.rs", 3, Some(1)),
                    body: "Split this".into(),
                },
                PendingEntry::Suggestion {
                    id: "s1".into(),
                    anchor: anchor("src/lib.rs", 5, None),
                    body: "Name it".into(),
                    replacement: "pub fn named() {}".into(),
                },
                PendingEntry::Reply {
                    id: "r1".into(),
                    thread_id: "THREAD_9".into(),
                    comment_id: 90,
                    path: "src/lib.rs".into(),
                    line: Some(1),
                    body: "Agreed".into(),
                },
                PendingEntry::AcceptedFinding {
                    id: "f1".into(),
                    finding_id: "finding-7".into(),
                    anchor: anchor("src/lib.rs", 8, None),
                    body: "This unwrap panics on an empty file".into(),
                },
            ],
            ..PendingReview::default()
        };
        fixture.desk.save_pending_review(4, pending).await.unwrap();

        // It reads back from the profile database, as after a restart.
        let stored = fixture.desk.pending_review(4).await.unwrap().unwrap();
        assert_eq!(stored.entries.len(), 4);
        assert_eq!(stored.posted_comment_count(), 4);

        fixture.desk.submit_pending_review(4).await.unwrap();

        let comment = |line: i64, start_line: Option<i64>, body: &str| PendingComment {
            path: "src/lib.rs".into(),
            line,
            side: Some("RIGHT".into()),
            start_line,
            start_side: start_line.map(|_| "RIGHT".into()),
            body: body.into(),
        };
        assert_eq!(
            fixture.github.sent(),
            [Sent::Review {
                number: 4,
                review: ReviewSubmission {
                    verdict: ReviewVerdict::RequestChanges,
                    body: "Close, two things".into(),
                    commit_id: Some(pr.head_sha.clone()),
                    comments: vec![
                        comment(3, Some(1), "Split this"),
                        comment(5, None, "Name it\n\n```suggestion\npub fn named() {}\n```"),
                        comment(8, None, "This unwrap panics on an empty file"),
                    ],
                    replies: vec![ReviewReply {
                        thread_id: "THREAD_9".into(),
                        body: "Agreed".into(),
                    }],
                },
            }]
        );
        assert_eq!(fixture.desk.pending_review(4).await.unwrap(), None);
    }
}
