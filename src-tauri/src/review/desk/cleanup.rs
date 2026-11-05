//! Cleanup: what a closed PR leaves behind. Today its refs; review checkouts
//! join this file.

use super::{pr_number, ReviewDesk};
use crate::git::objects;

impl ReviewDesk {
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
    use crate::git::objects;

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
