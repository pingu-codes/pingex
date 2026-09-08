//! Durable Home and project registrations inside a local Profile.
//!
//! Frontend conversation references qualify native IDs with their Home key.

use serde::{Deserialize, Serialize};
use turso::{params, Database};

use super::db;
use crate::harness::HarnessKind;
use crate::util::host::Host;

#[derive(Clone, Debug, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProfileHome {
    pub id: String,
    pub harness: HarnessKind,
    pub host: Host,
    pub config_dir: String,
    pub binary: String,
    pub label: String,
    pub is_default: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProfileProject {
    pub id: String,
    pub host: Host,
    pub path: String,
}

fn encode<T: Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string(value).map_err(|error| error.to_string())
}

fn decode<T: serde::de::DeserializeOwned>(value: String) -> Result<T, String> {
    serde_json::from_str(&value).map_err(|error| format!("Invalid profile data: {error}"))
}

pub(crate) async fn initialize(database: &Database) -> Result<(), String> {
    let connection = db::conn(database)?;
    for sql in [
        "CREATE TABLE IF NOT EXISTS profile_homes (
            id TEXT PRIMARY KEY, harness TEXT NOT NULL, host TEXT NOT NULL,
            config_dir TEXT NOT NULL, binary TEXT NOT NULL, label TEXT NOT NULL,
            is_default INTEGER NOT NULL DEFAULT 0,
            UNIQUE(harness, host, config_dir))",
        "CREATE UNIQUE INDEX IF NOT EXISTS profile_home_default
            ON profile_homes(harness, host) WHERE is_default = 1",
        "CREATE TABLE IF NOT EXISTS profile_projects (
            id TEXT PRIMARY KEY, host TEXT NOT NULL, path TEXT NOT NULL,
            UNIQUE(host, path))",
    ] {
        db::exec(&connection, sql, ()).await?;
    }
    Ok(())
}

pub(crate) async fn homes(database: &Database) -> Result<Vec<ProfileHome>, String> {
    db::rows(
        &db::conn(database)?,
        "SELECT id, harness, host, config_dir, binary, label, is_default
         FROM profile_homes ORDER BY rowid",
        (),
        |row| {
            Ok(ProfileHome {
                id: db::text(row, 0)?,
                harness: decode(db::text(row, 1)?)?,
                host: decode(db::text(row, 2)?)?,
                config_dir: db::text(row, 3)?,
                binary: db::text(row, 4)?,
                label: db::text(row, 5)?,
                is_default: db::flag(row, 6)?,
            })
        },
    )
    .await
}

/// Caller settles and canonicalizes the path on its Host before registering it.
/// Re-registering a Home preserves its ID, configuration and default selection.
pub(crate) async fn register_home(
    database: &Database,
    harness: HarnessKind,
    host: &Host,
    config_dir: &str,
    binary: &str,
    label: &str,
) -> Result<ProfileHome, String> {
    let connection = db::conn(database)?;
    let transaction = connection
        .unchecked_transaction()
        .await
        .map_err(db::db_error)?;
    let harness_text = encode(&harness)?;
    let host_text = encode(host)?;
    db::exec(
        &transaction,
        "INSERT INTO profile_homes(id, harness, host, config_dir, binary, label, is_default)
         SELECT ?1, ?2, ?3, ?4, ?5, ?6,
             NOT EXISTS(SELECT 1 FROM profile_homes WHERE harness = ?2 AND host = ?3)
         ON CONFLICT(harness, host, config_dir) DO NOTHING",
        params![
            uuid::Uuid::new_v4().to_string(),
            harness_text,
            host_text,
            config_dir.to_string(),
            binary.to_string(),
            label.to_string()
        ],
    )
    .await?;
    transaction.commit().await.map_err(db::db_error)?;
    homes(database)
        .await?
        .into_iter()
        .find(|home| home.harness == harness && home.host == *host && home.config_dir == config_dir)
        .ok_or_else(|| "Registered Home is missing".into())
}

