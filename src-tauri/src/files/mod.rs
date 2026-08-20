//! Project file listing and `@`-mention search.
//!
//! Both commands take a single root, except when that root is a workspace hub —
//! then each member tree is searched separately and its hits are prefixed with
//! the member alias, so `@api/src/main.rs` addresses the right repository.

use std::collections::HashSet;
use std::path::PathBuf;
use tauri::State;

use crate::storage;
use crate::AppState;

pub(crate) mod fuzzy;

#[tauri::command]
pub(crate) async fn search_project_files(
    root: String,
    query: String,
    limit: Option<usize>,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Vec<fuzzy::FileHit>, String> {
    let ctx = state.ctx(&window);
    let root_path = PathBuf::from(&root);
    if !root_path.is_dir() {
        return Err(format!("{root} is not a folder"));
    }
    let limit = limit.unwrap_or(20).min(100);
    let workspace_members = workspace_members_for_hub(&root, &ctx).await?;
    tauri::async_runtime::spawn_blocking(move || {
        let Some(members) = workspace_members else {
            return fuzzy::search_files(&root_path, &query, limit);
        };

        let aliases = members
            .iter()
            .map(|member| member.alias.clone())
            .collect::<HashSet<_>>();
        let mut hits = fuzzy::search_files(&root_path, &query, limit)
            .into_iter()
            .filter(|hit| !is_workspace_managed_path(&hit.path, &aliases))
            .collect::<Vec<_>>();
        for member in members {
            let member_root = PathBuf::from(member.effective_path);
            if !member_root.is_dir() {
                continue;
            }
            for mut hit in fuzzy::search_files(&member_root, &query, limit) {
                hit.path = format!("{}/{}", member.alias, hit.path);
                hits.push(hit);
            }
        }
        hits.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.path.cmp(&b.path)));
        hits.truncate(limit);
        hits
    })
    .await
    .map_err(|error| format!("File search failed: {error}"))
}

#[tauri::command]
pub(crate) async fn list_project_files(
    root: String,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    let ctx = state.ctx(&window);
    let root_path = PathBuf::from(&root);
    if !root_path.is_dir() {
        return Err(format!("{root} is not a folder"));
    }
    let workspace_members = workspace_members_for_hub(&root, &ctx).await?;
    tauri::async_runtime::spawn_blocking(move || {
        let Some(members) = workspace_members else {
            return fuzzy::list_files(&root_path);
        };

        let aliases = members
            .iter()
            .map(|member| member.alias.clone())
            .collect::<HashSet<_>>();
        let mut paths = fuzzy::list_files(&root_path)
            .into_iter()
            .filter(|path| !is_workspace_managed_path(path, &aliases))
            .collect::<Vec<_>>();
        for member in members {
            let member_root = PathBuf::from(member.effective_path);
            if !member_root.is_dir() {
                continue;
            }
            paths.extend(
                fuzzy::list_files(&member_root)
                    .into_iter()
                    .map(|path| format!("{}/{}", member.alias, path)),
            );
        }
        paths.sort();
        paths.dedup();
        paths
    })
    .await
    .map_err(|error| format!("File listing failed: {error}"))
}

/// Return workspace members only when `root` is the workspace's virtual hub.
/// Normal project file commands retain their single-root behavior.
async fn workspace_members_for_hub(
    root: &str,
    ctx: &crate::HomeContext,
) -> Result<Option<Vec<storage::StoredWorkspaceMember>>, String> {
    let database = ctx.database();
    let Some(workspace) = storage::read_workspaces(&database)
        .await?
        .into_iter()
        .find(|workspace| !workspace.archived && workspace.hub_path == root)
    else {
        return Ok(None);
    };
    storage::read_workspace_members(&database, &workspace.id)
        .await
        .map(Some)
}

/// The hub contains implementation-managed member links and metadata. Search
/// the hub for user-owned files (notes, plans, etc.) without double-counting
/// those member trees; each member is searched independently above.
fn is_workspace_managed_path(path: &str, aliases: &HashSet<String>) -> bool {
    let first_component = path.split('/').next().unwrap_or_default();
    first_component == ".pingu" || aliases.contains(first_component)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_file_walk_keeps_hub_files_but_excludes_managed_entries() {
        let aliases = HashSet::from(["api".to_string(), "web".to_string()]);
        assert!(is_workspace_managed_path(".pingu/manifest.json", &aliases));
        assert!(is_workspace_managed_path("api/src/main.rs", &aliases));
        assert!(is_workspace_managed_path("web/package.json", &aliases));
        assert!(!is_workspace_managed_path("NOTES.md", &aliases));
        assert!(!is_workspace_managed_path("plans/release.md", &aliases));
    }
}
