//! Threads that live on a harness other than Codex. Codex owns its thread
//! list; a Claude thread exists only here (its transcript is the journal),
//! so this table is the sidebar's source for them.

use turso::{params, Database};

use super::db;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HarnessThread {
    pub(crate) thread_id: String,
    pub(crate) harness: String,
    pub(crate) cwd: String,
    pub(crate) title: String,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
    pub(crate) archived: bool,
}

fn from_row(row: &turso::Row) -> Result<HarnessThread, String> {
    Ok(HarnessThread {
        thread_id: db::text(row, 0)?,
        harness: db::text(row, 1)?,
        cwd: db::text(row, 2)?,
        title: db::text(row, 3)?,
        created_at: db::int(row, 4)?,
        updated_at: db::int(row, 5)?,
        archived: db::int(row, 6)? != 0,
    })
}

const COLUMNS: &str = "thread_id, harness, cwd, title, created_at, updated_at, archived";

pub(crate) async fn record_harness_thread(
    database: &Database,
    thread_id: &str,
    harness: &str,
    cwd: &str,
    title: &str,
    now: i64,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "INSERT INTO harness_threads(thread_id, harness, cwd, title, created_at, updated_at, archived)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5, 0)
         ON CONFLICT(thread_id) DO UPDATE SET updated_at = excluded.updated_at",
        params![thread_id, harness, cwd, title, now],
    )
    .await
}

/// Which harness a thread belongs to, or `None` for a Codex thread.
pub(crate) async fn thread_harness(
    database: &Database,
    thread_id: &str,
) -> Result<Option<HarnessThread>, String> {
    let connection = db::conn(database)?;
    db::one(
        &connection,
        &format!("SELECT {COLUMNS} FROM harness_threads WHERE thread_id = ?1 LIMIT 1"),
        params![thread_id],
        from_row,
    )
    .await
}

pub(crate) async fn read_harness_threads(
    database: &Database,
    archived: bool,
) -> Result<Vec<HarnessThread>, String> {
    let connection = db::conn(database)?;
    db::rows(
        &connection,
        &format!(
            "SELECT {COLUMNS} FROM harness_threads WHERE archived = ?1 ORDER BY updated_at DESC"
        ),
        params![archived as i64],
        from_row,
    )
    .await
}

pub(crate) async fn touch_harness_thread(
    database: &Database,
    thread_id: &str,
    now: i64,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "UPDATE harness_threads SET updated_at = ?2 WHERE thread_id = ?1",
        params![thread_id, now],
    )
    .await
}

pub(crate) async fn rename_harness_thread(
    database: &Database,
    thread_id: &str,
    title: &str,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "UPDATE harness_threads SET title = ?2 WHERE thread_id = ?1",
        params![thread_id, title],
    )
    .await
}

pub(crate) async fn set_harness_thread_archived(
    database: &Database,
    thread_id: &str,
    archived: bool,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "UPDATE harness_threads SET archived = ?2 WHERE thread_id = ?1",
        params![thread_id, archived as i64],
    )
    .await
}

pub(crate) async fn delete_harness_thread(
    database: &Database,
    thread_id: &str,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "DELETE FROM harness_threads WHERE thread_id = ?1",
        params![thread_id],
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn records_and_reads_back_a_claude_thread() {
        let dir = tempfile::tempdir().unwrap();
        let database = crate::storage::open(dir.path()).await.unwrap();
        record_harness_thread(&database, "t1", "claude", "/repo", "Untitled", 10)
            .await
            .unwrap();
        let found = thread_harness(&database, "t1").await.unwrap().unwrap();
        assert_eq!(found.harness, "claude");
        assert!(thread_harness(&database, "nope").await.unwrap().is_none());
        rename_harness_thread(&database, "t1", "Named")
            .await
            .unwrap();
        touch_harness_thread(&database, "t1", 20).await.unwrap();
        let listed = read_harness_threads(&database, false).await.unwrap();
        assert_eq!(listed[0].title, "Named");
        assert_eq!(listed[0].updated_at, 20);
        set_harness_thread_archived(&database, "t1", true)
            .await
            .unwrap();
        assert!(read_harness_threads(&database, false)
            .await
            .unwrap()
            .is_empty());
        delete_harness_thread(&database, "t1").await.unwrap();
        assert!(read_harness_threads(&database, true)
            .await
            .unwrap()
            .is_empty());
    }
}
