//! What each turn actually ran on.
//!
//! Codex's turn payload does not say which model or reasoning effort produced a
//! reply, so a thread whose model was switched part-way through gives no way to
//! tell which answer came from which. The composer knows the resolved pair at
//! send time; it is recorded here and merged back onto the turns at read time,
//! the same way `thread_items` are.

use turso::{params, Database};

use super::db;

/// The model and effort one turn ran with, as resolved by the composer.
pub(crate) struct TurnSettings {
    pub(crate) turn_id: String,
    pub(crate) model: Option<String>,
    pub(crate) reasoning_effort: Option<String>,
}

pub(crate) async fn record_turn_settings(
    database: &Database,
    thread_id: &str,
    turn_id: &str,
    model: Option<&str>,
    reasoning_effort: Option<&str>,
) -> Result<(), String> {
    if model.is_none() && reasoning_effort.is_none() {
        return Ok(());
    }
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "INSERT INTO turn_settings(thread_id, turn_id, model, reasoning_effort)
         VALUES (?, ?, ?, ?)
         ON CONFLICT(thread_id, turn_id) DO UPDATE SET
             model = excluded.model,
             reasoning_effort = excluded.reasoning_effort",
        params![thread_id, turn_id, model, reasoning_effort],
    )
    .await
}

pub(crate) async fn read_turn_settings(
    database: &Database,
    thread_id: &str,
) -> Result<Vec<TurnSettings>, String> {
    let connection = db::conn(database)?;
    db::rows(
        &connection,
        "SELECT turn_id, model, reasoning_effort FROM turn_settings WHERE thread_id = ?",
        (thread_id,),
        |row| {
            Ok(TurnSettings {
                turn_id: db::text(row, 0)?,
                model: db::opt_text(row, 1)?,
                reasoning_effort: db::opt_text(row, 2)?,
            })
        },
    )
    .await
}

pub(crate) async fn delete_turn_settings(
    database: &Database,
    thread_id: &str,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "DELETE FROM turn_settings WHERE thread_id = ?",
        (thread_id,),
    )
    .await
}

/// Carry the settings over to a forked thread, which inherits the turns they
/// describe.
pub(crate) async fn copy_turn_settings(
    database: &Database,
    from_thread_id: &str,
    to_thread_id: &str,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "INSERT INTO turn_settings(thread_id, turn_id, model, reasoning_effort)
         SELECT ?, turn_id, model, reasoning_effort FROM turn_settings
         WHERE thread_id = ?
         ON CONFLICT(thread_id, turn_id) DO NOTHING",
        (to_thread_id, from_thread_id),
    )
    .await
}

/// Drop settings for turns a rollback removed, so they cannot be merged back.
pub(crate) async fn retain_turn_settings(
    database: &Database,
    thread_id: &str,
    keep: &[String],
) -> Result<(), String> {
    let connection = db::conn(database)?;
    if keep.is_empty() {
        return delete_turn_settings(database, thread_id).await;
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
        &format!("DELETE FROM turn_settings WHERE thread_id = ? AND turn_id NOT IN ({list})"),
        (thread_id,),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn database() -> Database {
        let directory = tempfile::tempdir().unwrap();
        let database = crate::storage::open(directory.path()).await.unwrap();
        // The temp directory only has to outlive `open`; the connection keeps
        // the file alive for the rest of the test.
        std::mem::forget(directory);
        database
    }

    #[tokio::test]
    async fn records_and_replaces_what_a_turn_ran_with() {
        let database = database().await;
        record_turn_settings(
            &database,
            "thread-1",
            "turn-1",
            Some("gpt-5.2"),
            Some("low"),
        )
        .await
        .unwrap();
        record_turn_settings(
            &database,
            "thread-1",
            "turn-1",
            Some("gpt-5.6-terra"),
            Some("high"),
        )
        .await
        .unwrap();
        record_turn_settings(&database, "thread-2", "turn-9", Some("gpt-5.2"), None)
            .await
            .unwrap();

        let settings = read_turn_settings(&database, "thread-1").await.unwrap();
        assert_eq!(settings.len(), 1);
        assert_eq!(settings[0].model.as_deref(), Some("gpt-5.6-terra"));
        assert_eq!(settings[0].reasoning_effort.as_deref(), Some("high"));
    }

    #[tokio::test]
    async fn a_turn_with_nothing_resolved_is_not_recorded() {
        let database = database().await;
        record_turn_settings(&database, "thread-1", "turn-1", None, None)
            .await
            .unwrap();
        assert!(read_turn_settings(&database, "thread-1")
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn a_fork_inherits_the_settings_of_the_thread_it_came_from() {
        let database = database().await;
        record_turn_settings(&database, "thread-1", "turn-1", Some("gpt-5.2"), None)
            .await
            .unwrap();

        copy_turn_settings(&database, "thread-1", "fork-1")
            .await
            .unwrap();

        let settings = read_turn_settings(&database, "fork-1").await.unwrap();
        assert_eq!(settings.len(), 1);
        assert_eq!(settings[0].turn_id, "turn-1");
        assert_eq!(
            read_turn_settings(&database, "thread-1")
                .await
                .unwrap()
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn retains_only_the_turns_a_thread_still_has() {
        let database = database().await;
        for turn in ["turn-1", "turn-2"] {
            record_turn_settings(&database, "thread-1", turn, Some("gpt-5.2"), None)
                .await
                .unwrap();
        }

        retain_turn_settings(&database, "thread-1", &["turn-1".to_string()])
            .await
            .unwrap();
        let settings = read_turn_settings(&database, "thread-1").await.unwrap();
        assert_eq!(settings.len(), 1);
        assert_eq!(settings[0].turn_id, "turn-1");

        retain_turn_settings(&database, "thread-1", &[])
            .await
            .unwrap();
        assert!(read_turn_settings(&database, "thread-1")
            .await
            .unwrap()
            .is_empty());
    }
}
