//! Locally cached thread data: the sidebar summaries and the full `thread/read`
//! payloads.
//!
//! Both are caches of app-server state, kept so the app renders instantly on
//! launch and stays readable while Codex is unavailable. A detail row is keyed
//! by the summary's `updated_at`, so a stale cache simply misses and is refetched.

use serde_json::Value;
use turso::{params, Database};

use super::db;

const SUMMARY_COLUMNS: &str = "thread_id, cwd, title, updated_at, status,
     parent_thread_id, agent_nickname, agent_role";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StoredThreadSummary {
    pub(crate) id: String,
    pub(crate) cwd: String,
    pub(crate) title: String,
    pub(crate) updated_at: i64,
    pub(crate) status: String,
    pub(crate) parent_thread_id: Option<String>,
    pub(crate) agent_nickname: Option<String>,
    pub(crate) agent_role: Option<String>,
}

fn summary_from_row(row: &turso::Row) -> Result<StoredThreadSummary, String> {
    Ok(StoredThreadSummary {
        id: db::text(row, 0)?,
        cwd: db::text(row, 1)?,
        title: db::text(row, 2)?,
        updated_at: db::int(row, 3)?,
        status: db::text(row, 4)?,
        parent_thread_id: db::opt_text(row, 5)?,
        agent_nickname: db::opt_text(row, 6)?,
        agent_role: db::opt_text(row, 7)?,
    })
}

pub(crate) async fn replace_thread_summaries(
    database: &Database,
    summaries: &[StoredThreadSummary],
) -> Result<(), String> {
    let connection = db::conn(database)?;
    let transaction = connection
        .unchecked_transaction()
        .await
        .map_err(db::db_error)?;
    db::exec(&transaction, "DELETE FROM thread_summaries", ()).await?;
    for summary in summaries {
        db::exec(
            &transaction,
            "INSERT INTO thread_summaries(
                thread_id, cwd, title, updated_at, status,
                parent_thread_id, agent_nickname, agent_role
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                summary.id.clone(),
                summary.cwd.clone(),
                summary.title.clone(),
                summary.updated_at,
                summary.status.clone(),
                summary.parent_thread_id.clone(),
                summary.agent_nickname.clone(),
                summary.agent_role.clone()
            ],
        )
        .await?;
    }
    transaction.commit().await.map_err(db::db_error)
}

pub(crate) async fn read_thread_summaries(
    database: &Database,
) -> Result<Vec<StoredThreadSummary>, String> {
    let connection = db::conn(database)?;
    db::rows(
        &connection,
        &format!("SELECT {SUMMARY_COLUMNS} FROM thread_summaries ORDER BY updated_at DESC"),
        (),
        summary_from_row,
    )
    .await
}

/// Case-insensitive title search over locally-cached thread summaries whose
/// `cwd` lives under `project_path`. Windowed by `offset`/`limit` for cursor
/// paging. One extra row is fetched so the caller can report `has_more`.
pub(crate) async fn search_thread_summaries(
    database: &Database,
    project_path: &str,
    query: &str,
    limit: usize,
    offset: usize,
) -> Result<Vec<StoredThreadSummary>, String> {
    let connection = db::conn(database)?;
    let pattern = format!("%{}%", db::escape_like(query));
    let cwd_prefix = format!("{}%", db::escape_like(project_path));
    db::rows(
        &connection,
        &format!(
            "SELECT {SUMMARY_COLUMNS}
             FROM thread_summaries
             WHERE cwd LIKE ? ESCAPE '\\' AND title LIKE ? ESCAPE '\\'
             ORDER BY updated_at DESC
             LIMIT ? OFFSET ?"
        ),
        params![cwd_prefix, pattern, (limit + 1) as i64, offset as i64],
        summary_from_row,
    )
    .await
}

pub(crate) async fn rename_thread_summary(
    database: &Database,
    thread_id: &str,
    title: &str,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "UPDATE thread_summaries SET title = ? WHERE thread_id = ?",
        (title, thread_id),
    )
    .await
}

/// Who last set a thread's name: `"user"` (an explicit rename) or `"auto"` (the
/// generated title). Kept in its own table because `replace_thread_summaries`
/// rewrites every summary row from app-server data on each bootstrap, which
/// would drop a column stored alongside them.
pub(crate) async fn read_thread_name_source(
    database: &Database,
    thread_id: &str,
) -> Result<Option<String>, String> {
    let connection = db::conn(database)?;
    db::one(
        &connection,
        "SELECT source FROM thread_name_sources WHERE thread_id = ?",
        (thread_id,),
        |row| db::text(row, 0),
    )
    .await
}

