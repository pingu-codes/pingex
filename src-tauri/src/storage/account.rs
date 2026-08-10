//! A cached copy of the signed-in account, so the sidebar can render a plan
//! badge before the app-server has answered.

use turso::Database;

use super::db;

const ACCOUNT_CACHE_KEY: &str = "account_cache";

/// Store the account JSON, or clear it when signed out.
pub(crate) async fn write_account_cache(
    database: &Database,
    account_json: Option<&str>,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    match account_json {
        Some(account_json) => {
            db::exec(
                &connection,
                "INSERT INTO metadata(key, value) VALUES (?, ?)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                (ACCOUNT_CACHE_KEY, account_json),
            )
            .await
        }
        None => {
            db::exec(
                &connection,
                "DELETE FROM metadata WHERE key = ?",
                (ACCOUNT_CACHE_KEY,),
            )
            .await
        }
    }
}

pub(crate) async fn read_account_cache(database: &Database) -> Result<Option<String>, String> {
    let connection = db::conn(database)?;
    db::one(
        &connection,
        "SELECT value FROM metadata WHERE key = ?",
        (ACCOUNT_CACHE_KEY,),
        |row| db::text(row, 0),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::open;

    #[tokio::test]
    async fn round_trips_and_clears_the_account_cache() {
        let directory = tempfile::tempdir().unwrap();
        let database = open(directory.path()).await.unwrap();
        assert_eq!(read_account_cache(&database).await.unwrap(), None);

        write_account_cache(&database, Some(r#"{"label":"me"}"#))
            .await
            .unwrap();
        assert_eq!(
            read_account_cache(&database).await.unwrap().as_deref(),
            Some(r#"{"label":"me"}"#)
        );

        write_account_cache(&database, None).await.unwrap();
        assert_eq!(read_account_cache(&database).await.unwrap(), None);
    }
}
