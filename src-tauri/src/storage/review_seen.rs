//! Activity seen state for the review desk inbox, per profile: when the
//! viewer last looked at each PR, which PRs are muted until new commits, and
//! the "since you last looked" feed, kept until marked seen.
//!
//! The Seen head ("changes since") is a git ref, not stored here.

use std::collections::HashMap;

use turso::{params, Database};

use super::db;
use crate::review::types::{FeedEntry, InboxEvent};
use crate::util::time::unix_millis;

/// The newest feed entries kept per repository; older ones drop off.
const MAX_FEED: i64 = 200;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SeenPr {
    /// When the viewer last marked the PR's activity seen, Unix
    /// milliseconds: the timeline cursor for "N new since".
    pub(crate) seen_at: Option<i64>,
    /// Muted until the head moves off this commit.
    pub(crate) muted_head: Option<String>,
}

pub(crate) async fn read_seen_prs(
    database: &Database,
    provider: &str,
    repo: &str,
) -> Result<HashMap<i64, SeenPr>, String> {
    let connection = db::conn(database)?;
    let rows = db::rows(
        &connection,
        "SELECT pr_number, seen_at, muted_head FROM review_seen WHERE provider = ? AND repo = ?",
        params![provider, repo],
        |row| {
            Ok((
                db::int(row, 0)?,
                SeenPr {
                    seen_at: db::opt_int(row, 1)?,
                    muted_head: db::opt_text(row, 2)?,
                },
            ))
        },
    )
    .await?;
    Ok(rows.into_iter().collect())
}

/// Append one sync's events, listed newest first, above the feed.
pub(crate) async fn append_feed(
    database: &Database,
    provider: &str,
    repo: &str,
    events: &[&InboxEvent],
    at: i64,
) -> Result<(), String> {
    if events.is_empty() {
        return Ok(());
    }
    let connection = db::conn(database)?;
    let top = db::one(
        &connection,
        "SELECT MAX(ordinal) FROM review_feed WHERE provider = ? AND repo = ?",
        params![provider, repo],
        |row| db::opt_int(row, 0),
    )
    .await?
    .flatten()
    .unwrap_or(0);
    let count = events.len() as i64;
    for (index, event) in events.iter().enumerate() {
        // The first event of the sync gets the highest ordinal.
        let ordinal = top + count - index as i64;
        db::exec(
            &connection,
            "INSERT INTO review_feed(provider, repo, ordinal, pr_number, kind, actor, state, at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                provider,
                repo,
                ordinal,
                event.number,
                event.kind.as_str(),
                event.actor.clone(),
                event.state.clone(),
                at
            ],
        )
        .await?;
    }
    db::exec(
        &connection,
        "DELETE FROM review_feed WHERE provider = ? AND repo = ? AND ordinal <= ?",
        params![provider, repo, top + count - MAX_FEED],
    )
    .await
}

/// The feed, newest first.
pub(crate) async fn read_feed(
    database: &Database,
    provider: &str,
    repo: &str,
) -> Result<Vec<FeedEntry>, String> {
    let connection = db::conn(database)?;
    db::rows(
        &connection,
        "SELECT ordinal, pr_number, kind, actor, state, at FROM review_feed
         WHERE provider = ? AND repo = ? ORDER BY ordinal DESC",
        params![provider, repo],
        |row| {
            Ok(FeedEntry {
                id: db::int(row, 0)?.to_string(),
                number: db::int(row, 1)?,
                kind: db::text(row, 2)?,
                actor: db::opt_text(row, 3)?,
                state: db::opt_text(row, 4)?,
                at: db::int(row, 5)?,
            })
        },
    )
    .await
}

/// Mark PRs `numbers` seen now: their feed entries go.
pub(crate) async fn mark_prs_seen(
    database: &Database,
    provider: &str,
    repo: &str,
    numbers: &[i64],
) -> Result<(), String> {
    let connection = db::conn(database)?;
    let now = unix_millis();
    for &number in numbers {
        db::exec(
            &connection,
            "DELETE FROM review_feed WHERE provider = ? AND repo = ? AND pr_number = ?",
            params![provider, repo, number],
        )
        .await?;
        db::exec(
            &connection,
            "INSERT INTO review_seen(provider, repo, pr_number, seen_at)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(provider, repo, pr_number) DO UPDATE SET seen_at = excluded.seen_at",
            params![provider, repo, number, now],
        )
        .await?;
    }
    Ok(())
}

/// Mute PR `number` until its head moves off `head_sha`.
pub(crate) async fn mute_pr(
    database: &Database,
    provider: &str,
    repo: &str,
    number: i64,
    head_sha: &str,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "INSERT INTO review_seen(provider, repo, pr_number, muted_head)
         VALUES (?, ?, ?, ?)
         ON CONFLICT(provider, repo, pr_number) DO UPDATE SET muted_head = excluded.muted_head",
        params![provider, repo, number, head_sha],
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::open;

    fn event(number: i64, kind: &str) -> InboxEvent {
        InboxEvent {
            number,
            kind: kind.into(),
            actor: None,
            state: None,
        }
    }

    #[tokio::test]
    async fn the_feed_keeps_its_newest_entries_per_repository() {
        let directory = tempfile::tempdir().unwrap();
        let database = open(directory.path()).await.unwrap();
        let first: Vec<InboxEvent> = (0..MAX_FEED).map(|n| event(n, "pushed")).collect();
        let refs: Vec<&InboxEvent> = first.iter().collect();
        append_feed(&database, "github", "o/r", &refs, 1)
            .await
            .unwrap();
        let later = event(900, "opened");
        append_feed(&database, "github", "o/r", &[&later], 2)
            .await
            .unwrap();
        append_feed(&database, "github", "o/other", &[&later], 2)
            .await
            .unwrap();

        let feed = read_feed(&database, "github", "o/r").await.unwrap();
        assert_eq!(feed.len() as i64, MAX_FEED);
        assert_eq!((feed[0].number, feed[0].at), (900, 2));
        assert_eq!(feed[1].number, 0, "within a sync, the order it listed");
        assert_eq!(feed.last().unwrap().number, MAX_FEED - 2, "the oldest went");
    }
}
