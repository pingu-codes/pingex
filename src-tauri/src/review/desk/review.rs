//! One PR under review: its detail, whether it went stale, the local diff
//! that works before a PR exists, and the review written on it. The typed
//! pending review joins this file.

use super::ReviewDesk;
use crate::git::run_git;
use crate::review::diff::parse_git_diff;
use crate::review::types::{
    CommentRef, PrDetail, PrFile, PrFreshness, PrSummary, ProviderStatus, ReviewSubmission,
};

/// Local diffs should come back as fast as a `gh` read.
const LOCAL_DIFF_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

impl ReviewDesk {
    pub(crate) async fn provider_status(&self) -> Result<ProviderStatus, String> {
        self.blocking("Provider check failed", |desk| Ok(desk.github.status()))
            .await
    }

    /// Open PRs for the single-PR picker.
    pub(crate) async fn list_prs(&self) -> Result<Vec<PrSummary>, String> {
        self.blocking("Pull-request listing failed", |desk| {
            desk.github.list_open_prs()
        })
        .await
    }

    pub(crate) async fn pr_detail(&self, number: i64) -> Result<PrDetail, String> {
        self.blocking("Pull-request fetch failed", move |desk| {
            desk.github.pr_detail(number)
        })
        .await
    }

    /// Whether the PR moved on GitHub since it was read at `known_head`.
    pub(crate) async fn check_fresh(
        &self,
        number: i64,
        known_head: String,
        known_updated_at: String,
    ) -> Result<PrFreshness, String> {
        self.blocking("Freshness check failed", move |desk| {
            let remote = desk.github.pr_head(number)?;
            Ok(freshness(
                &known_head,
                &known_updated_at,
                &remote.head_sha,
                &remote.updated_at,
            ))
        })
        .await
    }

    /// Diff the working tree against a base revision, as `PrFile`s, so the
    /// review view works before a PR exists.
    pub(crate) async fn local_diff(
        &self,
        base: String,
        head: Option<String>,
    ) -> Result<Vec<PrFile>, String> {
        self.blocking("Local diff failed", move |desk| {
            let range = diff_range(&base, head.as_deref())?;
            // The trailing `--` keeps the range from being read as a path.
            let output = run_git(
                &desk.host,
                desk.dir(),
                &["diff", "--no-color", &range, "--"],
                LOCAL_DIFF_TIMEOUT,
            )?;
            if !output.ok {
                return Err("Could not diff against that base revision".to_string());
            }
            Ok(parse_git_diff(&output.stdout))
        })
        .await
    }

    /// Post one review with its inline comments.
    pub(crate) async fn submit_review(
        &self,
        number: i64,
        review: ReviewSubmission,
    ) -> Result<(), String> {
        self.blocking("Review submission failed", move |desk| {
            desk.github.submit_review(number, &review)
        })
        .await
    }

    /// Reply in an inline comment's thread at once, outside any review.
    pub(crate) async fn reply(
        &self,
        number: i64,
        comment_id: i64,
        body: String,
    ) -> Result<(), String> {
        self.blocking("Reply failed", move |desk| {
            desk.github.reply(number, comment_id, &body)
        })
        .await
    }

    pub(crate) async fn resolve_thread(&self, thread_id: String) -> Result<(), String> {
        self.blocking("Resolve failed", move |desk| {
            desk.github.resolve_thread(&thread_id)
        })
        .await
    }

    pub(crate) async fn edit_comment(
        &self,
        comment: CommentRef,
        body: String,
    ) -> Result<(), String> {
        self.blocking("Editing the comment failed", move |desk| {
            desk.github.edit_comment(comment, &body)
        })
        .await
    }

    pub(crate) async fn delete_comment(&self, comment: CommentRef) -> Result<(), String> {
        self.blocking("Deleting the comment failed", move |desk| {
            desk.github.delete_comment(comment)
        })
        .await
    }
}

/// A PR is stale when GitHub's head or `updatedAt` differs from what was
/// read. An empty answer (the read failed) is not drift.
fn freshness(
    known_head: &str,
    known_updated_at: &str,
    remote_head: &str,
    remote_updated_at: &str,
) -> PrFreshness {
    let stale = (!remote_head.is_empty() && remote_head != known_head)
        || (!remote_updated_at.is_empty() && remote_updated_at != known_updated_at);
    PrFreshness {
        stale,
        remote_head: remote_head.to_string(),
        remote_updated_at: remote_updated_at.to_string(),
    }
}

