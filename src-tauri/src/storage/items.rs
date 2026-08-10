//! A local journal of thread items seen on the event stream.
//!
//! Codex's `thread/read` projection does not return every item it streamed:
//! command executions never come back at all, and anything that has not been
//! persisted yet is missing from a thread that is still mid-turn. Without a
//! local copy the work an agent did is only ever visible to whoever was
//! watching the stream live, which makes reopening a thread — or switching away
//! from one and back — lose it.
//!
//! So every completed item is journaled here as it arrives and merged back into
//! `thread/read`, the same way `user_input_answers` are. Codex's own copy wins
//! on a collision; these rows only fill in what its projection drops.

use serde_json::Value;
use turso::{params, Database};

use super::db;
use crate::util::time::unix_secs;

/// One journaled item, in stream order.
pub(crate) struct JournaledItem {
    pub(crate) turn_id: String,
    pub(crate) item_id: String,
    pub(crate) payload: Value,
    /// The id of the item that immediately preceded this one in the turn's
    /// real stream order, captured at the moment it was journaled. `None`
    /// when no earlier item was seen for the turn (it was first) or the row
    /// predates this column.
    pub(crate) after_item_id: Option<String>,
}

pub(crate) async fn record_thread_item(
    database: &Database,
    thread_id: &str,
    turn_id: &str,
    item_id: &str,
    payload: &Value,
    after_item_id: Option<&str>,
) -> Result<(), String> {
    let payload = serde_json::to_string(payload).map_err(|error| error.to_string())?;
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "INSERT INTO thread_items(thread_id, item_id, turn_id, payload, recorded_at, after_item_id)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT(thread_id, item_id) DO UPDATE SET
             turn_id = excluded.turn_id,
             payload = excluded.payload",
        params![
            thread_id,
            item_id,
            turn_id,
            payload,
            unix_secs(),
            after_item_id
        ],
    )
    .await
}

pub(crate) async fn read_thread_items(
    database: &Database,
    thread_id: &str,
) -> Result<Vec<JournaledItem>, String> {
    let connection = db::conn(database)?;
    db::rows(
        &connection,
        "SELECT turn_id, item_id, payload, after_item_id FROM thread_items
         WHERE thread_id = ? ORDER BY recorded_at, rowid",
        (thread_id,),
        |row| {
            let payload = db::text(row, 2)?;
            Ok(JournaledItem {
                turn_id: db::text(row, 0)?,
                item_id: db::text(row, 1)?,
                payload: serde_json::from_str(&payload)
                    .map_err(|error| format!("Could not parse journaled item: {error}"))?,
                after_item_id: db::opt_text(row, 3)?,
            })
        },
    )
    .await
}

/// Note that a turn was seen from its very first item. Only a turn the app
/// watched start to finish can be replayed from the journal alone, so this is
/// what separates "we have the whole thing" from "we have some of it".
pub(crate) async fn record_turn_start(
    database: &Database,
    thread_id: &str,
    turn_id: &str,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "INSERT INTO journaled_turns(thread_id, turn_id, complete) VALUES (?, ?, 0)
         ON CONFLICT(thread_id, turn_id) DO NOTHING",
        (thread_id, turn_id),
    )
    .await
}

/// Close a turn the app watched from the start. The `UPDATE` deliberately does
/// not insert: a turn whose start this process missed (it was resumed
/// mid-flight, or ran under an older build) must not claim full coverage.
pub(crate) async fn mark_turn_complete(
    database: &Database,
    thread_id: &str,
    turn_id: &str,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "UPDATE journaled_turns SET complete = 1 WHERE thread_id = ? AND turn_id = ?",
        (thread_id, turn_id),
    )
    .await
}

/// The turns whose journal is a complete record of what streamed.
pub(crate) async fn read_complete_turns(
    database: &Database,
    thread_id: &str,
) -> Result<Vec<String>, String> {
    let connection = db::conn(database)?;
    db::rows(
        &connection,
        "SELECT turn_id FROM journaled_turns WHERE thread_id = ? AND complete = 1",
        (thread_id,),
        |row| db::text(row, 0),
    )
    .await
}