pub(crate) async fn write_thread_name_source(
    database: &Database,
    thread_id: &str,
    source: &str,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "INSERT INTO thread_name_sources(thread_id, source) VALUES (?, ?)
         ON CONFLICT(thread_id) DO UPDATE SET source = excluded.source",
        (thread_id, source),
    )
    .await
}

/// Threads that belong under something else rather than in a project listing:
/// side questions, and the threads app-owned subagents run in.
///
/// Both are ordinary Codex threads in an ordinary cwd, so every path that lists
/// threads has to exclude them explicitly or they surface as if the user had
/// started them.
pub(crate) async fn read_hidden_thread_ids(
    database: &Database,
) -> Result<std::collections::HashSet<String>, String> {
    let connection = db::conn(database)?;
    let ids = db::rows(
        &connection,
        "SELECT side_thread_id FROM side_questions
         UNION
         SELECT child_thread_id FROM agent_runs WHERE child_thread_id IS NOT NULL",
        (),
        |row| db::text(row, 0),
    )
    .await?;
    Ok(ids.into_iter().collect())
}

pub(crate) async fn delete_thread_summary(
    database: &Database,
    thread_id: &str,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "DELETE FROM thread_name_sources WHERE thread_id = ?",
        (thread_id,),
    )
    .await?;
    db::exec(
        &connection,
        "DELETE FROM thread_summaries WHERE thread_id = ?",
        (thread_id,),
    )
    .await
}

/// The cached `updated_at` for a thread, used as the cache key for its detail.
pub(crate) async fn thread_updated_at(
    database: &Database,
    thread_id: &str,
) -> Result<Option<i64>, String> {
    let connection = db::conn(database)?;
    db::one(
        &connection,
        "SELECT updated_at FROM thread_summaries WHERE thread_id = ?",
        (thread_id,),
        |row| db::int(row, 0),
    )
    .await
}

/// The cached full thread, but only if it was written against
/// `source_updated_at`. A mismatch reads as a miss so the caller refetches.
pub(crate) async fn read_thread_detail(
    database: &Database,
    thread_id: &str,
    source_updated_at: i64,
) -> Result<Option<Value>, String> {
    let connection = db::conn(database)?;
    let payload = db::one(
        &connection,
        "SELECT payload FROM thread_details
         WHERE thread_id = ? AND source_updated_at = ?",
        (thread_id, source_updated_at),
        |row| db::text(row, 0),
    )
    .await?;
    let Some(payload) = payload else {
        return Ok(None);
    };
    serde_json::from_str(&payload)
        .map(Some)
        .map_err(|error| format!("Could not parse cached thread {thread_id}: {error}"))
}

pub(crate) async fn write_thread_detail(
    database: &Database,
    thread_id: &str,
    source_updated_at: i64,
    detail: &Value,
) -> Result<(), String> {
    let payload = serde_json::to_string(detail).map_err(|error| error.to_string())?;
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "INSERT INTO thread_details(thread_id, source_updated_at, payload)
         VALUES (?, ?, ?)
         ON CONFLICT(thread_id) DO UPDATE SET
             source_updated_at = excluded.source_updated_at,
             payload = excluded.payload",
        (thread_id, source_updated_at, payload),
    )
    .await
}

