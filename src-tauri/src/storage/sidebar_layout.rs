//! User-made sidebar folders and the explicit ordering of what sits in them.
//!
//! A folder is a purely local grouping: at the root scope it holds projects
//! (and other root folders); inside a project (`scope` = the project path) it
//! holds that project's threads. Ordering is only ever written for one
//! parent's siblings at a time and only once the user has dragged something,
//! so items without a placement keep their natural order after the placed ones.
//! The tree itself is assembled in the frontend from this flat data.

use serde::{Deserialize, Serialize};
use turso::{params, Database};

use super::db;
use crate::util::id::unique_suffix;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StoredSidebarFolder {
    pub(crate) id: String,
    /// `""` for the top level, else the path of the project it lives in.
    pub(crate) scope: String,
    pub(crate) parent_id: Option<String>,
    pub(crate) name: String,
    pub(crate) expanded: bool,
    pub(crate) ordinal: i64,
}

/// Where one project (root scope) or thread (project scope) was placed.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StoredPlacement {
    pub(crate) item_key: String,
    pub(crate) scope: String,
    pub(crate) parent_id: Option<String>,
    pub(crate) ordinal: i64,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SidebarLayout {
    pub(crate) folders: Vec<StoredSidebarFolder>,
    pub(crate) placements: Vec<StoredPlacement>,
}

/// One entry of a parent's ordered children, as the frontend sends it back.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SiblingRef {
    /// "folder" | "item".
    pub(crate) kind: String,
    pub(crate) id: String,
}

pub(crate) async fn read_sidebar_layout(database: &Database) -> Result<SidebarLayout, String> {
    let connection = db::conn(database)?;
    let folders = db::rows(
        &connection,
        "SELECT id, scope, parent_id, name, expanded, ordinal FROM sidebar_folders
         ORDER BY ordinal, rowid",
        (),
        |row| {
            Ok(StoredSidebarFolder {
                id: db::text(row, 0)?,
                scope: db::text(row, 1)?,
                parent_id: db::opt_text(row, 2)?,
                name: db::text(row, 3)?,
                expanded: db::flag(row, 4)?,
                ordinal: db::int(row, 5)?,
            })
        },
    )
    .await?;
    let placements = db::rows(
        &connection,
        "SELECT item_key, scope, parent_id, ordinal FROM sidebar_placements ORDER BY ordinal",
        (),
        |row| {
            Ok(StoredPlacement {
                item_key: db::text(row, 0)?,
                scope: db::text(row, 1)?,
                parent_id: db::opt_text(row, 2)?,
                ordinal: db::int(row, 3)?,
            })
        },
    )
    .await?;
    Ok(SidebarLayout {
        folders,
        placements,
    })
}

/// Create a folder appended after its siblings; returns its id.
pub(crate) async fn create_sidebar_folder(
    database: &Database,
    scope: &str,
    parent_id: Option<&str>,
    name: &str,
) -> Result<String, String> {
    let connection = db::conn(database)?;
    let id = format!("folder-{}", unique_suffix());
    let next = db::one(
        &connection,
        "SELECT COALESCE(MAX(ordinal), -1) + 1 FROM (
            SELECT ordinal FROM sidebar_folders WHERE scope = ?1 AND parent_id IS ?2
            UNION ALL
            SELECT ordinal FROM sidebar_placements WHERE scope = ?1 AND parent_id IS ?2
         )",
        params![scope.to_string(), parent_id.map(str::to_string)],
        |row| db::int(row, 0),
    )
    .await?
    .unwrap_or(0);
    db::exec(
        &connection,
        "INSERT INTO sidebar_folders(id, scope, parent_id, name, expanded, ordinal)
         VALUES (?, ?, ?, ?, 1, ?)",
        params![
            id.clone(),
            scope.to_string(),
            parent_id.map(str::to_string),
            name.to_string(),
            next
        ],
    )
    .await?;
    Ok(id)
}

pub(crate) async fn rename_sidebar_folder(
    database: &Database,
    id: &str,
    name: &str,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "UPDATE sidebar_folders SET name = ? WHERE id = ?",
        params![name.to_string(), id.to_string()],
    )
    .await
}

