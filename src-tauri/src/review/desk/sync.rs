//! Sync: a repository's inbox in one batched provider read, the "since you
//! last looked" feed against the previous sync, and its native stacks.
//! Each sync classifies every PR for the viewer (Needs me, Mine, Review
//! requested); the inbox filters and the sidebar badge read that one
//! classification. Stacks are GitHub's native stacks, never inferred from
//! base and head git branches.

use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::Duration;

use super::{ReviewDesk, PROVIDER};
use crate::git::{objects, run::READ_TIMEOUT, run_git};
use crate::review::feed::diff_snapshots;
use crate::review::types::{
    Inbox, InboxCounts, InboxEvent, InboxPr, NeedsMe, PrTriage, StackListing,
};
use crate::storage::{self, SeenPr};
use crate::util::host::Host;
use crate::util::ttl_cache::TtlCache;

/// `rev-list --count since..head` by repository and range. Both ends are full
/// SHAs, so the count never changes; each inbox sync would otherwise run one
/// `git` per PR the viewer has reviewed.
static COMMIT_COUNTS: LazyLock<TtlCache<(Host, String, String), i64>> =
    LazyLock::new(|| TtlCache::new(Duration::from_secs(30 * 60), 1024));

/// Why `pr` needs `viewer`, if it does. `my_review_head` is the commit of
/// the viewer's latest review, when known.
fn needs_me(pr: &InboxPr, viewer: &str, my_review_head: Option<&str>) -> Option<NeedsMe> {
    if pr.author != viewer {
        let requested = pr.review_requests.iter().any(|login| login == viewer);
        let reviewed_head = my_review_head == Some(pr.head_sha.as_str());
        return (requested && !reviewed_head).then_some(NeedsMe::ReviewRequested);
    }
    let failing = matches!(pr.checks.as_deref(), Some("FAILURE" | "ERROR"));
    if pr.review_decision.as_deref() == Some("CHANGES_REQUESTED") {
        Some(NeedsMe::ChangesRequested)
    } else if failing {
        Some(NeedsMe::ChecksFailing)
    } else if pr.unresolved_threads > 0 {
        Some(NeedsMe::Unresolved)
    } else if !pr.is_draft
        && pr.review_decision.as_deref() == Some("APPROVED")
        && matches!(pr.checks.as_deref(), None | Some("SUCCESS"))
    {
        Some(NeedsMe::ReadyToMerge)
    } else {
        None
    }
}

fn counts(triage: &[PrTriage]) -> InboxCounts {
    let count = |keep: fn(&PrTriage) -> bool| triage.iter().filter(|t| keep(t)).count() as i64;
    InboxCounts {
        needs_me: count(|t| t.needs_me.is_some()),
        mine: count(|t| t.mine),
        review_requested: count(|t| t.review_requested),
        everyone: triage.len() as i64,
    }
}

impl ReviewDesk {
    /// Commits on `head` since `since`, when both are in the clone.
    fn commits_between(&self, since: &str, head: &str) -> Option<i64> {
        let (since, head) = (
            objects::require_sha(since).ok()?,
            objects::require_sha(head).ok()?,
        );
        let range = format!("{since}..{head}");
        let key = (
            self.host.clone(),
            self.host.path_string(self.dir()),
            range.clone(),
        );
        if let Some(count) = COMMIT_COUNTS.get(&key) {
            return Some(count);
        }
        let output = run_git(
            &self.host,
            self.dir(),
            &["rev-list", "--count", &range],
            READ_TIMEOUT,
        )
        .ok()?;
        if !output.ok {
            return None;
        }
        let count = output.stdout.trim().parse().ok()?;
        COMMIT_COUNTS.insert(key, count);
        Some(count)
    }

