//! Changes to a PR itself, none of which needs a checkout: merge, close,
//! retarget, mark ready, its failing checks, and opening a new PR.

use super::ReviewDesk;
use crate::review::types::{CheckLog, MergeRequest, NewPr};

impl ReviewDesk {
    pub(crate) async fn merge(&self, number: i64, request: MergeRequest) -> Result<(), String> {
        self.blocking("Merging failed", move |desk| {
            desk.github.merge(number, &request)
        })
        .await
    }

    pub(crate) async fn close(&self, number: i64) -> Result<(), String> {
        self.blocking("Closing failed", move |desk| desk.github.close(number))
            .await
    }

    /// Point PR `number` at another base git branch.
    pub(crate) async fn retarget(&self, number: i64, base: String) -> Result<(), String> {
        self.blocking("Retargeting failed", move |desk| {
            desk.github.retarget(number, &base)
        })
        .await
    }

    pub(crate) async fn mark_ready(&self, number: i64) -> Result<(), String> {
        self.blocking("Marking ready failed", move |desk| {
            desk.github.mark_ready(number)
        })
        .await
    }

    /// The finished logs of PR `number`'s failing checks.
    pub(crate) async fn failed_check_logs(&self, number: i64) -> Result<Vec<CheckLog>, String> {
        self.blocking("Reading check logs failed", move |desk| {
            desk.github.failed_check_logs(number)
        })
        .await
    }

    pub(crate) async fn rerun_failed_checks(&self, number: i64) -> Result<(), String> {
        self.blocking("Re-running checks failed", move |desk| {
            desk.github.rerun_failed_checks(number)
        })
        .await
    }

    /// Open a PR from a pushed git branch; returns its number.
    pub(crate) async fn create_pr(&self, pr: NewPr) -> Result<i64, String> {
        self.blocking("Opening the pull request failed", move |desk| {
            desk.github.create_pr(&pr)
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::fixture;
    use crate::review::fake::Sent;
    use crate::review::types::{CheckLog, MergeMethod, MergeRequest, NewPr};

    #[tokio::test]
    async fn pr_writes_reach_github_without_a_checkout() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(21, "feature", "src/lib.rs");
        let merge = MergeRequest {
            method: MergeMethod::Squash,
            head_sha: Some(pr.head_sha.clone()),
        };

        fixture.desk.mark_ready(21).await.unwrap();
        fixture.desk.retarget(21, "release".into()).await.unwrap();
        fixture.desk.merge(21, merge.clone()).await.unwrap();

        assert_eq!(
            fixture.github.sent(),
            [
                Sent::MarkReady(21),
                Sent::Retarget {
                    number: 21,
                    base: "release".into()
                },
                Sent::Merge {
                    number: 21,
                    request: merge
                },
            ]
        );
        assert!(fixture.desk.sync_inbox().await.unwrap().prs.is_empty());
        assert_eq!(fixture.repo.git(&["status", "--porcelain"]), "");
    }

    #[tokio::test]
    async fn a_new_pr_comes_back_with_its_number_and_shows_in_the_inbox() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = NewPr {
            base: "feature".into(),
            head: "feature-2".into(),
            title: "On top".into(),
            ..NewPr::default()
        };

        let number = fixture.desk.create_pr(pr.clone()).await.unwrap();

        assert_eq!(fixture.github.sent(), [Sent::CreatePr(pr)]);
        let inbox = fixture.desk.sync_inbox().await.unwrap();
        assert_eq!(inbox.prs[0].number, number);
        assert_eq!(inbox.prs[0].base_ref, "feature");
    }

    #[tokio::test]
    async fn failing_checks_can_be_read_and_re_run() {
        let Some(fixture) = fixture().await else {
            return;
        };
        fixture.open_pr(22, "feature", "src/lib.rs");
        let log = CheckLog {
            name: "test".into(),
            log: "assertion failed".into(),
            truncated: false,
        };
        fixture.github.put_check_logs(22, vec![log.clone()]);

        assert_eq!(fixture.desk.failed_check_logs(22).await.unwrap(), [log]);
        fixture.desk.rerun_failed_checks(22).await.unwrap();
        assert_eq!(fixture.github.sent(), [Sent::RerunFailedChecks(22)]);
    }
}