pub(crate) async fn set_sidebar_folder_expanded(
    database: &Database,
    id: &str,
    expanded: bool,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "UPDATE sidebar_folders SET expanded = ? WHERE id = ?",
        params![i64::from(expanded), id.to_string()],
    )
    .await
}

/// Remove a folder, lifting everything inside it to the folder's own parent.
/// Nothing the user made (projects, threads, nested folders) is lost.
pub(crate) async fn delete_sidebar_folder(database: &Database, id: &str) -> Result<(), String> {
    let connection = db::conn(database)?;
    let parent = db::one(
        &connection,
        "SELECT parent_id FROM sidebar_folders WHERE id = ?",
        params![id.to_string()],
        |row| db::opt_text(row, 0),
    )
    .await?
    .flatten();
    let transaction = connection
        .unchecked_transaction()
        .await
        .map_err(db::db_error)?;
    // Lifted children go after the folder's remaining siblings, preserving
    // their own relative order.
    db::exec(
        &transaction,
        "UPDATE sidebar_folders SET parent_id = ?1, ordinal = ordinal + 1000000 WHERE parent_id = ?2",
        params![parent.clone(), id.to_string()],
    )
    .await?;
    db::exec(
        &transaction,
        "UPDATE sidebar_placements SET parent_id = ?1, ordinal = ordinal + 1000000 WHERE parent_id = ?2",
        params![parent.clone(), id.to_string()],
    )
    .await?;
    db::exec(
        &transaction,
        "DELETE FROM sidebar_folders WHERE id = ?",
        params![id.to_string()],
    )
    .await?;
    transaction.commit().await.map_err(db::db_error)
}

/// Whether `candidate` is `folder_id` itself or one of its descendants.
pub(crate) fn is_folder_or_descendant(
    folders: &[StoredSidebarFolder],
    folder_id: &str,
    candidate: &str,
) -> bool {
    let mut current = Some(candidate.to_string());
    let mut hops = 0;
    while let Some(id) = current {
        if id == folder_id {
            return true;
        }
        hops += 1;
        if hops > folders.len() {
            return false;
        }
        current = folders
            .iter()
            .find(|folder| folder.id == id)
            .and_then(|folder| folder.parent_id.clone());
    }
    false
}

/// Put `item` under `parent_id` and rewrite the ordinals of every sibling
/// there to match `siblings` (which must include `item`). Any sibling missing
/// from the list keeps its row but sorts after the listed ones.
pub(crate) async fn place_sidebar_item(
    database: &Database,
    scope: &str,
    item: &SiblingRef,
    parent_id: Option<&str>,
    siblings: &[SiblingRef],
) -> Result<(), String> {
    let connection = db::conn(database)?;
    let transaction = connection
        .unchecked_transaction()
        .await
        .map_err(db::db_error)?;
    // Push the unlisted siblings out of the way first so listed order wins.
    db::exec(
        &transaction,
        "UPDATE sidebar_folders SET ordinal = ordinal + 1000000 WHERE scope = ?1 AND parent_id IS ?2",
        params![scope.to_string(), parent_id.map(str::to_string)],
    )
    .await?;
    db::exec(
        &transaction,
        "UPDATE sidebar_placements SET ordinal = ordinal + 1000000 WHERE scope = ?1 AND parent_id IS ?2",
        params![scope.to_string(), parent_id.map(str::to_string)],
    )
    .await?;
    if item.kind == "folder" {
        db::exec(
            &transaction,
            "UPDATE sidebar_folders SET parent_id = ? WHERE id = ? AND scope = ?",
            params![
                parent_id.map(str::to_string),
                item.id.clone(),
                scope.to_string()
            ],
        )
        .await?;
    } else {
        db::exec(
            &transaction,
            "INSERT INTO sidebar_placements(item_key, scope, parent_id, ordinal) VALUES (?, ?, ?, 0)
             ON CONFLICT(item_key) DO UPDATE SET scope = excluded.scope, parent_id = excluded.parent_id",
            params![
                item.id.clone(),
                scope.to_string(),
                parent_id.map(str::to_string)
            ],
        )
        .await?;
    }
    for (ordinal, sibling) in siblings.iter().enumerate() {
        let ordinal = ordinal as i64;
        if sibling.kind == "folder" {
            db::exec(
                &transaction,
                "UPDATE sidebar_folders SET ordinal = ? WHERE id = ? AND scope = ? AND parent_id IS ?",
                params![
                    ordinal,
                    sibling.id.clone(),
                    scope.to_string(),
                    parent_id.map(str::to_string)
                ],
            )
            .await?;
        } else {
            db::exec(
                &transaction,
                "INSERT INTO sidebar_placements(item_key, scope, parent_id, ordinal) VALUES (?, ?, ?, ?)
                 ON CONFLICT(item_key) DO UPDATE SET scope = excluded.scope,
                    parent_id = excluded.parent_id, ordinal = excluded.ordinal",
                params![
                    sibling.id.clone(),
                    scope.to_string(),
                    parent_id.map(str::to_string),
                    ordinal
                ],
            )
            .await?;
        }
    }
    transaction.commit().await.map_err(db::db_error)
}

