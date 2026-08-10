//! Commands that create and edit workspaces.
//!
//! Both create and update are multi-step (git worktrees, then hub links, then
//! the database) and every step after the first has to undo the ones before it
//! on failure — hence the `created` rollback list threaded through both.

use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::State;

use super::hub::{materialize_hub, remove_managed_link};
use super::worktree::{
    available_branch, create_isolated_worktree, is_git_repository, remove_created_worktree,
};
use super::{clean_alias, runtime_for_workspace, workspace_id};
use crate::projects::{bootstrap_cached, BootstrapData};
use crate::storage::{self, StoredWorkspace, StoredWorkspaceMember};
use crate::AppState;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorkspaceMemberInput {
    source_path: String,
    alias: String,
    #[serde(default)]
    isolated: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CreateWorkspaceInput {
    name: String,
    members: Vec<WorkspaceMemberInput>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UpdateWorkspaceInput {
    workspace_id: String,
    name: String,
    members: Vec<WorkspaceMemberInput>,
}

/// Check the requested membership before anything is created: at least two
/// projects, unique aliases, real folders, and no project nested inside another.
fn validate_members(
    inputs: &[WorkspaceMemberInput],
) -> Result<Vec<(WorkspaceMemberInput, PathBuf)>, String> {
    if inputs.len() < 2 {
        return Err("Choose at least two projects for a workspace".into());
    }
    let mut aliases = HashSet::new();
    let mut paths = Vec::new();
    for input in inputs {
        let alias = clean_alias(&input.alias)?;
        if !aliases.insert(alias) {
            return Err("Workspace member aliases must be unique".into());
        }
        let path = fs::canonicalize(&input.source_path)
            .map_err(|_| format!("Could not open {}", input.source_path))?;
        if !path.is_dir() {
            return Err(format!("{} is not a folder", path.display()));
        }
        paths.push((input.clone(), path));
    }
    for (index, (_, path)) in paths.iter().enumerate() {
        if paths.iter().enumerate().any(|(other_index, (_, other))| {
            other_index != index && (path.starts_with(other) || other.starts_with(path))
        }) {
            return Err("Workspace projects cannot overlap or contain one another".into());
        }
    }
    Ok(paths)
}

/// Undo every worktree created so far, newest first.
fn roll_back(created: &[(PathBuf, PathBuf, String)]) {
    for (source, destination, branch) in created.iter().rev() {
        remove_created_worktree(source, destination, branch);
    }
}

#[tauri::command]
pub(crate) async fn create_workspace(
    input: CreateWorkspaceInput,
    state: State<'_, AppState>,
) -> Result<BootstrapData, String> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err("Give the workspace a name".into());
    }
    let inputs = validate_members(&input.members)?;
    let runtime = state.runtime();
    let id = workspace_id();
    let hub = runtime.codex_home.join("multi-projects").join(&id);
    let mut created = Vec::<(PathBuf, PathBuf, String)>::new();
    let mut members = Vec::new();
    for (ordinal, (input, source)) in inputs.into_iter().enumerate() {
        let alias = clean_alias(&input.alias)?;
        let source_string = source.display().to_string();
        let (effective, branch) = if input.isolated && is_git_repository(&source) {
            let branch = available_branch(&source, &id, &alias);
            let destination = runtime.codex_home.join("worktrees").join(&id).join(&alias);
            if let Err(error) = create_isolated_worktree(&source, &destination, &branch) {
                roll_back(&created);
                return Err(error);
            }
            created.push((source.clone(), destination.clone(), branch.clone()));
            (destination.display().to_string(), Some(branch))
        } else {
            (source_string.clone(), None)
        };
        members.push(StoredWorkspaceMember {
            workspace_id: id.clone(),
            source_path: source_string,
            effective_path: effective,
            alias,
            isolated: branch.is_some(),
            branch,
            ordinal: ordinal as i64,
        });
    }
    let workspace = StoredWorkspace {
        id: id.clone(),
        name: name.to_string(),
        hub_path: hub.display().to_string(),
        archived: false,
    };
    let disk_workspace = workspace.clone();
    let disk_members = members.clone();
    let materialized = tauri::async_runtime::spawn_blocking(move || {
        materialize_hub(&disk_workspace, &disk_members)
    })
    .await
    .map_err(|_| "Could not prepare workspace directory".to_string())?;
    if let Err(error) = materialized {
        roll_back(&created);
        return Err(error);
    }
    if let Err(error) = storage::create_workspace(&state.database(), &workspace, &members).await {
        roll_back(&created);
        return Err(error);
    }
    bootstrap_cached(&state).await
}