/// The turns this process watched start but never saw finish — i.e. the ones
/// still running right now.
///
/// Codex's `thread/read` projection reports no turn as in progress, so a thread
/// opened for the first time mid-turn renders as a finished transcript: no
/// typing indicator, and its live work collapsed away. These rows are the only
/// record that the turn is still going. A row left behind by a process that
/// died still reads as running here; the transcript resolves that by demoting a
/// turn whose thread has no live stream to `interrupted`.
pub(crate) async fn read_running_turns(
    database: &Database,
    thread_id: &str,
) -> Result<Vec<String>, String> {
    let connection = db::conn(database)?;
    db::rows(
        &connection,
        "SELECT turn_id FROM journaled_turns WHERE thread_id = ? AND complete = 0",
        (thread_id,),
        |row| db::text(row, 0),
    )
    .await
}

pub(crate) async fn delete_thread_items(
    database: &Database,
    thread_id: &str,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "DELETE FROM thread_items WHERE thread_id = ?",
        (thread_id,),
    )
    .await?;
    db::exec(
        &connection,
        "DELETE FROM journaled_turns WHERE thread_id = ?",
        (thread_id,),
    )
    .await
}

/// Carry the journal over to a forked thread. A fork copies the history it was
/// made from, so its transcript needs the same locally-held items; rows whose
/// turn the fork does not have are dropped when they are merged, not here.
pub(crate) async fn copy_thread_items(
    database: &Database,
    from_thread_id: &str,
    to_thread_id: &str,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "INSERT INTO thread_items(thread_id, item_id, turn_id, payload, recorded_at, after_item_id)
         SELECT ?, item_id, turn_id, payload, recorded_at, after_item_id FROM thread_items
         WHERE thread_id = ?
         ON CONFLICT(thread_id, item_id) DO NOTHING",
        (to_thread_id, from_thread_id),
    )
    .await?;
    db::exec(
        &connection,
        "INSERT INTO journaled_turns(thread_id, turn_id, complete)
         SELECT ?, turn_id, complete FROM journaled_turns WHERE thread_id = ?
         ON CONFLICT(thread_id, turn_id) DO NOTHING",
        (to_thread_id, from_thread_id),
    )
    .await
}

