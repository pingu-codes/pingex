//! Commands that create and edit workspaces.
//!
//! Both create and update are multi-step (git worktrees, then hub links, then
//! the database) and every step after the first has to undo the ones before it
//! on failure — hence the `created` rollback list threaded through both.

use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use tauri::State;

use super::hub::{materialize_hub, remove_managed_link};
use super::worktree::{
    available_branch, create_isolated_worktree, is_git_repository, remove_created_worktree,
};
use super::{clean_alias, runtime_for_workspace, workspace_id};
use crate::projects::{bootstrap_cached, BootstrapData};
use crate::storage::{self, StoredWorkspace, StoredWorkspaceMember};
use crate::util::host::Host;
use crate::AppState;

#[derive(Clone, Debug, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorkspaceMemberInput {
    source_path: String,
    alias: String,
    #[serde(default)]
    isolated: bool,
}

#[derive(Clone, Debug, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CreateWorkspaceInput {
    name: String,
    members: Vec<WorkspaceMemberInput>,
}

#[derive(Clone, Debug, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UpdateWorkspaceInput {
    workspace_id: String,
    name: String,
    members: Vec<WorkspaceMemberInput>,
}

/// Check the requested membership before anything is created: at least two
/// projects, unique aliases, real folders, and no project nested inside another.
fn validate_members(
    host: &Host,
    inputs: &[WorkspaceMemberInput],
) -> Result<Vec<(WorkspaceMemberInput, String)>, String> {
    if inputs.len() < 2 {
        return Err("Choose at least two projects for a workspace".into());
    }
    let mut aliases = HashSet::new();
    let mut paths = Vec::new();
    for input in inputs {
        let (picked_host, _) = Host::from_local(&input.source_path);
        if picked_host.is_wsl() && picked_host != *host {
            return Err("A workspace can only contain projects on the same Host".into());
        }
        let alias = clean_alias(&input.alias)?;
        if !aliases.insert(alias) {
            return Err("Workspace member aliases must be unique".into());
        }
        // A dialog pick on a WSL home is a local path; settle it first.
        let source = host.to_host_path(&input.source_path);
        if !host.is_dir(&source) {
            return Err(format!("Could not open {}", input.source_path));
        }
        paths.push((input.clone(), host.canonical(&source)));
    }
    for (index, (_, path)) in paths.iter().enumerate() {
        if paths.iter().enumerate().any(|(other_index, (_, other))| {
            other_index != index && (host.is_under(other, path) || host.is_under(path, other))
        }) {
            return Err("Workspace projects cannot overlap or contain one another".into());
        }
    }
    Ok(paths)
}

