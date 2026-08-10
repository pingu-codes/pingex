//! Thin conveniences over the `turso` API.
//!
//! Every call into turso returns its own error type, and every one of them is
//! reported to the user the same way. Without these helpers each query costs
//! three or four `.map_err(db_error)?` calls plus a hand-rolled row loop, which
//! buries the SQL. Nothing here hides a decision — it only removes repetition.

use turso::{Connection, Database, IntoParams, Row};

/// The single conversion from a turso error to the message the frontend shows.
pub(crate) fn db_error(error: impl std::fmt::Display) -> String {
    format!("Pingex database error: {error}")
}

/// Open a connection to the frontend database.
pub(crate) fn conn(database: &Database) -> Result<Connection, String> {
    database.connect().map_err(db_error)
}

/// How long a statement keeps trying to get past whoever is writing.
///
/// Every caller opens its own connection, so statements started in the same
/// instant — three agents spawning at once, a turn being journaled while a run
/// is recorded — race each other, and the losers come back "database is
/// locked". Nothing here retried them, so they were simply lost: the callers
/// that cannot report a failure (`let _ = update_agent_run(..)`, the journal)
/// dropped the write on the floor, and the ones that can turned a lock race
/// into a real error. This is the `busy_timeout` SQLite would apply for us if
/// turso exposed one. Individual statements take well under a millisecond, so
/// the budget is only ever spent waiting behind a long transaction.
///
/// Reads need this as much as writes do, and are the likelier casualty: a
/// single agent's journal is a steady stream of writes, and it is the parent
/// reading the thread list beside it that loses the race.
const BUSY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const MAX_BACKOFF_MS: u64 = 50;

/// Whether an error means "somebody else is writing", as opposed to something
/// retrying cannot fix.
fn is_contention(error: &turso::Error) -> bool {
    matches!(error, turso::Error::Busy(_) | turso::Error::BusySnapshot(_))
}

/// How long to wait before attempt `attempt`, growing to `MAX_BACKOFF_MS`.
///
/// Jittered, and that is not decoration: writers that collide once are by
/// definition in step, and a fixed schedule keeps them in step so they collide
/// again on every retry.
fn backoff(attempt: u32) -> std::time::Duration {
    let ceiling = MAX_BACKOFF_MS.min(1 << attempt.min(6));
    let jitter = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.subsec_nanos() as u64)
        .unwrap_or(0);
    std::time::Duration::from_millis(1 + jitter % ceiling)
}

/// Keep running `attempt` until it stops losing to another connection.
///
/// The closure is handed the parameters each time because both `execute` and
/// `query` consume them, and it must therefore be retryable: every caller here
/// runs a single statement, so a retry re-runs exactly that statement.
async fn retrying<T, F, Fut>(params: impl IntoParams, mut attempt: F) -> Result<T, String>
where
    F: FnMut(turso::params::Params) -> Fut,
    Fut: std::future::Future<Output = Result<T, turso::Error>>,
{
    // Materialised once so the same values can be re-bound on a retry.
    let params = params.into_params().map_err(db_error)?;
    let started = std::time::Instant::now();
    let mut tries = 0;
    loop {
        match attempt(params.clone()).await {
            Ok(value) => return Ok(value),
            Err(error) if is_contention(&error) && started.elapsed() < BUSY_TIMEOUT => {
                tokio::time::sleep(backoff(tries)).await;
                tries += 1;
            }
            Err(error) => return Err(db_error(error)),
        }
    }
}

/// Run a statement, discarding the affected-row count.
pub(crate) async fn exec(
    connection: &Connection,
    sql: &str,
    params: impl IntoParams,
) -> Result<(), String> {
    retrying(params, |params| connection.execute(sql, params)).await?;
    Ok(())
}

/// Run a query and map every row.
pub(crate) async fn rows<T>(
    connection: &Connection,
    sql: &str,
    params: impl IntoParams,
    mut map: impl FnMut(&Row) -> Result<T, String>,
) -> Result<Vec<T>, String> {
    // Only opening the cursor contends; draining it cannot be replayed from
    // here, so a failure mid-scan is reported rather than restarted.
    let mut cursor = retrying(params, |params| connection.query(sql, params)).await?;
    let mut mapped = Vec::new();
    while let Some(row) = cursor.next().await.map_err(db_error)? {
        mapped.push(map(&row)?);
    }
    Ok(mapped)
}

/// Run a query and map only its first row, if any. Later rows are ignored, so
/// callers that care should say `LIMIT 1` in the SQL.
pub(crate) async fn one<T>(
    connection: &Connection,
    sql: &str,
    params: impl IntoParams,
    map: impl FnOnce(&Row) -> Result<T, String>,
) -> Result<Option<T>, String> {
    let mut cursor = retrying(params, |params| connection.query(sql, params)).await?;
    match cursor.next().await.map_err(db_error)? {
        Some(row) => Ok(Some(map(&row)?)),
        None => Ok(None),
    }
}

/// Whether a query matched anything, without decoding the row.
pub(crate) async fn exists(
    connection: &Connection,
    sql: &str,
    params: impl IntoParams,
) -> Result<bool, String> {
    let mut cursor = retrying(params, |params| connection.query(sql, params)).await?;
    Ok(cursor.next().await.map_err(db_error)?.is_some())
}