/// Drop journaled items for turns a thread no longer has. Rolling back removes
/// turns from Codex's history; the journal has to follow or the dropped work
/// would be merged back into the next read.
pub(crate) async fn retain_thread_turns(
    database: &Database,
    thread_id: &str,
    keep: &[String],
) -> Result<(), String> {
    let connection = db::conn(database)?;
    if keep.is_empty() {
        return delete_thread_items(database, thread_id).await;
    }
    // Turn ids come from Codex (UUIDs); they are quoted here because turso's
    // parameter binding cannot expand a list.
    let list = keep
        .iter()
        .map(|id| format!("'{}'", id.replace('\'', "''")))
        .collect::<Vec<_>>()
        .join(",");
    db::exec(
        &connection,
        &format!("DELETE FROM thread_items WHERE thread_id = ? AND turn_id NOT IN ({list})"),
        (thread_id,),
    )
    .await?;
    db::exec(
        &connection,
        &format!("DELETE FROM journaled_turns WHERE thread_id = ? AND turn_id NOT IN ({list})"),
        (thread_id,),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    async fn database() -> Database {
        let directory = tempfile::tempdir().unwrap();
        let database = crate::storage::open(directory.path()).await.unwrap();
        // The temp directory only has to outlive `open`; the connection keeps
        // the file alive for the rest of the test.
        std::mem::forget(directory);
        database
    }

    fn item(id: &str) -> Value {
        json!({"type": "commandExecution", "id": id, "command": "cargo test"})
    }

    #[tokio::test]
    async fn records_reads_and_replaces_items() {
        let database = database().await;
        record_thread_item(
            &database,
            "thread-1",
            "turn-1",
            "item_1",
            &item("item_1"),
            None,
        )
        .await
        .unwrap();
        record_thread_item(
            &database,
            "thread-1",
            "turn-1",
            "item_1",
            &json!({"type": "commandExecution", "id": "item_1", "exitCode": 0}),
            None,
        )
        .await
        .unwrap();
        record_thread_item(
            &database,
            "thread-2",
            "turn-9",
            "item_1",
            &item("item_1"),
            None,
        )
        .await
        .unwrap();

        let items = read_thread_items(&database, "thread-1").await.unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].payload["exitCode"], json!(0));
    }

    #[tokio::test]
    async fn round_trips_the_stream_position_anchor() {
        let database = database().await;
        record_thread_item(
            &database,
            "thread-1",
            "turn-1",
            "item_1",
            &item("item_1"),
            None,
        )
        .await
        .unwrap();
        record_thread_item(
            &database,
            "thread-1",
            "turn-1",
            "item_2",
            &item("item_2"),
            Some("item_1"),
        )
        .await
        .unwrap();

        let items = read_thread_items(&database, "thread-1").await.unwrap();
        assert_eq!(items[0].after_item_id, None);
        assert_eq!(items[1].after_item_id.as_deref(), Some("item_1"));
    }

    #[tokio::test]
    async fn only_a_turn_seen_from_its_start_counts_as_complete() {
        let database = database().await;
        record_turn_start(&database, "thread-1", "turn-1")
            .await
            .unwrap();
        mark_turn_complete(&database, "thread-1", "turn-1")
            .await
            .unwrap();
        // Never started as far as this process saw — resumed mid-turn, say.
        mark_turn_complete(&database, "thread-1", "turn-2")
            .await
            .unwrap();
        // Started but interrupted.
        record_turn_start(&database, "thread-1", "turn-3")
            .await
            .unwrap();
        record_turn_start(&database, "thread-2", "turn-4")
            .await
            .unwrap();
        mark_turn_complete(&database, "thread-2", "turn-4")
            .await
            .unwrap();

        let complete = read_complete_turns(&database, "thread-1").await.unwrap();
        assert_eq!(complete, vec!["turn-1".to_string()]);
    }

    #[tokio::test]
    async fn a_turn_that_started_and_never_finished_reads_as_running() {
        let database = database().await;
        record_turn_start(&database, "thread-1", "turn-1")
            .await
            .unwrap();
        mark_turn_complete(&database, "thread-1", "turn-1")
            .await
            .unwrap();
        record_turn_start(&database, "thread-1", "turn-2")
            .await
            .unwrap();
        record_turn_start(&database, "thread-2", "turn-3")
            .await
            .unwrap();

        let running = read_running_turns(&database, "thread-1").await.unwrap();
        assert_eq!(running, vec!["turn-2".to_string()]);

        mark_turn_complete(&database, "thread-1", "turn-2")
            .await
            .unwrap();
        assert!(read_running_turns(&database, "thread-1")
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn a_fork_inherits_the_journal_of_the_thread_it_came_from() {
        let database = database().await;
        record_thread_item(
            &database,
            "thread-1",
            "turn-1",
            "item_1",
            &item("item_1"),
            None,
        )
        .await
        .unwrap();

        copy_thread_items(&database, "thread-1", "fork-1")
            .await
            .unwrap();

        let items = read_thread_items(&database, "fork-1").await.unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].turn_id, "turn-1");
        // The original keeps its own copy.
        assert_eq!(
            read_thread_items(&database, "thread-1")
                .await
                .unwrap()
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn retains_only_the_turns_a_thread_still_has() {
        let database = database().await;
        for (turn, id) in [("turn-1", "item_1"), ("turn-2", "item_2")] {
            record_thread_item(&database, "thread-1", turn, id, &item(id), None)
                .await
                .unwrap();
        }

        retain_thread_turns(&database, "thread-1", &["turn-1".to_string()])
            .await
            .unwrap();

        let items = read_thread_items(&database, "thread-1").await.unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].turn_id, "turn-1");

        retain_thread_turns(&database, "thread-1", &[])
            .await
            .unwrap();
        assert!(read_thread_items(&database, "thread-1")
            .await
            .unwrap()
            .is_empty());
    }
}