    /// Classify every PR for `viewer`. Runs git, so call it on a blocking
    /// task.
    fn triage(
        &self,
        prs: &[InboxPr],
        viewer: &str,
        seen: &HashMap<i64, SeenPr>,
        activity: &HashMap<i64, i64>,
    ) -> Vec<PrTriage> {
        prs.iter()
            .map(|pr| {
                let mine_review = pr.reviews.iter().find(|review| review.author == viewer);
                let review_head = mine_review.and_then(|review| review.commit.as_deref());
                let commits_since_my_review =
                    review_head.and_then(|since| self.commits_between(since, &pr.head_sha));
                let mine = pr.author == viewer;
                let requested = pr.review_requests.iter().any(|login| login == viewer);
                let pushed_since = commits_since_my_review.is_some_and(|count| count > 0);
                let muted = seen
                    .get(&pr.number)
                    .and_then(|seen| seen.muted_head.as_deref())
                    == Some(pr.head_sha.as_str());
                PrTriage {
                    number: pr.number,
                    mine,
                    review_requested: !mine && (requested || pushed_since),
                    needs_me: needs_me(pr, viewer, review_head).filter(|_| !muted),
                    my_review: mine_review.map(|review| review.state.clone()),
                    commits_since_my_review,
                    new_activity: activity.get(&pr.number).copied().unwrap_or(0),
                    muted,
                }
            })
            .collect()
    }
}

/// Activity on a PR muted until new commits: anything but a push.
fn muted_activity(event: &InboxEvent, prs: &[InboxPr], seen: &HashMap<i64, SeenPr>) -> bool {
    let Some(muted_head) = seen
        .get(&event.number)
        .and_then(|seen| seen.muted_head.as_deref())
    else {
        return false;
    };
    prs.iter()
        .find(|pr| pr.number == event.number)
        .is_some_and(|pr| pr.head_sha == muted_head)
}