/// Escape LIKE wildcards so a user's query is matched literally. Every query
/// built with this must pair it with an explicit `ESCAPE '\'` clause.
pub(crate) fn escape_like(query: &str) -> String {
    query
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

// --- Column accessors -------------------------------------------------------
//
// Typed rather than generic: turso's `FromValue` trait is not re-exported, and
// every column in this schema is TEXT or INTEGER anyway. Naming the type at the
// call site also documents the column.

/// A `TEXT NOT NULL` column.
pub(crate) fn text(row: &Row, index: usize) -> Result<String, String> {
    row.get(index).map_err(db_error)
}

/// A nullable `TEXT` column.
pub(crate) fn opt_text(row: &Row, index: usize) -> Result<Option<String>, String> {
    row.get(index).map_err(db_error)
}

/// An `INTEGER NOT NULL` column.
pub(crate) fn int(row: &Row, index: usize) -> Result<i64, String> {
    row.get(index).map_err(db_error)
}

/// A nullable `INTEGER` column.
pub(crate) fn opt_int(row: &Row, index: usize) -> Result<Option<i64>, String> {
    row.get(index).map_err(db_error)
}

/// An `INTEGER` column holding a boolean flag, stored as 0 or 1.
pub(crate) fn flag(row: &Row, index: usize) -> Result<bool, String> {
    Ok(row.get::<i64>(index).map_err(db_error)? != 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use turso::Builder;

    async fn memory() -> Database {
        Builder::new_local(":memory:")
            .build()
            .await
            .expect("in-memory database")
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn concurrent_writers_all_land() {
        // Every storage call opens its own connection, so writes started in the
        // same instant contend for the file's single writer. The losers used to
        // fail, and the callers that cannot report a failure — a run recording
        // the thread it just started, say — dropped the write on the floor.
        let directory = tempfile::tempdir().unwrap();
        let database = Builder::new_local(directory.path().join("t.db").to_str().unwrap())
            .build()
            .await
            .expect("database");
        exec(
            &conn(&database).unwrap(),
            "CREATE TABLE t (id INTEGER PRIMARY KEY)",
            (),
        )
        .await
        .unwrap();

        let writes: Vec<_> = (0..64)
            .map(|id| {
                let database = database.clone();
                tokio::spawn(async move {
                    exec(
                        &conn(&database).unwrap(),
                        "INSERT INTO t(id) VALUES (?)",
                        (id as i64,),
                    )
                    .await
                })
            })
            .collect();
        // Reads race the same writer slot, and are the likelier casualty: a
        // subagent journaling its work is a steady stream of writes, and it is
        // the transcript being read beside it that loses.
        let reads: Vec<_> = (0..16)
            .map(|_| {
                let database = database.clone();
                tokio::spawn(async move {
                    rows(&conn(&database).unwrap(), "SELECT id FROM t", (), |row| {
                        int(row, 0)
                    })
                    .await
                })
            })
            .collect();
        for write in writes {
            write.await.unwrap().expect("write survives contention");
        }
        for read in reads {
            read.await.unwrap().expect("read survives contention");
        }

        let stored = rows(&conn(&database).unwrap(), "SELECT id FROM t", (), |row| {
            int(row, 0)
        })
        .await
        .unwrap();
        assert_eq!(stored.len(), 64);
    }

    #[tokio::test]
    async fn round_trips_rows_and_column_types() {
        let database = memory().await;
        let connection = conn(&database).unwrap();
        exec(
            &connection,
            "CREATE TABLE t (name TEXT NOT NULL, note TEXT, count INTEGER NOT NULL, on_off INTEGER NOT NULL)",
            (),
        )
        .await
        .unwrap();
        exec(
            &connection,
            "INSERT INTO t VALUES (?, ?, ?, ?), (?, ?, ?, ?)",
            (
                "a",
                None::<String>,
                1_i64,
                1_i64,
                "b",
                Some("note"),
                2_i64,
                0_i64,
            ),
        )
        .await
        .unwrap();

        let all = rows(
            &connection,
            "SELECT name, note, count, on_off FROM t ORDER BY name",
            (),
            |row| {
                Ok((
                    text(row, 0)?,
                    opt_text(row, 1)?,
                    int(row, 2)?,
                    flag(row, 3)?,
                ))
            },
        )
        .await
        .unwrap();
        assert_eq!(
            all,
            vec![
                ("a".to_string(), None, 1, true),
                ("b".to_string(), Some("note".to_string()), 2, false),
            ]
        );
    }

    #[tokio::test]
    async fn one_returns_first_row_or_none() {
        let database = memory().await;
        let connection = conn(&database).unwrap();
        exec(&connection, "CREATE TABLE t (name TEXT NOT NULL)", ())
            .await
            .unwrap();

        assert_eq!(
            one(&connection, "SELECT name FROM t", (), |row| text(row, 0))
                .await
                .unwrap(),
            None
        );
        assert!(!exists(&connection, "SELECT 1 FROM t", ()).await.unwrap());

        exec(
            &connection,
            "INSERT INTO t VALUES ('first'), ('second')",
            (),
        )
        .await
        .unwrap();
        assert_eq!(
            one(&connection, "SELECT name FROM t ORDER BY name", (), |row| {
                text(row, 0)
            })
            .await
            .unwrap(),
            Some("first".to_string())
        );
        assert!(exists(&connection, "SELECT 1 FROM t", ()).await.unwrap());
    }

    #[test]
    fn escapes_like_wildcards() {
        assert_eq!(escape_like("a_b%c\\d"), "a\\_b\\%c\\\\d");
    }

    #[tokio::test]
    async fn a_mapping_failure_propagates() {
        let database = memory().await;
        let connection = conn(&database).unwrap();
        exec(&connection, "CREATE TABLE t (name TEXT NOT NULL)", ())
            .await
            .unwrap();
        exec(&connection, "INSERT INTO t VALUES ('not a number')", ())
            .await
            .unwrap();
        // Asking for the wrong type surfaces as a database error, not a panic.
        let result = rows(&connection, "SELECT name FROM t", (), |row| int(row, 0)).await;
        assert!(result.is_err());
    }
}
