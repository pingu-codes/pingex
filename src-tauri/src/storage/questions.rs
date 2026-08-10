//! Two kinds of question Pingex tracks itself.
//!
//! `request_user_input` answers are persisted because the Codex `thread/read`
//! projection has no item type for them — without a local row an answered
//! question would vanish from the transcript on reload, and an *unanswered* one
//! would be lost entirely when the app-server that asked it dies.
//!
//! Side questions are separate throwaway threads spawned from a parent thread,
//! tracked so the UI can offer them back under the thread they came from.

use serde::Serialize;
use serde_json::Value;
use turso::{params, Database};

use super::db;
use crate::util::time::unix_secs;

/// A `request_user_input` question and, once given, its answer. `answered` is
/// false while the question is still open.
pub(crate) struct UserInputAnswer {
    pub(crate) turn_id: String,
    pub(crate) item_id: String,
    pub(crate) payload: Value,
    pub(crate) answered: bool,
    /// The id of the item that immediately preceded this question in the
    /// turn's real stream order, captured when the question was first asked.
    pub(crate) after_item_id: Option<String>,
}

/// Record a question the moment Codex asks it. Never overwrites an existing
/// row: the answer is the authoritative version once it exists.
pub(crate) async fn add_pending_user_input(
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
        "INSERT INTO user_input_answers(item_id, thread_id, turn_id, payload, created_at, answered_at, after_item_id)
         VALUES (?, ?, ?, ?, ?, NULL, ?)
         ON CONFLICT(item_id) DO NOTHING",
        params![item_id, thread_id, turn_id, payload, unix_secs(), after_item_id],
    )
    .await
}

/// Thread ids holding a question that was never answered — the app-server that
/// asked it is gone, so these are only recoverable as a fresh turn.
pub(crate) async fn list_threads_with_unanswered_user_inputs(
    database: &Database,
) -> Result<Vec<String>, String> {
    let connection = db::conn(database)?;
    db::rows(
        &connection,
        "SELECT DISTINCT thread_id FROM user_input_answers WHERE answered_at IS NULL",
        (),
        |row| db::text(row, 0),
    )
    .await
}

pub(crate) async fn add_user_input_answer(
    database: &Database,
    thread_id: &str,
    turn_id: &str,
    item_id: &str,
    payload: &Value,
) -> Result<(), String> {
    let payload = serde_json::to_string(payload).map_err(|error| error.to_string())?;
    let now = unix_secs();
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "INSERT INTO user_input_answers(item_id, thread_id, turn_id, payload, created_at, answered_at, after_item_id)
         VALUES (?, ?, ?, ?, ?, ?, NULL)
         ON CONFLICT(item_id) DO UPDATE SET
             payload = excluded.payload,
             answered_at = excluded.answered_at",
        params![item_id, thread_id, turn_id, payload, now, now],
    )
    .await
}

pub(crate) async fn read_user_input_answers(
    database: &Database,
    thread_id: &str,
) -> Result<Vec<UserInputAnswer>, String> {
    let connection = db::conn(database)?;
    db::rows(
        &connection,
        "SELECT turn_id, item_id, payload, answered_at IS NOT NULL, after_item_id
         FROM user_input_answers WHERE thread_id = ? ORDER BY created_at",
        (thread_id,),
        |row| {
            let payload = db::text(row, 2)?;
            Ok(UserInputAnswer {
                turn_id: db::text(row, 0)?,
                item_id: db::text(row, 1)?,
                payload: serde_json::from_str(&payload)
                    .map_err(|error| format!("Could not parse stored answer: {error}"))?,
                answered: db::flag(row, 3)?,
                after_item_id: db::opt_text(row, 4)?,
            })
        },
    )
    .await
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SideQuestion {
    pub(crate) side_thread_id: String,
    pub(crate) parent_thread_id: String,
    pub(crate) title: String,
    pub(crate) created_at: i64,
}

pub(crate) async fn add_side_question(
    database: &Database,
    side_question: &SideQuestion,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "INSERT INTO side_questions(side_thread_id, parent_thread_id, title, created_at)
         VALUES (?, ?, ?, ?)
         ON CONFLICT(side_thread_id) DO UPDATE SET
             title = excluded.title",
        params![
            side_question.side_thread_id.clone(),
            side_question.parent_thread_id.clone(),
            side_question.title.clone(),
            side_question.created_at
        ],
    )
    .await
}

