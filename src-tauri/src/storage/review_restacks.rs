//! A restack stopped at a conflict, per (repository, stack): what every git
//! branch pointed at before it started and the steps left, so Continue and
//! Abort work after a restart. The payload is the desk's own JSON.

use turso::{params, Database};

use super::db;
use crate::util::time::unix_secs;

pub(crate) async fn write_restack(
    database: &Database,
    repo_key: &str,
    stack: i64,
    payload: &str,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "INSERT INTO review_restacks (repo_key, stack_number, payload, updated_at) VALUES (?, ?, ?, ?)
         ON CONFLICT (repo_key, stack_number) DO UPDATE SET payload = excluded.payload, updated_at = excluded.updated_at",
        params![repo_key, stack, payload, unix_secs()],
    )
    .await
}

pub(crate) async fn read_restack(
    database: &Database,
    repo_key: &str,
    stack: i64,
) -> Result<Option<String>, String> {
    let connection = db::conn(database)?;
    db::one(
        &connection,
        "SELECT payload FROM review_restacks WHERE repo_key = ? AND stack_number = ?",
        params![repo_key, stack],
        |row| db::text(row, 0),
    )
    .await
}

pub(crate) async fn delete_restack(
    database: &Database,
    repo_key: &str,
    stack: i64,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "DELETE FROM review_restacks WHERE repo_key = ? AND stack_number = ?",
        params![repo_key, stack],
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_stored_restack_reads_back_until_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let database = crate::storage::open(dir.path()).await.unwrap();
        assert_eq!(read_restack(&database, "/repo", 7).await.unwrap(), None);
        write_restack(&database, "/repo", 7, "{\"a\":1}")
            .await
            .unwrap();
        write_restack(&database, "/repo", 7, "{\"a\":2}")
            .await
            .unwrap();
        assert_eq!(
            read_restack(&database, "/repo", 7)
                .await
                .unwrap()
                .as_deref(),
            Some("{\"a\":2}")
        );
        delete_restack(&database, "/repo", 7).await.unwrap();
        assert_eq!(read_restack(&database, "/repo", 7).await.unwrap(), None);
    }
}
