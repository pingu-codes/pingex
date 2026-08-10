//! Local review drafts — pending comments for a PR that has not been submitted.
//!
//! Tagged with the head SHA they were written against so the UI can warn when
//! the remote has since moved on and the line anchors may no longer be right.

use serde::Serialize;
use turso::{params, Database};

use super::db;
use crate::util::time::unix_secs;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReviewDraft {
    pub(crate) head_sha: String,
    /// Opaque JSON owned by the frontend (pending comments, chosen event).
    pub(crate) payload: String,
    pub(crate) updated_at: i64,
}

pub(crate) async fn write_review_draft(
    database: &Database,
    provider: &str,
    repo: &str,
    pr_number: i64,
    head_sha: &str,
    payload: &str,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "INSERT INTO review_drafts(provider, repo, pr_number, head_sha, payload, updated_at)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT(provider, repo, pr_number) DO UPDATE SET
             head_sha = excluded.head_sha,
             payload = excluded.payload,
             updated_at = excluded.updated_at",
        params![provider, repo, pr_number, head_sha, payload, unix_secs()],
    )
    .await
}

pub(crate) async fn read_review_draft(
    database: &Database,
    provider: &str,
    repo: &str,
    pr_number: i64,
) -> Result<Option<ReviewDraft>, String> {
    let connection = db::conn(database)?;
    db::one(
        &connection,
        "SELECT head_sha, payload, updated_at FROM review_drafts
         WHERE provider = ? AND repo = ? AND pr_number = ?",
        params![provider, repo, pr_number],
        |row| {
            Ok(ReviewDraft {
                head_sha: db::text(row, 0)?,
                payload: db::text(row, 1)?,
                updated_at: db::int(row, 2)?,
            })
        },
    )
    .await
}

pub(crate) async fn delete_review_draft(
    database: &Database,
    provider: &str,
    repo: &str,
    pr_number: i64,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "DELETE FROM review_drafts WHERE provider = ? AND repo = ? AND pr_number = ?",
        params![provider, repo, pr_number],
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::open;

    #[tokio::test]
    async fn round_trips_and_replaces_review_drafts() {
        let directory = tempfile::tempdir().unwrap();
        let database = open(directory.path()).await.unwrap();
        assert!(read_review_draft(&database, "github", "o/r", 5)
            .await
            .unwrap()
            .is_none());

        write_review_draft(
            &database,
            "github",
            "o/r",
            5,
            "sha1",
            r#"{"event":"comment"}"#,
        )
        .await
        .unwrap();
        let draft = read_review_draft(&database, "github", "o/r", 5)
            .await
            .unwrap()
            .expect("draft");
        assert_eq!(draft.head_sha, "sha1");
        assert_eq!(draft.payload, r#"{"event":"comment"}"#);

        // Re-saving the same PR replaces (not duplicates) the draft.
        write_review_draft(
            &database,
            "github",
            "o/r",
            5,
            "sha2",
            r#"{"event":"approve"}"#,
        )
        .await
        .unwrap();
        let updated = read_review_draft(&database, "github", "o/r", 5)
            .await
            .unwrap()
            .expect("draft");
        assert_eq!(updated.head_sha, "sha2");
        assert_eq!(updated.payload, r#"{"event":"approve"}"#);

        delete_review_draft(&database, "github", "o/r", 5)
            .await
            .unwrap();
        assert!(read_review_draft(&database, "github", "o/r", 5)
            .await
            .unwrap()
            .is_none());
    }
}