pub(crate) async fn read_side_questions(database: &Database) -> Result<Vec<SideQuestion>, String> {
    let connection = db::conn(database)?;
    db::rows(
        &connection,
        "SELECT side_thread_id, parent_thread_id, title, created_at
         FROM side_questions ORDER BY created_at DESC",
        (),
        |row| {
            Ok(SideQuestion {
                side_thread_id: db::text(row, 0)?,
                parent_thread_id: db::text(row, 1)?,
                title: db::text(row, 2)?,
                created_at: db::int(row, 3)?,
            })
        },
    )
    .await
}

pub(crate) async fn delete_side_question(
    database: &Database,
    side_thread_id: &str,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "DELETE FROM side_questions WHERE side_thread_id = ?",
        (side_thread_id,),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::open;
    use serde_json::json;

    #[tokio::test]
    async fn stores_and_deletes_side_questions() {
        let directory = tempfile::tempdir().unwrap();
        let database = open(directory.path()).await.unwrap();
        let side_question = SideQuestion {
            side_thread_id: "side-1".into(),
            parent_thread_id: "parent-1".into(),
            title: "What about tests?".into(),
            created_at: 10,
        };
        add_side_question(&database, &side_question).await.unwrap();
        add_side_question(
            &database,
            &SideQuestion {
                side_thread_id: "side-2".into(),
                parent_thread_id: "parent-1".into(),
                title: "Second".into(),
                created_at: 20,
            },
        )
        .await
        .unwrap();

        let listed = read_side_questions(&database).await.unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].side_thread_id, "side-2");
        assert_eq!(listed[1], side_question);

        delete_side_question(&database, "side-1").await.unwrap();
        assert_eq!(read_side_questions(&database).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn an_unanswered_question_stays_pending_until_answered() {
        let directory = tempfile::tempdir().unwrap();
        let database = open(directory.path()).await.unwrap();
        add_pending_user_input(
            &database,
            "t1",
            "turn-1",
            "item_1",
            &json!({"q": "which?"}),
            Some("item_0"),
        )
        .await
        .unwrap();
        assert_eq!(
            list_threads_with_unanswered_user_inputs(&database)
                .await
                .unwrap(),
            vec!["t1".to_string()]
        );
        let pending = read_user_input_answers(&database, "t1").await.unwrap();
        assert_eq!(pending.len(), 1);
        assert!(!pending[0].answered);

        // Re-recording the same question must not clobber it.
        add_pending_user_input(
            &database,
            "t1",
            "turn-1",
            "item_1",
            &json!({"q": "changed"}),
            None,
        )
        .await
        .unwrap();
        assert_eq!(
            read_user_input_answers(&database, "t1").await.unwrap()[0].payload,
            json!({"q": "which?"})
        );

        add_user_input_answer(
            &database,
            "t1",
            "turn-1",
            "item_1",
            &json!({"a": "this one"}),
        )
        .await
        .unwrap();
        let answered = read_user_input_answers(&database, "t1").await.unwrap();
        assert!(answered[0].answered);
        assert_eq!(answered[0].payload, json!({"a": "this one"}));
        // The anchor captured when the question was first asked survives the
        // answer overwriting the row.
        assert_eq!(answered[0].after_item_id.as_deref(), Some("item_0"));
        assert!(list_threads_with_unanswered_user_inputs(&database)
            .await
            .unwrap()
            .is_empty());
    }
}
