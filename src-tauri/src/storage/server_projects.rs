//! Which app-server project (`project/*`, Codex ≥0.149) stands for which local
//! sidebar entry.
//!
//! The sidebar stays keyed by path; this table only remembers the server id
//! each entry was mirrored to, so a cached bootstrap can still read a thread's
//! `projectId` back into the right project without asking Codex.

use std::collections::HashMap;
use turso::{params, Database};

use super::db;

/// Every mapping as `server project id → local key` (a project path or a
/// workspace hub path).
pub(crate) async fn read_server_projects(
    database: &Database,
) -> Result<HashMap<String, String>, String> {
    let connection = db::conn(database)?;
    let rows = db::rows(
        &connection,
        "SELECT project_id, local_key FROM server_projects",
        (),
        |row| Ok((db::text(row, 0)?, db::text(row, 1)?)),
    )
    .await?;
    Ok(rows.into_iter().collect())
}

/// Replace the whole mapping with what the server currently reports.
pub(crate) async fn replace_server_projects(
    database: &Database,
    mapping: &HashMap<String, String>,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    let transaction = connection
        .unchecked_transaction()
        .await
        .map_err(db::db_error)?;
    db::exec(&transaction, "DELETE FROM server_projects", ()).await?;
    for (project_id, local_key) in mapping {
        db::exec(
            &transaction,
            "INSERT INTO server_projects(project_id, local_key) VALUES (?, ?)",
            params![project_id.clone(), local_key.clone()],
        )
        .await?;
    }
    transaction.commit().await.map_err(db::db_error)
}

pub(crate) async fn remove_server_project(database: &Database, local_key: &str) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "DELETE FROM server_projects WHERE local_key = ?",
        params![local_key.to_string()],
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::open;

    #[tokio::test]
    async fn round_trips_and_replaces_the_mapping() {
        let directory = tempfile::tempdir().unwrap();
        let database = open(directory.path()).await.unwrap();
        let mapping = HashMap::from([
            ("srv-1".to_string(), "/repo/a".to_string()),
            ("srv-2".to_string(), "/hub/b".to_string()),
        ]);
        replace_server_projects(&database, &mapping).await.unwrap();
        assert_eq!(read_server_projects(&database).await.unwrap(), mapping);

        remove_server_project(&database, "/repo/a").await.unwrap();
        assert_eq!(
            read_server_projects(&database).await.unwrap(),
            HashMap::from([("srv-2".to_string(), "/hub/b".to_string())])
        );
    }
}
