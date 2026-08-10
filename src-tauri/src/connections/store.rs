//! The frontend-only `remote_connections` table.
//!
//! The remote-control protocol has no notion of a user-chosen device name, and
//! a device claimed at pairing time may not appear in the protocol listing for a
//! while. Both gaps are filled by these locally-persisted records.

use turso::{params, Database};

use super::DeviceRecord;
use crate::storage::db;

pub(crate) async fn ensure_table(database: &Database) -> Result<(), String> {
    let connection = db::conn(database)?;
    connection
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS remote_connections (
                 client_id TEXT PRIMARY KEY,
                 platform TEXT,
                 name TEXT,
                 paired_at INTEGER NOT NULL,
                 last_seen INTEGER,
                 scope TEXT
             );",
        )
        .await
        .map_err(db::db_error)
}

pub(crate) async fn read_records(database: &Database) -> Result<Vec<DeviceRecord>, String> {
    let connection = db::conn(database)?;
    let mut rows = connection
        .query(
            "SELECT client_id, platform, name, paired_at, last_seen, scope
             FROM remote_connections ORDER BY COALESCE(last_seen, paired_at) DESC",
            (),
        )
        .await
        .map_err(db::db_error)?;
    let mut records = Vec::new();
    while let Some(row) = rows.next().await.map_err(db::db_error)? {
        records.push(DeviceRecord {
            client_id: row.get(0).map_err(db::db_error)?,
            platform: row.get(1).map_err(db::db_error)?,
            name: row.get(2).map_err(db::db_error)?,
            paired_at: row.get(3).map_err(db::db_error)?,
            last_seen: row.get(4).map_err(db::db_error)?,
            scope: row.get(5).map_err(db::db_error)?,
        });
    }
    Ok(records)
}

/// Upsert a device seen at pairing claim / protocol-list time. Preserves an
/// existing user-chosen name and the earliest `paired_at`; advances
/// `last_seen`.
pub(crate) async fn upsert_seen(
    database: &Database,
    client_id: &str,
    platform: Option<&str>,
    last_seen: Option<i64>,
    scope: Option<&str>,
    now: i64,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    connection
        .execute(
            "INSERT INTO remote_connections(client_id, platform, name, paired_at, last_seen, scope)
             VALUES (?, ?, NULL, ?, ?, ?)
             ON CONFLICT(client_id) DO UPDATE SET
                 platform = COALESCE(excluded.platform, remote_connections.platform),
                 last_seen = MAX(
                     COALESCE(excluded.last_seen, 0),
                     COALESCE(remote_connections.last_seen, 0)
                 ),
                 scope = COALESCE(excluded.scope, remote_connections.scope)",
            params![
                client_id.to_string(),
                platform.map(str::to_string),
                now,
                last_seen,
                scope.map(str::to_string)
            ],
        )
        .await
        .map_err(db::db_error)?;
    Ok(())
}

pub(crate) async fn set_name(
    database: &Database,
    client_id: &str,
    name: Option<&str>,
    now: i64,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    // Renaming a device we have not recorded yet still needs a row so the name
    // survives until the protocol list catches up.
    connection
        .execute(
            "INSERT INTO remote_connections(client_id, platform, name, paired_at, last_seen, scope)
             VALUES (?, NULL, ?, ?, NULL, NULL)
             ON CONFLICT(client_id) DO UPDATE SET name = excluded.name",
            params![client_id.to_string(), name.map(str::to_string), now],
        )
        .await
        .map_err(db::db_error)?;
    Ok(())
}

pub(crate) async fn delete_record(database: &Database, client_id: &str) -> Result<(), String> {
    let connection = db::conn(database)?;
    connection
        .execute(
            "DELETE FROM remote_connections WHERE client_id = ?",
            (client_id.to_string(),),
        )
        .await
        .map_err(db::db_error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::open;

    #[tokio::test]
    async fn store_round_trips_and_rename_preserved_across_reseen() {
        let directory = tempfile::tempdir().unwrap();
        let database = open(directory.path()).await.unwrap();
        ensure_table(&database).await.unwrap();

        upsert_seen(&database, "dev-1", Some("iOS"), Some(100), Some("full"), 10)
            .await
            .unwrap();
        let records = read_records(&database).await.unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].client_id, "dev-1");
        assert_eq!(records[0].last_seen, Some(100));
        assert_eq!(records[0].scope.as_deref(), Some("full"));

        set_name(&database, "dev-1", Some("Kitchen iPad"), 20)
            .await
            .unwrap();
        // A later "seen" upsert must not clobber the user's name and must only
        // advance last_seen forward.
        upsert_seen(&database, "dev-1", Some("iOS"), Some(80), None, 30)
            .await
            .unwrap();
        let records = read_records(&database).await.unwrap();
        assert_eq!(records[0].name.as_deref(), Some("Kitchen iPad"));
        assert_eq!(records[0].last_seen, Some(100));

        delete_record(&database, "dev-1").await.unwrap();
        assert!(read_records(&database).await.unwrap().is_empty());
        // Deleting a missing record is a no-op (idempotent).
        delete_record(&database, "dev-1").await.unwrap();
    }
    #[tokio::test]
    async fn rename_creates_row_when_device_not_yet_recorded() {
        let directory = tempfile::tempdir().unwrap();
        let database = open(directory.path()).await.unwrap();
        ensure_table(&database).await.unwrap();

        set_name(&database, "future", Some("Pending"), 5)
            .await
            .unwrap();
        let records = read_records(&database).await.unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].name.as_deref(), Some("Pending"));
    }
}