/// Forget the user's drag ordering within `scope`. Items at the scope root
/// lose their placement entirely; items inside folders keep the folder but
/// share one ordinal, so the natural (favourites, then recency) order wins
/// again. Folders keep their own order.
pub(crate) async fn reset_sidebar_order(database: &Database, scope: &str) -> Result<(), String> {
    let connection = db::conn(database)?;
    let transaction = connection
        .unchecked_transaction()
        .await
        .map_err(db::db_error)?;
    db::exec(
        &transaction,
        "DELETE FROM sidebar_placements WHERE scope = ? AND parent_id IS NULL",
        params![scope.to_string()],
    )
    .await?;
    db::exec(
        &transaction,
        "UPDATE sidebar_placements SET ordinal = 0 WHERE scope = ?",
        params![scope.to_string()],
    )
    .await?;
    transaction.commit().await.map_err(db::db_error)
}

/// Drop every folder and placement inside a project that was removed.
pub(crate) async fn forget_sidebar_scope(database: &Database, scope: &str) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "DELETE FROM sidebar_folders WHERE scope = ?",
        params![scope.to_string()],
    )
    .await?;
    db::exec(
        &connection,
        "DELETE FROM sidebar_placements WHERE scope = ?",
        params![scope.to_string()],
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::open;

    const ROOT_SCOPE: &str = "";

    fn item(id: &str) -> SiblingRef {
        SiblingRef {
            kind: "item".into(),
            id: id.into(),
        }
    }
    fn folder(id: &str) -> SiblingRef {
        SiblingRef {
            kind: "folder".into(),
            id: id.into(),
        }
    }

    #[tokio::test]
    async fn folders_and_placements_survive_reopen_and_order_by_ordinal() {
        let directory = tempfile::tempdir().unwrap();
        let database = open(directory.path()).await.unwrap();
        let work = create_sidebar_folder(&database, ROOT_SCOPE, None, "Work")
            .await
            .unwrap();
        let inner = create_sidebar_folder(&database, ROOT_SCOPE, Some(&work), "Inner")
            .await
            .unwrap();
        place_sidebar_item(
            &database,
            ROOT_SCOPE,
            &item("/b"),
            Some(&work),
            &[item("/b"), folder(&inner), item("/a")],
        )
        .await
        .unwrap();
        drop(database);

        let reopened = open(directory.path()).await.unwrap();
        let layout = read_sidebar_layout(&reopened).await.unwrap();
        assert_eq!(layout.folders.len(), 2);
        let inner_folder = layout.folders.iter().find(|f| f.id == inner).unwrap();
        assert_eq!(inner_folder.parent_id.as_deref(), Some(work.as_str()));
        assert_eq!(inner_folder.ordinal, 1);
        let keys: Vec<_> = layout
            .placements
            .iter()
            .map(|p| (p.item_key.as_str(), p.ordinal))
            .collect();
        assert_eq!(keys, vec![("/b", 0), ("/a", 2)]);
    }

    #[tokio::test]
    async fn deleting_a_folder_lifts_its_children_to_its_parent() {
        let directory = tempfile::tempdir().unwrap();
        let database = open(directory.path()).await.unwrap();
        let outer = create_sidebar_folder(&database, ROOT_SCOPE, None, "Outer")
            .await
            .unwrap();
        let inner = create_sidebar_folder(&database, ROOT_SCOPE, Some(&outer), "Inner")
            .await
            .unwrap();
        place_sidebar_item(
            &database,
            ROOT_SCOPE,
            &item("/a"),
            Some(&inner),
            &[item("/a")],
        )
        .await
        .unwrap();
        delete_sidebar_folder(&database, &inner).await.unwrap();

        let layout = read_sidebar_layout(&database).await.unwrap();
        assert_eq!(layout.folders.len(), 1);
        assert_eq!(
            layout.placements[0].parent_id.as_deref(),
            Some(outer.as_str())
        );
    }

    #[tokio::test]
    async fn forgetting_a_scope_leaves_other_scopes_alone() {
        let directory = tempfile::tempdir().unwrap();
        let database = open(directory.path()).await.unwrap();
        create_sidebar_folder(&database, "/gone", None, "Dead")
            .await
            .unwrap();
        create_sidebar_folder(&database, "/live", None, "Alive")
            .await
            .unwrap();
        place_sidebar_item(&database, "/gone", &item("t1"), None, &[item("t1")])
            .await
            .unwrap();
        forget_sidebar_scope(&database, "/gone").await.unwrap();

        let layout = read_sidebar_layout(&database).await.unwrap();
        assert_eq!(layout.folders.len(), 1);
        assert_eq!(layout.folders[0].scope, "/live");
        assert!(layout.placements.is_empty());
    }

    #[tokio::test]
    async fn resetting_order_drops_root_placements_and_flattens_folder_ordinals() {
        let directory = tempfile::tempdir().unwrap();
        let database = open(directory.path()).await.unwrap();
        let work = create_sidebar_folder(&database, "/p", None, "Work")
            .await
            .unwrap();
        place_sidebar_item(
            &database,
            "/p",
            &item("t1"),
            None,
            &[item("t1"), item("t2")],
        )
        .await
        .unwrap();
        place_sidebar_item(
            &database,
            "/p",
            &item("t3"),
            Some(&work),
            &[item("t4"), item("t3")],
        )
        .await
        .unwrap();
        place_sidebar_item(&database, "/other", &item("o1"), None, &[item("o1")])
            .await
            .unwrap();
        reset_sidebar_order(&database, "/p").await.unwrap();

        let layout = read_sidebar_layout(&database).await.unwrap();
        let mut placements: Vec<_> = layout
            .placements
            .iter()
            .map(|p| {
                (
                    p.item_key.as_str(),
                    p.scope.as_str(),
                    p.parent_id.as_deref(),
                    p.ordinal,
                )
            })
            .collect();
        placements.sort();
        assert_eq!(
            placements,
            vec![
                ("o1", "/other", None, 0),
                ("t3", "/p", Some(work.as_str()), 0),
                ("t4", "/p", Some(work.as_str()), 0),
            ]
        );
        assert_eq!(layout.folders.len(), 1);
    }

    #[test]
    fn descendant_check_walks_parents() {
        let folders = vec![
            StoredSidebarFolder {
                id: "a".into(),
                scope: ROOT_SCOPE.into(),
                parent_id: None,
                name: "A".into(),
                expanded: true,
                ordinal: 0,
            },
            StoredSidebarFolder {
                id: "b".into(),
                scope: ROOT_SCOPE.into(),
                parent_id: Some("a".into()),
                name: "B".into(),
                expanded: true,
                ordinal: 0,
            },
        ];
        assert!(is_folder_or_descendant(&folders, "a", "b"));
        assert!(is_folder_or_descendant(&folders, "a", "a"));
        assert!(!is_folder_or_descendant(&folders, "b", "a"));
    }
}