pub(crate) async fn invalidate_thread_detail(
    database: &Database,
    thread_id: &str,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "DELETE FROM thread_details WHERE thread_id = ?",
        (thread_id,),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::open;

    fn summary(id: &str, cwd: &str, title: &str, updated_at: i64) -> StoredThreadSummary {
        StoredThreadSummary {
            id: id.into(),
            cwd: cwd.into(),
            title: title.into(),
            updated_at,
            status: "idle".into(),
            parent_thread_id: None,
            agent_nickname: None,
            agent_role: None,
        }
    }

    #[tokio::test]
    async fn round_trips_summaries_and_the_detail_cache() {
        let directory = tempfile::tempdir().unwrap();
        let database = open(directory.path()).await.unwrap();
        let stored = StoredThreadSummary {
            parent_thread_id: Some("parent-1".into()),
            agent_nickname: Some("Scout".into()),
            agent_role: Some("researcher".into()),
            ..summary("thread-1", "/tmp/project", "Title", 42)
        };
        replace_thread_summaries(&database, std::slice::from_ref(&stored))
            .await
            .unwrap();
        assert_eq!(
            read_thread_summaries(&database).await.unwrap(),
            vec![stored]
        );

        rename_thread_summary(&database, "thread-1", "Renamed")
            .await
            .unwrap();
        assert_eq!(
            read_thread_summaries(&database).await.unwrap()[0].title,
            "Renamed"
        );
        assert_eq!(
            thread_updated_at(&database, "thread-1").await.unwrap(),
            Some(42)
        );

        let detail = serde_json::json!({"id": "thread-1", "turns": []});
        write_thread_detail(&database, "thread-1", 42, &detail)
            .await
            .unwrap();
        assert_eq!(
            read_thread_detail(&database, "thread-1", 42).await.unwrap(),
            Some(detail)
        );
        // A different source timestamp is a cache miss, not a stale hit.
        assert!(read_thread_detail(&database, "thread-1", 43)
            .await
            .unwrap()
            .is_none());

        invalidate_thread_detail(&database, "thread-1")
            .await
            .unwrap();
        assert!(read_thread_detail(&database, "thread-1", 42)
            .await
            .unwrap()
            .is_none());

        delete_thread_summary(&database, "thread-1").await.unwrap();
        assert!(read_thread_summaries(&database).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn tracks_who_named_a_thread_across_summary_refreshes() {
        let directory = tempfile::tempdir().unwrap();
        let database = open(directory.path()).await.unwrap();
        let stored = summary("thread-1", "/tmp/project", "Title", 42);
        replace_thread_summaries(&database, std::slice::from_ref(&stored))
            .await
            .unwrap();

        assert_eq!(
            read_thread_name_source(&database, "thread-1")
                .await
                .unwrap(),
            None
        );
        write_thread_name_source(&database, "thread-1", "auto")
            .await
            .unwrap();
        write_thread_name_source(&database, "thread-1", "user")
            .await
            .unwrap();
        // Bootstrap rewrites every summary row; provenance must survive it.
        replace_thread_summaries(&database, std::slice::from_ref(&stored))
            .await
            .unwrap();
        assert_eq!(
            read_thread_name_source(&database, "thread-1")
                .await
                .unwrap(),
            Some("user".into())
        );

        delete_thread_summary(&database, "thread-1").await.unwrap();
        assert_eq!(
            read_thread_name_source(&database, "thread-1")
                .await
                .unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn searches_thread_summary_titles_within_project() {
        let directory = tempfile::tempdir().unwrap();
        let database = open(directory.path()).await.unwrap();
        replace_thread_summaries(
            &database,
            &[
                summary("t1", "/proj/a", "Fix login bug", 30),
                summary("t2", "/other", "Login redesign", 20),
            ],
        )
        .await
        .unwrap();
        let hits = search_thread_summaries(&database, "/proj", "login", 10, 0)
            .await
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "t1");
    }

    #[tokio::test]
    async fn hidden_threads_cover_both_side_questions_and_agent_runs() {
        let directory = tempfile::tempdir().unwrap();
        let database = open(directory.path()).await.unwrap();
        crate::storage::add_side_question(
            &database,
            &crate::storage::SideQuestion {
                side_thread_id: "side-1".into(),
                parent_thread_id: "parent-1".into(),
                title: "Why?".into(),
                created_at: 1,
            },
        )
        .await
        .unwrap();
        crate::storage::record_agent_run(
            &database,
            &crate::storage::AgentRunRow {
                run_id: "agt_1".into(),
                parent_thread_id: "parent-1".into(),
                parent_turn_id: "turn-1".into(),
                call_id: None,
                child_thread_id: Some("agent-thread-1".into()),
                name: "probe".into(),
                prompt: "go".into(),
                cwd: "/proj".into(),
                model: None,
                reasoning_effort: None,
                status: crate::storage::STATUS_RUNNING.into(),
                result: None,
                error: None,
                created_at: 1,
                finished_at: None,
            },
        )
        .await
        .unwrap();

        let hidden = read_hidden_thread_ids(&database).await.unwrap();
        assert!(hidden.contains("side-1"));
        assert!(hidden.contains("agent-thread-1"));
        // The threads that spawned them are ordinary threads and stay listed.
        assert!(!hidden.contains("parent-1"));
    }
}