/// Undo every worktree created so far, newest first.
fn roll_back(host: &Host, created: &[(String, String, String)]) {
    for (source, destination, branch) in created.iter().rev() {
        remove_created_worktree(host, source, destination, branch);
    }
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn create_workspace(
    input: CreateWorkspaceInput,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<BootstrapData, String> {
    let ctx = state.ctx(&window);
    let name = input.name.trim();
    if name.is_empty() {
        return Err("Give the workspace a name".into());
    }
    let runtime = ctx.runtime();
    let host = runtime.host.clone();
    let home = runtime.codex_home_str();
    let inputs = validate_members(&host, &input.members)?;
    let id = workspace_id();
    let hub = host.join_str(&host.join_str(&home, "multi-projects"), &id);
    let mut created = Vec::<(String, String, String)>::new();
    let mut members = Vec::new();
    for (ordinal, (input, source)) in inputs.into_iter().enumerate() {
        let alias = clean_alias(&input.alias)?;
        let source_string = source.clone();
        let (effective, branch) = if input.isolated && is_git_repository(&host, &source) {
            let branch = available_branch(&host, &source, &id, &alias);
            let destination = host.join_str(
                &host.join_str(&host.join_str(&home, "worktrees"), &id),
                &alias,
            );
            if let Err(error) = create_isolated_worktree(&host, &source, &destination, &branch) {
                roll_back(&host, &created);
                return Err(error);
            }
            created.push((source.clone(), destination.clone(), branch.clone()));
            (destination, Some(branch))
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
        hub_path: hub,
        archived: false,
    };
    let disk_workspace = workspace.clone();
    let disk_members = members.clone();
    let disk_host = host.clone();
    let materialized = tauri::async_runtime::spawn_blocking(move || {
        materialize_hub(&disk_host, &disk_workspace, &disk_members)
    })
    .await
    .map_err(|_| "Could not prepare workspace directory".to_string())?;
    if let Err(error) = materialized {
        roll_back(&host, &created);
        return Err(error);
    }
    if let Err(error) = storage::create_workspace(&ctx.database(), &workspace, &members).await {
        roll_back(&host, &created);
        return Err(error);
    }
    bootstrap_cached(&ctx).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn update_workspace(
    input: UpdateWorkspaceInput,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<BootstrapData, String> {
    let ctx = state.ctx(&window);
    let name = input.name.trim();
    if name.is_empty() {
        return Err("Give the workspace a name".into());
    }
    let runtime = ctx.runtime();
    let host = runtime.host.clone();
    let home = runtime.codex_home_str();
    let requested = validate_members(&host, &input.members)?;
    let mut workspace = storage::read_workspaces(&ctx.database())
        .await?
        .into_iter()
        .find(|workspace| workspace.id == input.workspace_id && !workspace.archived)
        .ok_or("This workspace no longer exists or is archived")?;
    let old_members = storage::read_workspace_members(&ctx.database(), &workspace.id).await?;
    let old_by_source: HashMap<_, _> = old_members
        .iter()
        .map(|member| (member.source_path.as_str(), member))
        .collect();
    let mut created = Vec::<(String, String, String)>::new();
    let mut members = Vec::new();
    for (ordinal, (requested, source)) in requested.into_iter().enumerate() {
        let alias = clean_alias(&requested.alias)?;
        let source_path = source.clone();
        let old = old_by_source.get(source_path.as_str()).copied();
        let (effective_path, branch, isolated) = match old {
            Some(member) if member.isolated == requested.isolated => (
                member.effective_path.clone(),
                member.branch.clone(),
                member.isolated,
            ),
            _ if requested.isolated && is_git_repository(&host, &source) => {
                let branch = available_branch(&host, &source, &workspace.id, &alias);
                let destination = host.join_str(
                    &host.join_str(&host.join_str(&home, "worktrees"), &workspace.id),
                    &alias,
                );
                if let Err(error) = create_isolated_worktree(&host, &source, &destination, &branch)
                {
                    roll_back(&host, &created);
                    return Err(error);
                }
                created.push((source.clone(), destination.clone(), branch.clone()));
                (destination, Some(branch), true)
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

    let hub = workspace.hub_path.clone();
    // Remove only aliases that were managed and are no longer identical. If
    // any were replaced by a user file, fail before changing the database.
    for old in &old_members {
        let retained = members
            .iter()
            .any(|member| member.alias == old.alias && member.effective_path == old.effective_path);
        if !retained {
            if let Err(error) = remove_managed_link(&host, &hub, old) {
                roll_back(&host, &created);
                return Err(error);
            }
        }
    }
    workspace.name = name.to_string();
    let disk_workspace = workspace.clone();
    let disk_members = members.clone();
    let disk_host = host.clone();
    let materialized = tauri::async_runtime::spawn_blocking(move || {
        materialize_hub(&disk_host, &disk_workspace, &disk_members)
    })
    .await
    .map_err(|_| "Could not prepare workspace directory".to_string())?;
    if let Err(error) = materialized {
        // Recreate the old managed links when the update failed after a rename
        // or removal. User files remain untouched in either case.
        let old_workspace = workspace.clone();
        let old_for_disk = old_members.clone();
        let old_host = host.clone();
        let _ = tauri::async_runtime::spawn_blocking(move || {
            materialize_hub(&old_host, &old_workspace, &old_for_disk)
        })
        .await;
        roll_back(&host, &created);
        return Err(error);
    }
    if let Err(error) = storage::update_workspace(&ctx.database(), &workspace, &members).await {
        roll_back(&host, &created);
        return Err(error);
    }
    bootstrap_cached(&ctx).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn move_thread_to_workspace(
    thread_id: String,
    workspace_id: String,
    app: tauri::AppHandle,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<BootstrapData, String> {
    let ctx = state.ctx(&window);
    // Materialize and validate now, rather than saving a thread association
    // that will only fail at its next turn.
    runtime_for_workspace(&ctx, &workspace_id).await?;
    storage::assign_thread_workspace(&ctx.database(), &thread_id, &workspace_id).await?;
    crate::projects::server::assign_thread_to_workspace(&app, &ctx, &thread_id, &workspace_id)
        .await?;
    bootstrap_cached(&ctx).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

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
        assert!(validate_members(&Host::Native, &inputs).is_err());
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

        let host = Host::Native;
        assert!(validate_members(&host, &[member(&one, "one")]).is_err());
        assert!(validate_members(&host, &[member(&one, "same"), member(&two, "same")]).is_err());
        assert!(validate_members(&host, &[member(&one, "one"), member(&two, "two")]).is_ok());
    }
}
