//! Message versions. Editing a user message forks the thread strictly before
//! that message's turn and sends the new text on the fork, so every version
//! survives as its own thread. Codex has no idea these forks are related; the
//! rows here are what lets the UI show them as `‹ 2 / 3 ›` under one message
//! and hide them from the sidebar.
//!
//! Turn ids are preserved verbatim across a fork, so `group_turn_id` — the id
//! of the original message's turn — identifies a version group anywhere in
//! the family of threads.

use serde::Serialize;
use turso::{params, Database};

use super::db;

#[derive(Debug, Clone, Serialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ThreadBranch {
    /// The fork.
    pub thread_id: String,
    /// The thread the fork was taken from.
    pub parent_thread_id: String,
    /// Turn id of the original message being versioned.
    pub group_turn_id: String,
    /// The turn the fork excluded — the original, or a sibling's edit turn when
    /// an edit was itself edited.
    pub replaced_turn_id: String,
    /// How many turns the fork inherited; the edit turn is the next one.
    pub inherited_turns: u32,
    /// The fork's first own turn, once known.
    pub edit_turn_id: Option<String>,
    pub created_at: i64,
    /// Last activity, filled in from the thread listing at bootstrap so the UI
    /// can land on the newest leaf of a family. Not stored.
    pub updated_at: Option<i64>,
}

pub async fn add_thread_branch(database: &Database, branch: &ThreadBranch) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "INSERT INTO thread_branches(thread_id, parent_thread_id, group_turn_id, replaced_turn_id,
                                     inherited_turns, edit_turn_id, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(thread_id) DO NOTHING",
        params![
            branch.thread_id.clone(),
            branch.parent_thread_id.clone(),
            branch.group_turn_id.clone(),
            branch.replaced_turn_id.clone(),
            i64::from(branch.inherited_turns),
            branch.edit_turn_id.clone(),
            branch.created_at
        ],
    )
    .await
}

pub async fn set_branch_edit_turn(
    database: &Database,
    thread_id: &str,
    edit_turn_id: &str,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "UPDATE thread_branches SET edit_turn_id = ? WHERE thread_id = ?",
        params![edit_turn_id, thread_id],
    )
    .await
}

pub async fn read_thread_branches(database: &Database) -> Result<Vec<ThreadBranch>, String> {
    let connection = db::conn(database)?;
    db::rows(
        &connection,
        "SELECT thread_id, parent_thread_id, group_turn_id, replaced_turn_id, inherited_turns,
                edit_turn_id, created_at
         FROM thread_branches ORDER BY created_at, thread_id",
        (),
        |row| {
            Ok(ThreadBranch {
                thread_id: db::text(row, 0)?,
                parent_thread_id: db::text(row, 1)?,
                group_turn_id: db::text(row, 2)?,
                replaced_turn_id: db::text(row, 3)?,
                inherited_turns: u32::try_from(db::int(row, 4)?).unwrap_or(0),
                edit_turn_id: db::opt_text(row, 5)?,
                created_at: db::int(row, 6)?,
                updated_at: None,
            })
        },
    )
    .await
}

/// The group an edit turn belongs to, when `turn_id` is some branch's edit
/// turn — editing an edit adds a version to the original's group rather than
/// starting a nested one.
pub(crate) async fn branch_group_for_turn(
    database: &Database,
    turn_id: &str,
) -> Result<Option<String>, String> {
    let connection = db::conn(database)?;
    let groups = db::rows(
        &connection,
        "SELECT group_turn_id FROM thread_branches WHERE edit_turn_id = ? LIMIT 1",
        (turn_id,),
        |row| db::text(row, 0),
    )
    .await?;
    Ok(groups.into_iter().next())
}

/// Every branch below `thread_id`, deepest first, so they can be deleted in
/// an order that never orphans a row.
pub(crate) async fn branch_descendants(
    database: &Database,
    thread_id: &str,
) -> Result<Vec<String>, String> {
    let branches = read_thread_branches(database).await?;
    let mut frontier = vec![thread_id.to_string()];
    let mut found = Vec::new();
    while let Some(parent) = frontier.pop() {
        for branch in &branches {
            if branch.parent_thread_id == parent && !found.contains(&branch.thread_id) {
                found.push(branch.thread_id.clone());
                frontier.push(branch.thread_id.clone());
            }
        }
    }
    found.reverse();
    Ok(found)
}

pub async fn delete_thread_branch(database: &Database, thread_id: &str) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "DELETE FROM thread_branches WHERE thread_id = ?",
        (thread_id,),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::open;

    fn branch(
        thread: &str,
        parent: &str,
        group: &str,
        replaced: &str,
        created_at: i64,
    ) -> ThreadBranch {
        ThreadBranch {
            thread_id: thread.into(),
            parent_thread_id: parent.into(),
            group_turn_id: group.into(),
            replaced_turn_id: replaced.into(),
            inherited_turns: 1,
            edit_turn_id: None,
            created_at,
            updated_at: None,
        }
    }

    #[tokio::test]
    async fn stores_branches_in_creation_order_and_tracks_edit_turns() {
        let directory = tempfile::tempdir().unwrap();
        let database = open(directory.path()).await.unwrap();
        add_thread_branch(&database, &branch("fork-2", "root", "turn-2", "turn-2", 20))
            .await
            .unwrap();
        add_thread_branch(&database, &branch("fork-1", "root", "turn-2", "turn-2", 10))
            .await
            .unwrap();
        // Re-adding never rewrites what is there.
        add_thread_branch(&database, &branch("fork-1", "other", "x", "x", 99))
            .await
            .unwrap();

        let listed = read_thread_branches(&database).await.unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].thread_id, "fork-1");
        assert_eq!(listed[0].parent_thread_id, "root");
        assert_eq!(listed[1].thread_id, "fork-2");

        set_branch_edit_turn(&database, "fork-1", "turn-2b")
            .await
            .unwrap();
        let listed = read_thread_branches(&database).await.unwrap();
        assert_eq!(listed[0].edit_turn_id.as_deref(), Some("turn-2b"));
        assert_eq!(
            branch_group_for_turn(&database, "turn-2b").await.unwrap(),
            Some("turn-2".to_string())
        );
        assert_eq!(
            branch_group_for_turn(&database, "turn-2").await.unwrap(),
            None
        );

        delete_thread_branch(&database, "fork-1").await.unwrap();
        assert_eq!(read_thread_branches(&database).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn descendants_come_back_deepest_first() {
        let directory = tempfile::tempdir().unwrap();
        let database = open(directory.path()).await.unwrap();
        add_thread_branch(&database, &branch("child", "root", "t1", "t1", 1))
            .await
            .unwrap();
        add_thread_branch(&database, &branch("grandchild", "child", "t3", "t3", 2))
            .await
            .unwrap();
        add_thread_branch(&database, &branch("great", "grandchild", "t5", "t5", 3))
            .await
            .unwrap();
        add_thread_branch(&database, &branch("unrelated", "elsewhere", "t9", "t9", 4))
            .await
            .unwrap();

        let descendants = branch_descendants(&database, "root").await.unwrap();
        assert_eq!(descendants, vec!["great", "grandchild", "child"]);
        assert!(branch_descendants(&database, "great")
            .await
            .unwrap()
            .is_empty());
    }
}
