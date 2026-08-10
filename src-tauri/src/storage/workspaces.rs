//! Virtual projects that stitch several real repositories together, and the
//! threads assigned to them.
//!
//! A workspace's `hub_path` is a writable directory owned by Pingex holding one
//! link per member; the member paths themselves remain the authoritative
//! locations the sandbox may edit.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use turso::{params, Connection, Database};

use super::db;

const MEMBER_COLUMNS: &str =
    "workspace_id, source_path, effective_path, alias, isolated, branch, ordinal";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StoredWorkspace {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) hub_path: String,
    pub(crate) archived: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StoredWorkspaceMember {
    pub(crate) workspace_id: String,
    /// The project selected in the picker. Kept even when an isolated
    /// worktree is the effective root, so the UI can make that distinction.
    pub(crate) source_path: String,
    pub(crate) effective_path: String,
    pub(crate) alias: String,
    pub(crate) isolated: bool,
    pub(crate) branch: Option<String>,
    pub(crate) ordinal: i64,
}

fn member_from_row(row: &turso::Row) -> Result<StoredWorkspaceMember, String> {
    Ok(StoredWorkspaceMember {
        workspace_id: db::text(row, 0)?,
        source_path: db::text(row, 1)?,
        effective_path: db::text(row, 2)?,
        alias: db::text(row, 3)?,
        isolated: db::flag(row, 4)?,
        branch: db::opt_text(row, 5)?,
        ordinal: db::int(row, 6)?,
    })
}

/// Insert every member of one workspace. Shared by create and update, which
/// differ only in what they do to the workspace row itself. Called with an open
/// transaction (which derefs to its connection) so the batch is atomic.
async fn insert_members(
    transaction: &Connection,
    members: &[StoredWorkspaceMember],
) -> Result<(), String> {
    for member in members {
        db::exec(
            transaction,
            "INSERT INTO workspace_members(
                workspace_id, source_path, effective_path, alias, isolated, branch, ordinal
             ) VALUES (?, ?, ?, ?, ?, ?, ?)",
            params![
                member.workspace_id.clone(),
                member.source_path.clone(),
                member.effective_path.clone(),
                member.alias.clone(),
                i64::from(member.isolated),
                member.branch.clone(),
                member.ordinal,
            ],
        )
        .await?;
    }
    Ok(())
}

pub(crate) async fn read_workspaces(database: &Database) -> Result<Vec<StoredWorkspace>, String> {
    let connection = db::conn(database)?;
    db::rows(
        &connection,
        "SELECT id, name, hub_path, archived FROM workspaces ORDER BY rowid",
        (),
        |row| {
            Ok(StoredWorkspace {
                id: db::text(row, 0)?,
                name: db::text(row, 1)?,
                hub_path: db::text(row, 2)?,
                archived: db::flag(row, 3)?,
            })
        },
    )
    .await
}

pub(crate) async fn read_workspace_members(
    database: &Database,
    workspace_id: &str,
) -> Result<Vec<StoredWorkspaceMember>, String> {
    let connection = db::conn(database)?;
    db::rows(
        &connection,
        &format!(
            "SELECT {MEMBER_COLUMNS}
             FROM workspace_members WHERE workspace_id = ? ORDER BY ordinal"
        ),
        (workspace_id,),
        member_from_row,
    )
    .await
}

pub(crate) async fn read_all_workspace_members(
    database: &Database,
) -> Result<Vec<StoredWorkspaceMember>, String> {
    let connection = db::conn(database)?;
    db::rows(
        &connection,
        &format!(
            "SELECT {MEMBER_COLUMNS}
             FROM workspace_members ORDER BY workspace_id, ordinal"
        ),
        (),
        member_from_row,
    )
    .await
}

pub(crate) async fn create_workspace(
    database: &Database,
    workspace: &StoredWorkspace,
    members: &[StoredWorkspaceMember],
) -> Result<(), String> {
    let connection = db::conn(database)?;
    let transaction = connection
        .unchecked_transaction()
        .await
        .map_err(db::db_error)?;
    db::exec(
        &transaction,
        "INSERT INTO workspaces(id, name, hub_path, archived) VALUES (?, ?, ?, ?)",
        params![
            workspace.id.clone(),
            workspace.name.clone(),
            workspace.hub_path.clone(),
            i64::from(workspace.archived)
        ],
    )
    .await?;
    insert_members(&transaction, members).await?;
    transaction.commit().await.map_err(db::db_error)
}

pub(crate) async fn update_workspace(
    database: &Database,
    workspace: &StoredWorkspace,
    members: &[StoredWorkspaceMember],
) -> Result<(), String> {
    let connection = db::conn(database)?;
    let transaction = connection
        .unchecked_transaction()
        .await
        .map_err(db::db_error)?;
    db::exec(
        &transaction,
        "UPDATE workspaces SET name = ?, hub_path = ?, archived = ? WHERE id = ?",
        params![
            workspace.name.clone(),
            workspace.hub_path.clone(),
            i64::from(workspace.archived),
            workspace.id.clone(),
        ],
    )
    .await?;
    db::exec(
        &transaction,
        "DELETE FROM workspace_members WHERE workspace_id = ?",
        (workspace.id.clone(),),
    )
    .await?;
    insert_members(&transaction, members).await?;
    transaction.commit().await.map_err(db::db_error)
}