#[tauri::command]
pub(crate) async fn update_workspace(
    input: UpdateWorkspaceInput,
    state: State<'_, AppState>,
) -> Result<BootstrapData, String> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err("Give the workspace a name".into());
    }
    let requested = validate_members(&input.members)?;
    let mut workspace = storage::read_workspaces(&state.database())
        .await?
        .into_iter()
        .find(|workspace| workspace.id == input.workspace_id && !workspace.archived)
        .ok_or("This workspace no longer exists or is archived")?;
    let old_members = storage::read_workspace_members(&state.database(), &workspace.id).await?;
    let old_by_source: HashMap<_, _> = old_members
        .iter()
        .map(|member| (member.source_path.as_str(), member))
        .collect();
    let runtime = state.runtime();
    let mut created = Vec::<(PathBuf, PathBuf, String)>::new();
    let mut members = Vec::new();
    for (ordinal, (requested, source)) in requested.into_iter().enumerate() {
        let alias = clean_alias(&requested.alias)?;
        let source_path = source.display().to_string();
        let old = old_by_source.get(source_path.as_str()).copied();
        let (effective_path, branch, isolated) = match old {
            Some(member) if member.isolated == requested.isolated => (
                member.effective_path.clone(),
                member.branch.clone(),
                member.isolated,
            ),
            _ if requested.isolated && is_git_repository(&source) => {
                let branch = available_branch(&source, &workspace.id, &alias);
                let destination = runtime
                    .codex_home
                    .join("worktrees")
                    .join(&workspace.id)
                    .join(&alias);
                if let Err(error) = create_isolated_worktree(&source, &destination, &branch) {
                    roll_back(&created);
                    return Err(error);
                }
                created.push((source.clone(), destination.clone(), branch.clone()));
                (destination.display().to_string(), Some(branch), true)
            }
            _ => (source_path.clone(), None, false),
        };
        members.push(StoredWorkspaceMember {
            workspace_id: workspace.id.clone(),
            source_path,
            effective_path,
            alias,
            isolated,
            branch,
            ordinal: ordinal as i64,
        });
    }

    let hub = Path::new(&workspace.hub_path);
    // Remove only aliases that were managed and are no longer identical. If
    // any were replaced by a user file, fail before changing the database.
    for old in &old_members {
        let retained = members
            .iter()
            .any(|member| member.alias == old.alias && member.effective_path == old.effective_path);
        if !retained {
            if let Err(error) = remove_managed_link(hub, old) {
                roll_back(&created);
                return Err(error);
            }
        }
    }
    workspace.name = name.to_string();
    let disk_workspace = workspace.clone();
    let disk_members = members.clone();
    let materialized = tauri::async_runtime::spawn_blocking(move || {
        materialize_hub(&disk_workspace, &disk_members)
    })
    .await
    .map_err(|_| "Could not prepare workspace directory".to_string())?;
    if let Err(error) = materialized {
        // Recreate the old managed links when the update failed after a rename
        // or removal. User files remain untouched in either case.
        let old_workspace = workspace.clone();
        let old_for_disk = old_members.clone();
        let _ = tauri::async_runtime::spawn_blocking(move || {
            materialize_hub(&old_workspace, &old_for_disk)
        })
        .await;
        roll_back(&created);
        return Err(error);
    }
    if let Err(error) = storage::update_workspace(&state.database(), &workspace, &members).await {
        roll_back(&created);
        return Err(error);
    }
    bootstrap_cached(&state).await
}

#[tauri::command]
pub(crate) async fn move_thread_to_workspace(
    thread_id: String,
    workspace_id: String,
    state: State<'_, AppState>,
) -> Result<BootstrapData, String> {
    // Materialize and validate now, rather than saving a thread association
    // that will only fail at its next turn.
    runtime_for_workspace(&state, &workspace_id).await?;
    storage::assign_thread_workspace(&state.database(), &thread_id, &workspace_id).await?;
    bootstrap_cached(&state).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_nested_workspace_roots() {
        let temp = tempfile::tempdir().unwrap();
        let parent = temp.path().join("parent");
        let child = parent.join("child");
        fs::create_dir_all(&child).unwrap();
        let inputs = vec![
            WorkspaceMemberInput {
                source_path: parent.display().to_string(),
                alias: "parent".into(),
                isolated: false,
            },
            WorkspaceMemberInput {
                source_path: child.display().to_string(),
                alias: "child".into(),
                isolated: false,
            },
        ];
        assert!(validate_members(&inputs).is_err());
    }

    #[test]
    fn rejects_duplicate_aliases_and_lone_members() {
        let temp = tempfile::tempdir().unwrap();
        let one = temp.path().join("one");
        let two = temp.path().join("two");
        fs::create_dir_all(&one).unwrap();
        fs::create_dir_all(&two).unwrap();
        let member = |path: &Path, alias: &str| WorkspaceMemberInput {
            source_path: path.display().to_string(),
            alias: alias.into(),
            isolated: false,
        };

        assert!(validate_members(&[member(&one, "one")]).is_err());
        assert!(validate_members(&[member(&one, "same"), member(&two, "same")]).is_err());
        assert!(validate_members(&[member(&one, "one"), member(&two, "two")]).is_ok());
    }
}
