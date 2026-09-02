//! The project list and pinned threads — the small, hand-curated part of the
//! sidebar. Written as a whole `Store` rather than row-by-row because callers
//! read it, mutate it in memory, and write it back.

use serde::{Deserialize, Serialize};
use turso::{params, Database};

use super::db;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StoredProject {
    pub(crate) path: String,
    #[serde(default)]
    pub(crate) name: Option<String>,
    #[serde(default)]
    pub(crate) pinned: bool,
    #[serde(default)]
    pub(crate) archived: bool,
}

#[derive(Default, Debug, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct Store {
    pub(crate) projects: Vec<StoredProject>,
    pub(crate) pinned_threads: Vec<String>,
    /// Threads the user folded out of the sidebar ("Hide thread"). Unrelated
    /// to `read_hidden_thread_ids`, which covers app-owned threads.
    pub(crate) hidden_threads: Vec<String>,
}

pub(crate) async fn read_store(database: &Database) -> Result<Store, String> {
    let connection = db::conn(database)?;
    let projects = db::rows(
        &connection,
        "SELECT path, name, pinned, archived FROM projects ORDER BY pinned DESC, rowid",
        (),
        |row| {
            Ok(StoredProject {
                path: db::text(row, 0)?,
                name: db::opt_text(row, 1)?,
                pinned: db::flag(row, 2)?,
                archived: db::flag(row, 3)?,
            })
        },
    )
    .await?;
    let pinned_threads = db::rows(
        &connection,
        "SELECT thread_id FROM pinned_threads ORDER BY rowid",
        (),
        |row| db::text(row, 0),
    )
    .await?;
    let hidden_threads = db::rows(
        &connection,
        "SELECT thread_id FROM hidden_threads ORDER BY rowid",
        (),
        |row| db::text(row, 0),
    )
    .await?;
    Ok(Store {
        projects,
        pinned_threads,
        hidden_threads,
    })
}

pub(crate) async fn write_store(database: &Database, store: &Store) -> Result<(), String> {
    let connection = db::conn(database)?;
    let transaction = connection
        .unchecked_transaction()
        .await
        .map_err(db::db_error)?;
    db::exec(&transaction, "DELETE FROM projects", ()).await?;
    db::exec(&transaction, "DELETE FROM pinned_threads", ()).await?;
    db::exec(&transaction, "DELETE FROM hidden_threads", ()).await?;
    for project in &store.projects {
        db::exec(
            &transaction,
            "INSERT INTO projects(path, name, pinned, archived) VALUES (?, ?, ?, ?)",
            params![
                project.path.clone(),
                project.name.clone(),
                i64::from(project.pinned),
                i64::from(project.archived)
            ],
        )
        .await?;
    }
    for thread_id in &store.pinned_threads {
        db::exec(
            &transaction,
            "INSERT INTO pinned_threads(thread_id) VALUES (?)",
            (thread_id.clone(),),
        )
        .await?;
    }
    for thread_id in &store.hidden_threads {
        db::exec(
            &transaction,
            "INSERT INTO hidden_threads(thread_id) VALUES (?)",
            (thread_id.clone(),),
        )
        .await?;
    }
    transaction.commit().await.map_err(db::db_error)
}

/// Remember which repository a temporary worktree was cut from.
///
/// Threads started in a temporary worktree are listed under that repository,
/// so the link has to outlive the worktree itself — removing the throwaway
/// directory must not take its threads out of the sidebar with it.
pub async fn record_temp_worktree(
    database: &Database,
    path: &str,
    parent_path: &str,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "INSERT INTO temp_worktrees(path, parent_path) VALUES (?, ?)
         ON CONFLICT(path) DO UPDATE SET parent_path = excluded.parent_path",
        params![path.to_string(), parent_path.to_string()],
    )
    .await
}

/// Forget a temporary worktree once its branch has been handed off; the
/// directory is gone and its threads move with the branch.
pub async fn remove_temp_worktree(database: &Database, path: &str) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "DELETE FROM temp_worktrees WHERE path = ?",
        params![path.to_string()],
    )
    .await
}

/// Every remembered temporary worktree, as `(worktree path, repository path)`.
pub async fn read_temp_worktrees(database: &Database) -> Result<Vec<(String, String)>, String> {
    let connection = db::conn(database)?;
    db::rows(
        &connection,
        "SELECT path, parent_path FROM temp_worktrees",
        (),
        |row| Ok((db::text(row, 0)?, db::text(row, 1)?)),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::open;

    #[tokio::test]
    async fn round_trips_projects_and_pinned_threads() {
        let directory = tempfile::tempdir().unwrap();
        let database = open(directory.path()).await.unwrap();
        let store = Store {
            projects: vec![StoredProject {
                path: "/tmp/project".into(),
                name: Some("Project".into()),
                pinned: true,
                archived: true,
            }],
            pinned_threads: vec!["thread-1".into()],
            hidden_threads: vec!["thread-2".into()],
        };
        write_store(&database, &store).await.unwrap();
        assert_eq!(read_store(&database).await.unwrap(), store);

        // Writing replaces wholesale rather than accumulating.
        write_store(&database, &Store::default()).await.unwrap();
        assert_eq!(read_store(&database).await.unwrap(), Store::default());
    }

    #[tokio::test]
    async fn remembers_the_repository_a_temporary_worktree_came_from() {
        let directory = tempfile::tempdir().unwrap();
        let database = open(directory.path()).await.unwrap();
        record_temp_worktree(&database, "/tmp/wt/a", "/repo")
            .await
            .unwrap();
        // Re-recording the same worktree corrects the link rather than
        // duplicating it.
        record_temp_worktree(&database, "/tmp/wt/a", "/repo-moved")
            .await
            .unwrap();
        assert_eq!(
            read_temp_worktrees(&database).await.unwrap(),
            vec![("/tmp/wt/a".to_string(), "/repo-moved".to_string())]
        );
    }
}
