//! Sync: a repository's inbox in one batched provider read, the "since you
//! last looked" feed against the previous sync, and its native stacks.
//! Needs me classification joins this file. Stacks are GitHub's native
//! stacks, never inferred from base and head git branches.

use super::{ReviewDesk, PROVIDER};
use crate::review::feed::diff_snapshots;
use crate::review::types::{Inbox, InboxPr, StackListing};
use crate::storage;

impl ReviewDesk {
    /// Every open PR, plus what changed since the previous sync (compared
    /// with the stored snapshot, which this sync replaces).
    pub(crate) async fn sync_inbox(&self) -> Result<Inbox, String> {
        let read = self
            .blocking("Inbox sync failed", |desk| desk.github.inbox())
            .await?;
        let previous: Option<Vec<InboxPr>> =
            storage::read_inbox_snapshot(&self.database, PROVIDER, &read.repo)
                .await?
                .and_then(|payload| serde_json::from_str(&payload).ok());
        let events = previous
            .map(|previous| diff_snapshots(&previous, &read.prs, &read.viewer))
            .unwrap_or_default();
        let payload = serde_json::to_string(&read.prs)
            .map_err(|_| "Could not store the inbox".to_string())?;
        storage::write_inbox_snapshot(&self.database, PROVIDER, &read.repo, &payload).await?;
        Ok(Inbox {
            repo: read.repo,
            viewer: read.viewer,
            prs: read.prs,
            events,
            rate_limit: read.rate_limit,
            complete: read.complete,
            synced_at: crate::util::time::unix_millis(),
        })
    }

    /// The repository's native stacks (bottom PR first), or
    /// `supported: false`.
    pub(crate) async fn stacks(&self) -> Result<StackListing, String> {
        self.blocking("Stack listing failed", |desk| desk.github.stacks())
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{fixture, SLUG, VIEWER};
    use crate::review::types::{InboxPr, PrStack, StackListing, StackPr};

    #[tokio::test]
    async fn the_second_sync_reports_what_changed_since_the_first() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(4, "feature", "src/lib.rs");

        let first = fixture.desk.sync_inbox().await.unwrap();
        assert_eq!((first.repo.as_str(), first.viewer.as_str()), (SLUG, VIEWER));
        assert_eq!(first.prs.len(), 1);
        assert!(first.events.is_empty(), "nothing to compare the first with");

        fixture.github.put_pr(InboxPr {
            head_sha: "f".repeat(40),
            ..pr
        });
        fixture.github.put_pr(InboxPr {
            number: 5,
            author: "mira-k".into(),
            ..InboxPr::default()
        });
        let second = fixture.desk.sync_inbox().await.unwrap();
        let events: Vec<(i64, &str)> = second
            .events
            .iter()
            .map(|event| (event.number, event.kind.as_str()))
            .collect();
        assert_eq!(events, [(4, "pushed"), (5, "opened")]);

        let third = fixture.desk.sync_inbox().await.unwrap();
        assert!(
            third.events.is_empty(),
            "the second sync is the new baseline"
        );
    }

    #[tokio::test]
    async fn stacks_are_the_ones_github_records() {
        let Some(fixture) = fixture().await else {
            return;
        };
        assert!(!fixture.desk.stacks().await.unwrap().supported);
        let stack = PrStack {
            number: 7,
            base_ref: "main".into(),
            open: true,
            prs: [412, 415]
                .map(|number| StackPr {
                    number,
                    state: "open".into(),
                    ..StackPr::default()
                })
                .to_vec(),
        };
        fixture.github.put_stacks(StackListing {
            supported: true,
            stacks: vec![stack.clone()],
        });
        assert_eq!(fixture.desk.stacks().await.unwrap().stacks, [stack]);
    }
}
