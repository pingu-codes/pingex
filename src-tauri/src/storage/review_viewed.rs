//! Files the reviewer marked viewed on a PR, with each file's blob id at the
//! time, so the desk can tell which marks still hold after a push.

use turso::{params, Database};

use super::db;
use crate::util::time::unix_secs;

/// Every viewed mark on PR `pr_number`, as `(path, blob_sha)`.
pub(crate) async fn read_viewed_files(
    database: &Database,
    provider: &str,
    repo: &str,
    pr_number: i64,
) -> Result<Vec<(String, String)>, String> {
    let connection = db::conn(database)?;
    db::rows(
        &connection,
        "SELECT path, blob_sha FROM review_viewed_files
         WHERE provider = ? AND repo = ? AND pr_number = ?
         ORDER BY path",
        params![provider, repo, pr_number],
        |row| Ok((db::text(row, 0)?, db::text(row, 1)?)),
    )
    .await
}

/// Mark `(path, blob_sha)` files viewed, replacing older marks on the same
/// paths.
pub(crate) async fn mark_viewed_files(
    database: &Database,
    provider: &str,
    repo: &str,
    pr_number: i64,
    files: &[(String, String)],
) -> Result<(), String> {
    let connection = db::conn(database)?;
    let now = unix_secs();
    for (path, blob_sha) in files {
        db::exec(
            &connection,
            "INSERT INTO review_viewed_files(provider, repo, pr_number, path, blob_sha, updated_at)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(provider, repo, pr_number, path) DO UPDATE SET
                 blob_sha = excluded.blob_sha,
                 updated_at = excluded.updated_at",
            params![
                provider,
                repo,
                pr_number,
                path.as_str(),
                blob_sha.as_str(),
                now
            ],
        )
        .await?;
    }
    Ok(())
}

pub(crate) async fn unmark_viewed_files(
    database: &Database,
    provider: &str,
    repo: &str,
    pr_number: i64,
    paths: &[String],
) -> Result<(), String> {
    let connection = db::conn(database)?;
    for path in paths {
        db::exec(
            &connection,
            "DELETE FROM review_viewed_files
             WHERE provider = ? AND repo = ? AND pr_number = ? AND path = ?",
            params![provider, repo, pr_number, path.as_str()],
        )
        .await?;
    }
    Ok(())
}