pub(crate) async fn set_default_home(database: &Database, id: &str) -> Result<(), String> {
    let home = homes(database)
        .await?
        .into_iter()
        .find(|home| home.id == id)
        .ok_or("Home does not belong to this Profile")?;
    let connection = db::conn(database)?;
    let transaction = connection
        .unchecked_transaction()
        .await
        .map_err(db::db_error)?;
    db::exec(
        &transaction,
        "UPDATE profile_homes SET is_default = 0 WHERE harness = ? AND host = ?",
        params![encode(&home.harness)?, encode(&home.host)?],
    )
    .await?;
    db::exec(
        &transaction,
        "UPDATE profile_homes SET is_default = 1 WHERE id = ?",
        (id,),
    )
    .await?;
    transaction.commit().await.map_err(db::db_error)
}

pub(crate) async fn register_project(
    database: &Database,
    host: &Host,
    path: &str,
) -> Result<ProfileProject, String> {
    let connection = db::conn(database)?;
    let host_text = encode(host)?;
    db::exec(
        &connection,
        "INSERT INTO profile_projects(id, host, path) VALUES (?, ?, ?)
         ON CONFLICT(host, path) DO NOTHING",
        params![
            uuid::Uuid::new_v4().to_string(),
            host_text.clone(),
            path.to_string()
        ],
    )
    .await?;
    db::one(
        &connection,
        "SELECT id FROM profile_projects WHERE host = ? AND path = ?",
        params![host_text, path.to_string()],
        |row| {
            Ok(ProfileProject {
                id: db::text(row, 0)?,
                host: host.clone(),
                path: path.into(),
            })
        },
    )
    .await?
    .ok_or_else(|| "Registered project is missing".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn database() -> Database {
        let database = turso::Builder::new_local(":memory:").build().await.unwrap();
        initialize(&database).await.unwrap();
        database
    }

    #[tokio::test]
    async fn identities_are_scoped_and_registration_is_repeatable() {
        let database = database().await;
        let ubuntu = Host::wsl("Ubuntu");
        let debian = Host::wsl("Debian");
        let a = register_home(
            &database,
            HarnessKind::Codex,
            &ubuntu,
            "/home/u/.codex",
            "codex",
            "Ubuntu",
        )
        .await
        .unwrap();
        let b = register_home(
            &database,
            HarnessKind::Codex,
            &debian,
            "/home/u/.codex",
            "codex",
            "Debian",
        )
        .await
        .unwrap();
        assert_ne!(a.id, b.id);
        assert_eq!(
            a.id,
            register_home(
                &database,
                HarnessKind::Codex,
                &ubuntu,
                "/home/u/.codex",
                "ignored",
                "ignored"
            )
            .await
            .unwrap()
            .id
        );
        let pa = register_project(&database, &ubuntu, "/repo").await.unwrap();
        let pb = register_project(&database, &debian, "/repo").await.unwrap();
        assert_ne!(pa.id, pb.id);
    }

    #[tokio::test]
    async fn changing_a_default_keeps_existing_conversations_bound() {
        let database = database().await;
        let host = Host::Native;
        let a = register_home(&database, HarnessKind::Claude, &host, "/a", "claude", "A")
            .await
            .unwrap();
        let b = register_home(&database, HarnessKind::Claude, &host, "/b", "claude", "B")
            .await
            .unwrap();
        let wsl = register_home(
            &database,
            HarnessKind::Claude,
            &Host::wsl("Ubuntu"),
            "/a",
            "claude",
            "WSL",
        )
        .await
        .unwrap();
        set_default_home(&database, &b.id).await.unwrap();
        let homes = homes(&database).await.unwrap();
        assert!(
            !homes
                .iter()
                .find(|home| home.id == a.id)
                .unwrap()
                .is_default
        );
        assert!(
            homes
                .iter()
                .find(|home| home.id == b.id)
                .unwrap()
                .is_default
        );
        assert!(
            homes
                .iter()
                .find(|home| home.id == wsl.id)
                .unwrap()
                .is_default
        );
        assert_eq!(
            homes
                .iter()
                .find(|home| home.id == a.id)
                .unwrap()
                .config_dir,
            "/a"
        );
        assert!(set_default_home(&database, "unknown").await.is_err());
    }
}