impl ReviewDesk {
    /// Every open PR, plus what changed since the previous sync (compared
    /// with the stored snapshot, which this sync replaces).
    pub(crate) async fn sync_inbox(&self) -> Result<Inbox, String> {
        let read = self
            .blocking("Inbox sync failed", |desk| desk.github.inbox())
            .await?;
        let database = &self.database;
        let previous: Option<Vec<InboxPr>> =
            storage::read_inbox_snapshot(database, PROVIDER, &read.repo)
                .await?
                .and_then(|payload| serde_json::from_str(&payload).ok());
        let events = previous
            .map(|previous| diff_snapshots(&previous, &read.prs, &read.viewer))
            .unwrap_or_default();
        let payload = serde_json::to_string(&read.prs)
            .map_err(|_| "Could not store the inbox".to_string())?;
        storage::write_inbox_snapshot(database, PROVIDER, &read.repo, &payload).await?;

        let seen = storage::read_seen_prs(database, PROVIDER, &read.repo).await?;
        let synced_at = crate::util::time::unix_millis();
        let noticed: Vec<&InboxEvent> = events
            .iter()
            .filter(|event| !muted_activity(event, &read.prs, &seen))
            .collect();
        storage::append_feed(database, PROVIDER, &read.repo, &noticed, synced_at).await?;
        let feed = storage::read_feed(database, PROVIDER, &read.repo).await?;
        let mut activity: HashMap<i64, i64> = HashMap::new();
        for entry in &feed {
            *activity.entry(entry.number).or_default() += 1;
        }

        let (prs, viewer) = (read.prs.clone(), read.viewer.clone());
        let triage = self
            .blocking("Inbox sync failed", move |desk| {
                Ok(desk.triage(&prs, &viewer, &seen, &activity))
            })
            .await?;
        Ok(Inbox {
            repo: read.repo,
            viewer: read.viewer,
            prs: read.prs,
            events,
            rate_limit: read.rate_limit,
            complete: read.complete,
            synced_at,
            counts: counts(&triage),
            triage,
            feed,
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
    use crate::review::types::{
        InboxCounts, InboxPr, InboxReview, NeedsMe, PrStack, PrTriage, StackListing, StackPr,
    };

    fn pr(number: i64, author: &str) -> InboxPr {
        InboxPr {
            number,
            author: author.into(),
            head_sha: format!("{number:0>40}"),
            ..InboxPr::default()
        }
    }

    fn reviewed(author: &str, state: &str) -> Vec<InboxReview> {
        vec![InboxReview {
            author: author.into(),
            state: state.into(),
            commit: None,
        }]
    }

    fn triage_of(inbox: &crate::review::types::Inbox, number: i64) -> &PrTriage {
        inbox
            .triage
            .iter()
            .find(|triage| triage.number == number)
            .expect("every PR has a triage")
    }

    #[tokio::test]
    async fn needs_me_covers_requests_and_my_prs_that_wait_on_me() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let github = &fixture.github;
        github.put_pr(InboxPr {
            review_requests: vec![VIEWER.into()],
            ..pr(1, "jo-r")
        });
        github.put_pr(InboxPr {
            review_decision: Some("CHANGES_REQUESTED".into()),
            reviews: reviewed("mira-k", "CHANGES_REQUESTED"),
            ..pr(2, VIEWER)
        });
        github.put_pr(InboxPr {
            checks: Some("FAILURE".into()),
            ..pr(3, VIEWER)
        });
        github.put_pr(InboxPr {
            unresolved_threads: 2,
            ..pr(4, VIEWER)
        });
        github.put_pr(InboxPr {
            review_decision: Some("APPROVED".into()),
            checks: Some("SUCCESS".into()),
            ..pr(5, VIEWER)
        });
        // Mine, waiting on reviewers: not on me.
        github.put_pr(InboxPr {
            is_draft: true,
            ..pr(6, VIEWER)
        });
        // Approved but still a draft: not ready to merge.
        github.put_pr(InboxPr {
            is_draft: true,
            review_decision: Some("APPROVED".into()),
            ..pr(7, VIEWER)
        });
        // Someone else's, not asking me.
        github.put_pr(pr(8, "tomasz"));

        let inbox = fixture.desk.sync_inbox().await.unwrap();
        let needs: Vec<(i64, Option<NeedsMe>)> = inbox
            .triage
            .iter()
            .map(|triage| (triage.number, triage.needs_me))
            .collect();
        assert_eq!(
            needs,
            [
                (1, Some(NeedsMe::ReviewRequested)),
                (2, Some(NeedsMe::ChangesRequested)),
                (3, Some(NeedsMe::ChecksFailing)),
                (4, Some(NeedsMe::Unresolved)),
                (5, Some(NeedsMe::ReadyToMerge)),
                (6, None),
                (7, None),
                (8, None),
            ]
        );
        assert_eq!(
            inbox.counts,
            InboxCounts {
                needs_me: 5,
                mine: 6,
                review_requested: 1,
                everyone: 8,
            }
        );
        assert_eq!(triage_of(&inbox, 2).my_review, None);
    }

    #[tokio::test]
    async fn a_push_after_my_approval_counts_its_commits_and_a_re_request_needs_me() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let opened = fixture.open_pr(4, "feature", "src/one.rs");
        let approved = InboxPr {
            reviews: vec![InboxReview {
                author: VIEWER.into(),
                state: "APPROVED".into(),
                commit: Some(opened.head_sha.clone()),
            }],
            ..opened.clone()
        };
        fixture.github.put_pr(approved.clone());
        let inbox = fixture.desk.sync_inbox().await.unwrap();
        let triage = triage_of(&inbox, 4);
        assert_eq!(triage.my_review.as_deref(), Some("APPROVED"));
        assert_eq!(triage.commits_since_my_review, Some(0));
        assert!(!triage.review_requested);
        assert_eq!(triage.needs_me, None);

        fixture.push_to_pr(4, "feature", "src/two.rs");
        let head = fixture.push_to_pr(4, "feature", "src/three.rs");
        fixture.github.put_pr(InboxPr {
            head_sha: head,
            review_requests: vec![VIEWER.into()],
            ..approved
        });
        let inbox = fixture.desk.sync_inbox().await.unwrap();
        let triage = triage_of(&inbox, 4);
        assert_eq!(triage.commits_since_my_review, Some(2));
        assert!(triage.review_requested);
        assert_eq!(triage.needs_me, Some(NeedsMe::ReviewRequested));
    }

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
