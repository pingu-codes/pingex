//! Per-project sidebar presentation preferences.
//!
//! A project can be a saved folder, a discovered worktree, or a virtual
//! workspace. Keeping its expansion state in a separate path-keyed table makes
//! the preference apply uniformly to all three without changing their distinct
//! persistence models.

use std::collections::HashMap;

use turso::{params, Database};

use super::db;

/// Read the explicitly stored expansion state for each project path.
///
/// Projects without a row intentionally default to expanded in the bootstrap
/// builder, so newly discovered projects need no eager database write.
pub(crate) async fn read_project_expansion(
    database: &Database,
) -> Result<HashMap<String, bool>, String> {
    let connection = db::conn(database)?;
    let pairs = db::rows(
        &connection,
        "SELECT project_path, expanded FROM project_expansion",
        (),
        |row| Ok((db::text(row, 0)?, db::flag(row, 1)?)),
    )
    .await?;
    Ok(pairs.into_iter().collect())
}

/// Store the sidebar expansion state for one project path.
pub(crate) async fn set_project_expanded(
    database: &Database,
    project_path: &str,
    expanded: bool,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "INSERT INTO project_expansion(project_path, expanded) VALUES (?, ?)
         ON CONFLICT(project_path) DO UPDATE SET expanded = excluded.expanded",
        params![project_path.to_string(), i64::from(expanded)],
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::open;

    #[tokio::test]
    async fn persists_expansion_state_across_database_reopens() {
        let directory = tempfile::tempdir().unwrap();
        let database = open(directory.path()).await.unwrap();
        set_project_expanded(&database, "/project/open", true)
            .await
            .unwrap();
        set_project_expanded(&database, "/project/closed", false)
            .await
            .unwrap();
        drop(database);

        let reopened = open(directory.path()).await.unwrap();
        assert_eq!(
            read_project_expansion(&reopened).await.unwrap(),
            HashMap::from([
                ("/project/open".to_string(), true),
                ("/project/closed".to_string(), false),
            ])
        );
    }
}
