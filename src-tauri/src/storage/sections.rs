//! A cached copy of the app-server's thread sections (`threadSection/*`,
//! Codex ≥0.149) plus whether this Codex has them at all, so the cached
//! bootstrap can group threads without a round trip.

use serde::{Deserialize, Serialize};
use turso::{params, Database};

use super::db;

const SUPPORTED_KEY: &str = "thread_sections_supported";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StoredThreadSection {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) icon: Option<String>,
    pub(crate) color: Option<String>,
}

pub(crate) async fn read_thread_sections(
    database: &Database,
) -> Result<Vec<StoredThreadSection>, String> {
    let connection = db::conn(database)?;
    db::rows(
        &connection,
        "SELECT id, name, icon, color FROM thread_sections ORDER BY ordinal",
        (),
        |row| {
            Ok(StoredThreadSection {
                id: db::text(row, 0)?,
                name: db::text(row, 1)?,
                icon: db::opt_text(row, 2)?,
                color: db::opt_text(row, 3)?,
            })
        },
    )
    .await
}

/// Replace the cached sections with what the server listed, in its order,
/// and record whether the server has the API at all.
pub(crate) async fn replace_thread_sections(
    database: &Database,
    sections: &[StoredThreadSection],
    supported: bool,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    let transaction = connection
        .unchecked_transaction()
        .await
        .map_err(db::db_error)?;
    db::exec(&transaction, "DELETE FROM thread_sections", ()).await?;
    for (ordinal, section) in sections.iter().enumerate() {
        db::exec(
            &transaction,
            "INSERT INTO thread_sections(id, name, icon, color, ordinal) VALUES (?, ?, ?, ?, ?)",
            params![
                section.id.clone(),
                section.name.clone(),
                section.icon.clone(),
                section.color.clone(),
                ordinal as i64
            ],
        )
        .await?;
    }
    db::exec(
        &transaction,
        "INSERT INTO metadata(key, value) VALUES (?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        (SUPPORTED_KEY, if supported { "1" } else { "0" }),
    )
    .await?;
    transaction.commit().await.map_err(db::db_error)
}

/// Whether the last sync found `threadSection/*` on this Codex. Unknown
/// (never synced) reads as unsupported so nothing is offered prematurely.
pub(crate) async fn thread_sections_supported(database: &Database) -> Result<bool, String> {
    let connection = db::conn(database)?;
    Ok(db::one(
        &connection,
        "SELECT value FROM metadata WHERE key = ?",
        (SUPPORTED_KEY,),
        |row| db::text(row, 0),
    )
    .await?
    .is_some_and(|value| value == "1"))
}

/// Move a cached thread summary into `section_id` (or out of any section)
/// so the sidebar re-renders before the next full bootstrap.
pub(crate) async fn set_thread_section(
    database: &Database,
    thread_id: &str,
    section_id: Option<&str>,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "UPDATE thread_summaries SET section_id = ? WHERE thread_id = ?",
        params![section_id.map(str::to_string), thread_id.to_string()],
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::open;

    #[tokio::test]
    async fn caches_sections_in_server_order_and_remembers_support() {
        let directory = tempfile::tempdir().unwrap();
        let database = open(directory.path()).await.unwrap();
        assert!(!thread_sections_supported(&database).await.unwrap());

        let sections = vec![
            StoredThreadSection {
                id: "b".into(),
                name: "Bugs".into(),
                icon: None,
                color: Some("#f00".into()),
            },
            StoredThreadSection {
                id: "a".into(),
                name: "Archive later".into(),
                icon: Some("box".into()),
                color: None,
            },
        ];
        replace_thread_sections(&database, &sections, true).await.unwrap();
        assert_eq!(read_thread_sections(&database).await.unwrap(), sections);
        assert!(thread_sections_supported(&database).await.unwrap());

        replace_thread_sections(&database, &[], false).await.unwrap();
        assert!(read_thread_sections(&database).await.unwrap().is_empty());
        assert!(!thread_sections_supported(&database).await.unwrap());
    }
}
