//! Native side conversations belong to Pingex, never to the main journal.

use serde::{Deserialize, Serialize};
use turso::{params, Database};

use super::db;
use crate::harness::SideRefusal;

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SideExchange {
    pub id: String,
    pub question: String,
    pub response: Option<String>,
    pub outcome: String,
    pub synthetic: bool,
    pub refusal: Option<SideRefusal>,
    pub created_at: i64,
}

pub(crate) async fn create(
    database: &Database,
    parent: &str,
    id: &str,
    title: &str,
) -> Result<(), String> {
    db::exec(&db::conn(database)?,
        "INSERT INTO native_side_conversations(id, parent_thread_id, title, created_at) VALUES (?, ?, ?, ?)",
        params![id, parent, title, crate::util::time::unix_secs()]).await
}

pub(crate) async fn require_owner(
    database: &Database,
    parent: &str,
    id: &str,
) -> Result<(), String> {
    if !db::exists(
        &db::conn(database)?,
        "SELECT 1 FROM native_side_conversations WHERE id = ? AND parent_thread_id = ?",
        params![id, parent],
    )
    .await?
    {
        return Err("Side conversation does not belong to this thread".into());
    }
    Ok(())
}

pub(crate) async fn begin(
    database: &Database,
    id: &str,
    request: &str,
    question: &str,
) -> Result<(), String> {
    if !db::exists(&db::conn(database)?,
        "INSERT INTO native_side_exchanges(id, conversation_id, question, outcome, created_at) SELECT ?, id, ?, 'pending', ? FROM native_side_conversations WHERE id = ? RETURNING id",
        params![request, question, crate::util::time::unix_secs(), id]).await? {
        return Err("Side conversation no longer exists".into());
    }
    Ok(())
}

pub(crate) async fn read(database: &Database, id: &str) -> Result<Vec<SideExchange>, String> {
    db::rows(&db::conn(database)?,
        "SELECT id, question, response, outcome, synthetic, refusal, created_at FROM native_side_exchanges WHERE conversation_id = ? ORDER BY rowid",
        (id,), |row| Ok(SideExchange {
            id: db::text(row, 0)?, question: db::text(row, 1)?, response: db::opt_text(row, 2)?,
            outcome: db::text(row, 3)?, synthetic: db::flag(row, 4)?,
            refusal: db::opt_text(row, 5)?.map(|text| serde_json::from_str(&text).map_err(|e| e.to_string())).transpose()?,
            created_at: db::int(row, 6)?,
        })).await
}

/// Conditional update: a late answer can never replace a cancellation.
pub(crate) async fn finish(
    database: &Database,
    id: &str,
    request: &str,
    outcome: &str,
    response: Option<&str>,
    synthetic: bool,
    refusal: Option<&SideRefusal>,
) -> Result<(), String> {
    db::exec(&db::conn(database)?,
        "UPDATE native_side_exchanges SET outcome = ?, response = ?, synthetic = ?, refusal = ? WHERE conversation_id = ? AND id = ? AND outcome = 'pending'",
        params![outcome, response, synthetic as i64, refusal.map(serde_json::to_string).transpose().map_err(|e| e.to_string())?, id, request]).await
}

pub(crate) async fn delete(database: &Database, id: &str) -> Result<(), String> {
    let connection = db::conn(database)?;
    let transaction = connection
        .unchecked_transaction()
        .await
        .map_err(db::db_error)?;
    db::exec(
        &transaction,
        "DELETE FROM native_side_exchanges WHERE conversation_id = ?",
        (id,),
    )
    .await?;
    db::exec(
        &transaction,
        "DELETE FROM native_side_conversations WHERE id = ?",
        (id,),
    )
    .await?;
    transaction.commit().await.map_err(db::db_error)
}

pub(crate) fn history(exchanges: &[SideExchange]) -> Vec<SideExchange> {
    let mut history: Vec<_> = exchanges
        .iter()
        .rev()
        .filter(|e| e.outcome == "completed" && !e.synthetic && e.response.is_some())
        .take(20)
        .cloned()
        .collect();
    history.reverse();
    history
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn persists_history_and_cancel_wins_over_late_answer() {
        let dir = tempfile::tempdir().unwrap();
        let database = super::super::open(dir.path()).await.unwrap();
        create(&database, "parent", "side", "Question")
            .await
            .unwrap();
        assert!(require_owner(&database, "other", "side").await.is_err());
        begin(&database, "side", "one", "Why?").await.unwrap();
        assert!(begin(&database, "side", "two", "Concurrent?")
            .await
            .is_err());
        finish(&database, "side", "one", "interrupted", None, false, None)
            .await
            .unwrap();
        finish(
            &database,
            "side",
            "one",
            "completed",
            Some("Late"),
            false,
            None,
        )
        .await
        .unwrap();
        begin(&database, "side", "two", "Again?").await.unwrap();
        finish(
            &database,
            "side",
            "two",
            "completed",
            Some("Answer"),
            false,
            None,
        )
        .await
        .unwrap();
        drop(database);
        let database = super::super::open(dir.path()).await.unwrap();
        let exchanges = read(&database, "side").await.unwrap();
        assert_eq!(exchanges[0].outcome, "interrupted");
        assert_eq!(history(&exchanges), vec![exchanges[1].clone()]);
        delete(&database, "side").await.unwrap();
        assert!(read(&database, "side").await.unwrap().is_empty());
        assert!(require_owner(&database, "parent", "side").await.is_err());
    }

    #[tokio::test]
    async fn restart_recovers_pending_and_deleted_conversations_reject_new_exchanges() {
        let dir = tempfile::tempdir().unwrap();
        let database = super::super::open(dir.path()).await.unwrap();
        create(&database, "parent", "side", "Question")
            .await
            .unwrap();
        begin(&database, "side", "one", "Why?").await.unwrap();
        drop(database);
        let database = super::super::open(dir.path()).await.unwrap();
        assert_eq!(
            read(&database, "side").await.unwrap()[0].outcome,
            "interrupted"
        );
        let (_, deleted) = tokio::join!(
            begin(&database, "side", "two", "Again?"),
            delete(&database, "side")
        );
        deleted.unwrap();
        assert!(read(&database, "side").await.unwrap().is_empty());
        assert!(begin(&database, "side", "three", "Too late?")
            .await
            .is_err());
    }

    #[test]
    fn sends_only_twenty_successful_exchanges_in_order() {
        let exchanges: Vec<_> = (0..25)
            .map(|i| SideExchange {
                id: i.to_string(),
                question: i.to_string(),
                response: Some("answer".into()),
                outcome: "completed".into(),
                synthetic: i == 24,
                refusal: None,
                created_at: i,
            })
            .collect();
        let history = history(&exchanges);
        assert_eq!(history.len(), 20);
        assert_eq!(history[0].question, "4");
        assert_eq!(history[19].question, "23");
    }
}
