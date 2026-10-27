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
    pub(crate) permission_mode: Option<String>,
    pub(crate) plan_exit_mode: Option<String>,
    /// The harness's own id for the session, where the harness picks it
    /// (ACP's `session/new`). `None` when the thread id is the session id,
    /// as for Claude, or before the first turn.
    pub(crate) native_session_id: Option<String>,
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
        permission_mode: db::opt_text(row, 7)?,
        plan_exit_mode: db::opt_text(row, 8)?,
        native_session_id: db::opt_text(row, 9)?,
    })
}

const COLUMNS: &str = "thread_id, harness, cwd, title, created_at, updated_at, archived, \
     permission_mode, plan_exit_mode, native_session_id";

pub(crate) async fn set_harness_permission_mode(
    database: &Database,
    thread: &str,
    mode: &str,
    exit_mode: Option<&str>,
) -> Result<(), String> {
    db::exec(
        &db::conn(database)?,
        "UPDATE harness_threads SET permission_mode = ?, plan_exit_mode = COALESCE(?, plan_exit_mode) WHERE thread_id = ?",
        params![mode, exit_mode, thread],
    )
    .await
}

pub(crate) async fn set_native_session_id(
    database: &Database,
    thread: &str,
    session_id: &str,
) -> Result<(), String> {
    db::exec(
        &db::conn(database)?,
        "UPDATE harness_threads SET native_session_id = ? WHERE thread_id = ?",
        params![session_id, thread],
    )
    .await
}

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
        set_harness_permission_mode(&database, "t1", "plan", Some("bypassPermissions"))
            .await
            .unwrap();
        assert_eq!(
            thread_harness(&database, "t1")
                .await
                .unwrap()
                .unwrap()
                .permission_mode
                .as_deref(),
            Some("plan")
        );
        assert!(thread_harness(&database, "nope").await.unwrap().is_none());
        let saved = thread_harness(&database, "t1").await.unwrap().unwrap();
        let modes = crate::claude::modes::ClaudeModeSettings::restore(
            saved.permission_mode.as_deref(),
            saved.plan_exit_mode.as_deref(),
        )
        .unwrap();
        assert_eq!(modes.mode.as_str(), "plan");
        assert_eq!(modes.exit_plan_mode.as_str(), "bypassPermissions");
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