pub(crate) async fn workspace_for_thread(
    database: &Database,
    thread_id: &str,
) -> Result<Option<String>, String> {
    let connection = db::conn(database)?;
    db::one(
        &connection,
        "SELECT workspace_id FROM workspace_threads WHERE thread_id = ?",
        (thread_id,),
        |row| db::text(row, 0),
    )
    .await
}

pub(crate) async fn workspace_thread_map(
    database: &Database,
) -> Result<HashMap<String, String>, String> {
    let connection = db::conn(database)?;
    let pairs = db::rows(
        &connection,
        "SELECT thread_id, workspace_id FROM workspace_threads",
        (),
        |row| Ok((db::text(row, 0)?, db::text(row, 1)?)),
    )
    .await?;
    Ok(pairs.into_iter().collect())
}

pub(crate) async fn assign_thread_workspace(
    database: &Database,
    thread_id: &str,
    workspace_id: &str,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "INSERT OR REPLACE INTO workspace_threads(thread_id, workspace_id) VALUES (?, ?)",
        (thread_id, workspace_id),
    )
    .await
}

pub(crate) async fn unassign_thread_workspace(
    database: &Database,
    thread_id: &str,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "DELETE FROM workspace_threads WHERE thread_id = ?",
        (thread_id,),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::open;

    #[tokio::test]
    async fn workspace_membership_and_thread_assignment_round_trip() {
        let directory = tempfile::tempdir().unwrap();
        let database = open(directory.path()).await.unwrap();
        let workspace = StoredWorkspace {
            id: "workspace-1".into(),
            name: "API + Web".into(),
            hub_path: "/tmp/hub".into(),
            archived: false,
        };
        let members = vec![
            StoredWorkspaceMember {
                workspace_id: workspace.id.clone(),
                source_path: "/tmp/api".into(),
                effective_path: "/tmp/api-worktree".into(),
                alias: "api".into(),
                isolated: true,
                branch: Some("codex/workspace-1/api".into()),
                ordinal: 0,
            },
            StoredWorkspaceMember {
                workspace_id: workspace.id.clone(),
                source_path: "/tmp/web".into(),
                effective_path: "/tmp/web".into(),
                alias: "web".into(),
                isolated: false,
                branch: None,
                ordinal: 1,
            },
        ];
        create_workspace(&database, &workspace, &members)
            .await
            .unwrap();
        assert_eq!(
            read_workspaces(&database).await.unwrap(),
            vec![workspace.clone()]
        );
        assert_eq!(
            read_workspace_members(&database, &workspace.id)
                .await
                .unwrap(),
            members
        );
        assert_eq!(
            read_all_workspace_members(&database).await.unwrap(),
            members
        );

        assign_thread_workspace(&database, "thread-1", &workspace.id)
            .await
            .unwrap();
        assert_eq!(
            workspace_for_thread(&database, "thread-1").await.unwrap(),
            Some(workspace.id.clone())
        );
        assert_eq!(
            workspace_thread_map(&database)
                .await
                .unwrap()
                .get("thread-1"),
            Some(&workspace.id)
        );
        unassign_thread_workspace(&database, "thread-1")
            .await
            .unwrap();
        assert!(workspace_for_thread(&database, "thread-1")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn updating_a_workspace_replaces_its_members() {
        let directory = tempfile::tempdir().unwrap();
        let database = open(directory.path()).await.unwrap();
        let mut workspace = StoredWorkspace {
            id: "w1".into(),
            name: "One".into(),
            hub_path: "/tmp/hub".into(),
            archived: false,
        };
        let member = StoredWorkspaceMember {
            workspace_id: "w1".into(),
            source_path: "/tmp/api".into(),
            effective_path: "/tmp/api".into(),
            alias: "api".into(),
            isolated: false,
            branch: None,
            ordinal: 0,
        };
        create_workspace(&database, &workspace, std::slice::from_ref(&member))
            .await
            .unwrap();

        workspace.name = "Renamed".into();
        let replacement = StoredWorkspaceMember {
            alias: "web".into(),
            source_path: "/tmp/web".into(),
            effective_path: "/tmp/web".into(),
            ..member
        };
        update_workspace(&database, &workspace, std::slice::from_ref(&replacement))
            .await
            .unwrap();
        assert_eq!(read_workspaces(&database).await.unwrap()[0].name, "Renamed");
        assert_eq!(
            read_workspace_members(&database, "w1").await.unwrap(),
            vec![replacement]
        );
    }
}