/// The `git diff` revision argument for a user-typed base and head. A
/// revision starting with `-` would be parsed as an option (`--output=...`
/// writes a file), so it is refused rather than passed through.
fn diff_range(base: &str, head: Option<&str>) -> Result<String, String> {
    let base = base.trim();
    let head = head.map(str::trim).filter(|head| !head.is_empty());
    if base.is_empty() {
        return Err("Enter a base revision".to_string());
    }
    if base.starts_with('-') || head.is_some_and(|head| head.starts_with('-')) {
        return Err("A revision cannot start with '-'".to_string());
    }
    Ok(match head {
        Some(head) => format!("{base}...{head}"),
        None => base.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::super::test_support::fixture;
    use super::*;
    use crate::review::fake::Sent;
    use crate::review::types::{InboxPr, PendingComment, PrDetail, ReviewVerdict};

    #[tokio::test]
    async fn a_submitted_review_reaches_github_pinned_to_the_head_it_was_read_at() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(3, "feature", "src/lib.rs");
        let review = ReviewSubmission {
            verdict: ReviewVerdict::RequestChanges,
            body: "Two things".into(),
            commit_id: Some(pr.head_sha.clone()),
            comments: vec![PendingComment {
                path: "src/lib.rs".into(),
                line: 1,
                side: Some("RIGHT".into()),
                start_line: None,
                start_side: None,
                body: "Name this".into(),
            }],
            replies: vec![],
        };

        fixture.desk.submit_review(3, review.clone()).await.unwrap();

        assert_eq!(fixture.github.sent(), [Sent::Review { number: 3, review }]);
    }

    #[tokio::test]
    async fn comment_edits_name_the_collection_the_comment_lives_in() {
        let Some(fixture) = fixture().await else {
            return;
        };
        fixture
            .desk
            .edit_comment(CommentRef::Review(5), "Better".into())
            .await
            .unwrap();
        fixture
            .desk
            .delete_comment(CommentRef::Conversation(6))
            .await
            .unwrap();
        assert_eq!(
            fixture.github.sent(),
            [
                Sent::EditComment {
                    comment: CommentRef::Review(5),
                    body: "Better".into()
                },
                Sent::DeleteComment(CommentRef::Conversation(6)),
            ]
        );
    }

    #[tokio::test]
    async fn a_push_after_the_read_makes_the_pr_stale() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(3, "feature", "src/lib.rs");
        let read_at = (pr.head_sha.clone(), pr.updated_at.clone());

        let fresh = fixture
            .desk
            .check_fresh(3, read_at.0.clone(), read_at.1.clone())
            .await
            .unwrap();
        assert!(!fresh.stale);

        fixture.github.put_pr(InboxPr {
            head_sha: "e".repeat(40),
            ..pr
        });
        let stale = fixture
            .desk
            .check_fresh(3, read_at.0, read_at.1)
            .await
            .unwrap();
        assert!(stale.stale);
        assert_eq!(stale.remote_head, "e".repeat(40));
    }

    #[tokio::test]
    async fn the_local_diff_covers_uncommitted_work_and_refuses_option_like_revisions() {
        let Some(fixture) = fixture().await else {
            return;
        };
        std::fs::write(fixture.repo.repo.join("README.md"), "hello again\n").unwrap();

        let files = fixture.desk.local_diff("HEAD".into(), None).await.unwrap();
        assert_eq!(files[0].path, "README.md");
        assert!(fixture
            .desk
            .local_diff("--output=/tmp/x".into(), None)
            .await
            .is_err());
        assert!(fixture
            .desk
            .local_diff("main".into(), Some("--output=/tmp/x".into()))
            .await
            .is_err());
        assert!(fixture.desk.local_diff(" ".into(), None).await.is_err());
    }

    #[tokio::test]
    async fn a_pr_reads_back_as_github_has_it_and_an_unknown_one_is_an_error() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let detail = PrDetail {
            body: "Adds the desk".into(),
            head_sha: "c".repeat(40),
            ..PrDetail::default()
        };
        fixture.github.put_detail(8, detail.clone());
        assert_eq!(fixture.desk.pr_detail(8).await.unwrap(), detail);
        assert!(fixture.desk.pr_detail(9).await.is_err());
    }

    #[test]
    fn an_empty_answer_is_not_drift() {
        assert!(!freshness("abc", "t1", "", "").stale);
        assert!(freshness("abc", "t1", "abc", "t2").stale);
    }
}
