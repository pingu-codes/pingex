//! Copy an existing Home's database into its own local Profile. Copying SQL
//! rows through a read transaction includes committed WAL data. Copying the
//! database file alone would lose it.

use super::{db, schema};
use crate::util::host::Host;
use std::{
    fs,
    path::{Path, PathBuf},
};
use turso::{Builder, Database};

pub(crate) fn profile_root_path(host: &Host, home: &str) -> Result<PathBuf, String> {
    Ok(profile_root_path_in(
        &dirs::data_dir()
            .ok_or("Could not locate local app data")?
            .join("pingex")
            .join("profiles"),
        host,
        home,
    ))
}

pub(crate) fn profile_root_path_in(base: &Path, host: &Host, home: &str) -> PathBuf {
    let key = crate::home_key_for(host, home);
    let id = uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, key.as_bytes());
    base.join(id.to_string()).join("pingex.db")
}

pub(crate) async fn open_profile_root(host: &Host, home: &str) -> Result<Database, String> {
    let target = profile_root_path(host, home)?;
    open_profile_root_at(&target, host, home).await
}

pub(crate) async fn open_profile_root_at(
    target: &Path,
    host: &Host,
    home: &str,
) -> Result<Database, String> {
    let source = super::database_path_on(host, home);
    let local_home = host.to_local(home);
    let source = if source.is_file() {
        source
    } else {
        super::legacy_database_path(&local_home)
    };
    import(&source, target, &local_home).await
}

fn quote_identifier(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

async fn import(source: &Path, target: &Path, local_home: &Path) -> Result<Database, String> {
    fs::create_dir_all(target.parent().ok_or("Profile path has no parent")?)
        .map_err(|error| format!("Could not create Profile directory: {error}"))?;
    let database = Builder::new_local(target.to_str().ok_or("Invalid Profile path")?)
        .build()
        .await
        .map_err(db::db_error)?;
    let connection = db::conn(&database)?;
    db::exec(
        &connection,
        "CREATE TABLE IF NOT EXISTS profile_migration (source TEXT PRIMARY KEY)",
        (),
    )
    .await?;
    let source_key = source.to_string_lossy().into_owned();
    let imported = db::exists(&connection, "SELECT 1 FROM profile_migration", ()).await?;
    if !imported {
        let transaction = connection
            .unchecked_transaction()
            .await
            .map_err(db::db_error)?;
        if source.is_file() {
            let original =
                Builder::new_local(source.to_str().ok_or("Invalid legacy database path")?)
                    .build()
                    .await
                    .map_err(db::db_error)?;
            let reader = db::conn(&original)?;
            let snapshot = reader.unchecked_transaction().await.map_err(db::db_error)?;
            let objects = db::rows(&snapshot,
                "SELECT type, name, sql FROM sqlite_master WHERE sql IS NOT NULL AND name NOT LIKE 'sqlite_%' AND name != 'profile_migration' ORDER BY CASE type WHEN 'table' THEN 0 ELSE 1 END, rowid",
                (), |row| Ok((db::text(row, 0)?, db::text(row, 1)?, db::text(row, 2)?))).await?;
            for (kind, name, sql) in objects {
                db::exec(&transaction, &sql, ()).await?;
                if kind != "table" {
                    continue;
                }
                let name = quote_identifier(&name);
                let mut rows = snapshot
                    .query(&format!("SELECT * FROM {name}"), ())
                    .await
                    .map_err(db::db_error)?;
                let columns = rows.column_names();
                let names = columns
                    .iter()
                    .map(|column| quote_identifier(column))
                    .collect::<Vec<_>>()
                    .join(",");
                let placeholders = vec!["?"; columns.len()].join(",");
                let insert = format!("INSERT INTO {name}({names}) VALUES ({placeholders})");
                while let Some(row) = rows.next().await.map_err(db::db_error)? {
                    let values = (0..columns.len())
                        .map(|index| row.get_value(index))
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(db::db_error)?;
                    db::exec(&transaction, &insert, values).await?;
                }
            }
            snapshot.commit().await.map_err(db::db_error)?;
        }
        db::exec(
            &transaction,
            "INSERT INTO profile_migration(source) VALUES (?)",
            (source_key,),
        )
        .await?;
        transaction.commit().await.map_err(db::db_error)?;
    }
    schema::initialize(&database, local_home).await?;
    super::profiles::initialize(&database).await?;
    Ok(database)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn imports_committed_rows_once_and_leaves_the_original_independent() {
        let directory = tempfile::tempdir().unwrap();
        let legacy_home = directory.path().join("legacy");
        let original = crate::storage::open(&legacy_home).await.unwrap();
        let connection = db::conn(&original).unwrap();
        db::exec(
            &connection,
            "INSERT INTO projects(path, name, pinned) VALUES ('C:\\repo', 'Old name', 1)",
            (),
        )
        .await
        .unwrap();
        db::exec(&connection, "INSERT INTO thread_items(thread_id, item_id, turn_id, payload, recorded_at) VALUES ('thread', 'item', 'turn', '{\"text\":\"kept\"}', 1)", ()).await.unwrap();
        let target = directory.path().join("profile").join("pingex.db");
        let source = crate::storage::database_path(&legacy_home);
        let copied = import(&source, &target, &legacy_home).await.unwrap();
        assert_eq!(
            crate::storage::read_store(&copied).await.unwrap(),
            crate::storage::read_store(&original).await.unwrap()
        );
        assert!(db::exists(
            &db::conn(&copied).unwrap(),
            "SELECT 1 FROM thread_items WHERE thread_id = 'thread' AND item_id = 'item'",
            ()
        )
        .await
        .unwrap());
        db::exec(
            &db::conn(&copied).unwrap(),
            "UPDATE projects SET name = 'Profile name'",
            (),
        )
        .await
        .unwrap();
        drop(copied);
        let reopened = import(&source, &target, &legacy_home).await.unwrap();
        assert_eq!(
            crate::storage::read_store(&reopened)
                .await
                .unwrap()
                .projects[0]
                .name
                .as_deref(),
            Some("Profile name")
        );
        assert_eq!(
            crate::storage::read_store(&original)
                .await
                .unwrap()
                .projects[0]
                .name
                .as_deref(),
            Some("Old name")
        );
    }
}
