//! Thread start tips: every branch tip of each repository a thread works in,
//! recorded before its first turn there, so the Changes page can show what
//! moved since ("This thread").
//!
//! One row per `(thread, root, refname)`. The `HEAD` row also carries the
//! branch checked out and the paths that were already dirty. A root with any
//! row counts as recorded and is never recorded again for that thread.

use turso::{params, Database};

use super::db;
use crate::util::time::unix_secs;

pub(crate) const HEAD_REF: &str = "HEAD";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TipSnapshot {
    pub(crate) root: String,
    /// `None` when HEAD was unborn.
    pub(crate) head_sha: Option<String>,
    pub(crate) head_branch: Option<String>,
    /// Paths with uncommitted changes when the snapshot was taken.
    pub(crate) dirty_paths: Vec<String>,
    /// `(refs/heads/<name>, sha)`.
    pub(crate) branches: Vec<(String, String)>,
    pub(crate) taken_at: i64,
}

/// The roots already recorded for `thread_id`.
pub(crate) async fn recorded_snapshot_roots(
    database: &Database,
    thread_id: &str,
) -> Result<Vec<String>, String> {
    let connection = db::conn(database)?;
    db::rows(
        &connection,
        "SELECT root FROM thread_tip_snapshots WHERE thread_id = ? AND refname = ?",
        params![thread_id, HEAD_REF],
        |row| db::text(row, 0),
    )
    .await
}

/// Record `snapshot` unless its root already has rows for the thread.
pub(crate) async fn write_tip_snapshot(
    database: &Database,
    thread_id: &str,
    snapshot: &TipSnapshot,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    let known = db::exists(
        &connection,
        "SELECT 1 FROM thread_tip_snapshots WHERE thread_id = ? AND root = ?",
        params![thread_id, snapshot.root.as_str()],
    )
    .await?;
    if known {
        return Ok(());
    }
    let taken_at = if snapshot.taken_at > 0 {
        snapshot.taken_at
    } else {
        unix_secs()
    };
    let dirty = serde_json::to_string(&snapshot.dirty_paths).map_err(|error| error.to_string())?;
    db::exec(
        &connection,
        "INSERT OR IGNORE INTO thread_tip_snapshots
             (thread_id, root, refname, sha, head_branch, dirty_paths, taken_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
        params![
            thread_id,
            snapshot.root.as_str(),
            HEAD_REF,
            snapshot.head_sha.as_deref(),
            snapshot.head_branch.as_deref(),
            dirty,
            taken_at
        ],
    )
    .await?;
    for (refname, sha) in &snapshot.branches {
        db::exec(
            &connection,
            "INSERT OR IGNORE INTO thread_tip_snapshots
                 (thread_id, root, refname, sha, head_branch, dirty_paths, taken_at)
             VALUES (?, ?, ?, ?, NULL, NULL, ?)",
            params![
                thread_id,
                snapshot.root.as_str(),
                refname.as_str(),
                sha.as_str(),
                taken_at
            ],
        )
        .await?;
    }
    Ok(())
}

/// The snapshot of `root` for `thread_id`, if one was taken.
pub(crate) async fn read_tip_snapshot(
    database: &Database,
    thread_id: &str,
    root: &str,
) -> Result<Option<TipSnapshot>, String> {
    let connection = db::conn(database)?;
    let rows = db::rows(
        &connection,
        "SELECT refname, sha, head_branch, dirty_paths, taken_at
         FROM thread_tip_snapshots WHERE thread_id = ? AND root = ?
         ORDER BY refname",
        params![thread_id, root],
        |row| {
            Ok((
                db::text(row, 0)?,
                db::opt_text(row, 1)?,
                db::opt_text(row, 2)?,
                db::opt_text(row, 3)?,
                db::int(row, 4)?,
            ))
        },
    )
    .await?;
    if rows.is_empty() {
        return Ok(None);
    }
    let mut snapshot = TipSnapshot {
        root: root.to_string(),
        ..TipSnapshot::default()
    };
    for (refname, sha, head_branch, dirty, taken_at) in rows {
        if refname == HEAD_REF {
            snapshot.head_sha = sha;
            snapshot.head_branch = head_branch;
            snapshot.dirty_paths = dirty
                .and_then(|text| serde_json::from_str(&text).ok())
                .unwrap_or_default();
            snapshot.taken_at = taken_at;
        } else if let Some(sha) = sha {
            snapshot.branches.push((refname, sha));
        }
    }
    Ok(Some(snapshot))
}

pub(crate) async fn delete_tip_snapshots(
    database: &Database,
    thread_id: &str,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "DELETE FROM thread_tip_snapshots WHERE thread_id = ?",
        params![thread_id],
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(root: &str, head: &str) -> TipSnapshot {
        TipSnapshot {
            root: root.into(),
            head_sha: Some(head.into()),
            head_branch: Some("feat".into()),
            dirty_paths: vec!["a.txt".into()],
            branches: vec![
                ("refs/heads/feat".into(), head.into()),
                ("refs/heads/main".into(), "m".into()),
            ],
            taken_at: 10,
        }
    }

    #[tokio::test]
    async fn a_root_is_recorded_once_and_read_back_whole() {
        let dir = tempfile::tempdir().unwrap();
        let database = crate::storage::open(dir.path()).await.unwrap();
        write_tip_snapshot(&database, "t1", &snapshot("/r", "a"))
            .await
            .unwrap();
        // A later snapshot of the same root changes nothing.
        write_tip_snapshot(&database, "t1", &snapshot("/r", "b"))
            .await
            .unwrap();
        write_tip_snapshot(&database, "t1", &snapshot("/other", "c"))
            .await
            .unwrap();
        let read = read_tip_snapshot(&database, "t1", "/r")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(read, snapshot("/r", "a"));
        let mut roots = recorded_snapshot_roots(&database, "t1").await.unwrap();
        roots.sort();
        assert_eq!(roots, vec!["/other".to_string(), "/r".to_string()]);
        assert_eq!(
            read_tip_snapshot(&database, "t2", "/r").await.unwrap(),
            None
        );

        delete_tip_snapshots(&database, "t1").await.unwrap();
        assert!(recorded_snapshot_roots(&database, "t1")
            .await
            .unwrap()
            .is_empty());
    }
}
