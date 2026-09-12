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
    /// Whether this Home's conversations live in the Profile's own database
    /// rather than a database of its own. At most one Claude Home owns it —
    /// the one that existed before Homes could be added — and ownership never
    /// moves, so repointing the Settings default cannot take another Home's
    /// history with it.
    pub shares_profile_db: bool,
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
    // The column is added exactly once. A successful `ALTER` means this
    // database predates the flag, so the Claude Home holding the Profile's
    // conversations is the one registered first; re-running fails with
    // "duplicate column" and skips the backfill, leaving ownership put.
    if db::exec(
        &connection,
        "ALTER TABLE profile_homes
            ADD COLUMN shares_profile_db INTEGER NOT NULL DEFAULT 0",
        (),
    )
    .await
    .is_ok()
    {
        db::exec(
            &connection,
            "UPDATE profile_homes SET shares_profile_db = 1
             WHERE id = (SELECT id FROM profile_homes WHERE harness = ?1
                         ORDER BY rowid LIMIT 1)",
            params![encode(&HarnessKind::Claude)?],
        )
        .await?;
    }
    Ok(())
}

pub(crate) async fn homes(database: &Database) -> Result<Vec<ProfileHome>, String> {
    db::rows(
        &db::conn(database)?,
        "SELECT id, harness, host, config_dir, binary, label, is_default, shares_profile_db
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
                shares_profile_db: db::flag(row, 7)?,
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

/// Repoint a Home. Caller settles and canonicalizes `config_dir` on the Home's
/// Host first, as it does when registering one.
pub(crate) async fn update_home(
    database: &Database,
    id: &str,
    config_dir: &str,
    binary: &str,
    label: &str,
) -> Result<ProfileHome, String> {
    let existing = homes(database).await?;
    let home = existing
        .iter()
        .find(|home| home.id == id)
        .ok_or("Home does not belong to this Profile")?;
    // Caught here rather than by the UNIQUE constraint, whose raw error names
    // the index instead of the directory the reader typed.
    if existing.iter().any(|other| {
        other.id != id
            && other.harness == home.harness
            && other.host == home.host
            && other.config_dir == config_dir
    }) {
        return Err(format!("Another Home already uses {config_dir}"));
    }
    db::exec(
        &db::conn(database)?,
        "UPDATE profile_homes SET config_dir = ?1, binary = ?2, label = ?3 WHERE id = ?4",
        params![
            config_dir.to_string(),
            binary.to_string(),
            label.to_string(),
            id.to_string()
        ],
    )
    .await?;
    homes(database)
        .await?
        .into_iter()
        .find(|home| home.id == id)
        .ok_or_else(|| "Updated Home is missing".into())
}

/// Remove a Home, promoting its oldest sibling when it was the default so each
/// harness on a Host keeps exactly one.
pub(crate) async fn delete_home(database: &Database, id: &str) -> Result<(), String> {
    let existing = homes(database).await?;
    let home = existing
        .iter()
        .find(|home| home.id == id)
        .ok_or("Home does not belong to this Profile")?;
    let heir = existing
        .iter()
        .find(|other| other.id != id && other.harness == home.harness && other.host == home.host)
        .ok_or("A Host needs at least one Home for each harness")?;
    let connection = db::conn(database)?;
    let transaction = connection
        .unchecked_transaction()
        .await
        .map_err(db::db_error)?;
    // Delete before promoting: the `profile_home_default` partial index admits
    // one default per harness and Host, so a second one cannot exist even for
    // the length of a statement.
    db::exec(
        &transaction,
        "DELETE FROM profile_homes WHERE id = ?",
        (id,),
    )
    .await?;
    if home.is_default {
        db::exec(
            &transaction,
            "UPDATE profile_homes SET is_default = 1 WHERE id = ?",
            (heir.id.as_str(),),
        )
        .await?;
    }
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

    #[tokio::test]
    async fn a_home_is_repointed_or_removed_without_colliding() {
        let database = database().await;
        let host = Host::Native;
        let a = register_home(&database, HarnessKind::Claude, &host, "/a", "claude", "A")
            .await
            .unwrap();
        let b = register_home(&database, HarnessKind::Claude, &host, "/b", "claude", "B")
            .await
            .unwrap();
        let updated = update_home(&database, &b.id, "/c", "claude-next", "Renamed")
            .await
            .unwrap();
        assert_eq!(
            (
                updated.config_dir.as_str(),
                updated.binary.as_str(),
                updated.label.as_str()
            ),
            ("/c", "claude-next", "Renamed")
        );
        assert!(update_home(&database, "unknown", "/d", "claude", "D")
            .await
            .is_err());
        // Another Home already sits on `/a`, and the Host and harness match.
        assert!(update_home(&database, &b.id, "/a", "claude", "B")
            .await
            .is_err());
        // A different Host is a different identity, so the same path is free.
        let wsl = register_home(
            &database,
            HarnessKind::Claude,
            &Host::wsl("Ubuntu"),
            "/elsewhere",
            "claude",
            "WSL",
        )
        .await
        .unwrap();
        assert!(update_home(&database, &wsl.id, "/a", "claude", "WSL")
            .await
            .is_ok());
        // `a` is the default, so removing it promotes its oldest sibling and
        // the partial unique index still has exactly one row to admit.
        delete_home(&database, &a.id).await.unwrap();
        let homes = homes(&database).await.unwrap();
        assert!(!homes.iter().any(|home| home.id == a.id));
        let defaults: Vec<_> = homes
            .iter()
            .filter(|home| home.harness == HarnessKind::Claude && home.host == host)
            .filter(|home| home.is_default)
            .map(|home| home.id.clone())
            .collect();
        assert_eq!(defaults, vec![b.id.clone()]);
        // The last Home for a harness on a Host stays put.
        assert!(delete_home(&database, &b.id).await.is_err());
        assert!(delete_home(&database, "unknown").await.is_err());
    }

    #[tokio::test]
    async fn the_first_claude_home_keeps_the_profile_database() {
        let database = turso::Builder::new_local(":memory:").build().await.unwrap();
        // A database written before the flag existed: the table is created
        // without the column, so `initialize` adds it and backfills.
        db::exec(
            &db::conn(&database).unwrap(),
            "CREATE TABLE profile_homes (
                id TEXT PRIMARY KEY, harness TEXT NOT NULL, host TEXT NOT NULL,
                config_dir TEXT NOT NULL, binary TEXT NOT NULL, label TEXT NOT NULL,
                is_default INTEGER NOT NULL DEFAULT 0,
                UNIQUE(harness, host, config_dir))",
            (),
        )
        .await
        .unwrap();
        db::exec(
            &db::conn(&database).unwrap(),
            "INSERT INTO profile_homes(id, harness, host, config_dir, binary, label, is_default)
             VALUES ('old', ?1, ?2, '/old', 'claude', 'Claude Code', 1)",
            params![
                encode(&HarnessKind::Claude).unwrap(),
                encode(&Host::Native).unwrap()
            ],
        )
        .await
        .unwrap();
        initialize(&database).await.unwrap();
        let owner = |homes: &[ProfileHome]| {
            homes
                .iter()
                .filter(|home| home.shares_profile_db)
                .map(|home| home.id.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(owner(&homes(&database).await.unwrap()), vec!["old"]);
        // A Home added afterwards runs its own database, and re-running
        // `initialize` does not hand it the Profile's.
        let new = register_home(
            &database,
            HarnessKind::Claude,
            &Host::Native,
            "/new",
            "claude",
            "New",
        )
        .await
        .unwrap();
        assert!(!new.shares_profile_db);
        initialize(&database).await.unwrap();
        assert_eq!(owner(&homes(&database).await.unwrap()), vec!["old"]);
    }
}
