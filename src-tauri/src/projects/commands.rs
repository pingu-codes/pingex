//! Commands that edit the project list.
//!
//! Every mutation returns a freshly built `BootstrapData` rather than a partial
//! diff, so the sidebar re-renders from a single source of truth. They rebuild
//! from cache, since none of them change anything Codex knows about.

use serde_json::{json, Value};
use std::fs;
use tauri::{AppHandle, State};

use super::bootstrap::{bootstrap_cached, bootstrap_inner};
use super::types::BootstrapData;
use crate::storage::{self, Store, StoredProject};
use crate::AppState;

/// The stored entry for `path`, inserting a default one if it is new.
fn stored_project_mut<'a>(store: &'a mut Store, path: &str) -> &'a mut StoredProject {
    if let Some(index) = store
        .projects
        .iter()
        .position(|project| project.path == path)
    {
        return &mut store.projects[index];
    }
    store.projects.push(StoredProject {
        path: path.to_string(),
        name: None,
        pinned: false,
        archived: false,
    });
    store
        .projects
        .last_mut()
        .expect("a project was inserted immediately before lookup")
}

#[tauri::command]
pub(crate) async fn bootstrap(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<BootstrapData, String> {
    bootstrap_inner(&app, &state).await
}

/// Read the account's rolling rate-limit windows (5h / weekly). Codex also
/// pushes `account/rateLimits/updated` during turns; this is the cold-start
/// read so the usage meter is populated before the first turn.
#[tauri::command]
pub(crate) async fn read_account_rate_limits(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    state
        .session
        .request(&app, "account/rateLimits/read", json!({}))
        .await
}

#[tauri::command]
pub(crate) async fn add_project(
    path: String,
    state: State<'_, AppState>,
) -> Result<BootstrapData, String> {
    let canonical =
        fs::canonicalize(&path).map_err(|error| format!("Could not open {path}: {error}"))?;
    if !canonical.is_dir() {
        return Err(format!("{} is not a folder", canonical.display()));
    }
    let mut store = storage::read_store(&state.database()).await?;
    stored_project_mut(&mut store, &canonical.display().to_string());
    storage::write_store(&state.database(), &store).await?;
    bootstrap_cached(&state).await
}

#[tauri::command]
pub(crate) async fn rename_project(
    path: String,
    name: String,
    state: State<'_, AppState>,
) -> Result<BootstrapData, String> {
    let mut store = storage::read_store(&state.database()).await?;
    let trimmed = name.trim();
    // A blank name clears the override so the folder name is used again.
    stored_project_mut(&mut store, &path).name = if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    };
    storage::write_store(&state.database(), &store).await?;
    bootstrap_cached(&state).await
}

#[tauri::command]
pub(crate) async fn set_project_pinned(
    path: String,
    pinned: bool,
    state: State<'_, AppState>,
) -> Result<BootstrapData, String> {
    let mut store = storage::read_store(&state.database()).await?;
    stored_project_mut(&mut store, &path).pinned = pinned;
    storage::write_store(&state.database(), &store).await?;
    bootstrap_cached(&state).await
}

#[tauri::command]
pub(crate) async fn set_project_archived(
    path: String,
    archived: bool,
    state: State<'_, AppState>,
) -> Result<BootstrapData, String> {
    let mut store = storage::read_store(&state.database()).await?;
    stored_project_mut(&mut store, &path).archived = archived;
    storage::write_store(&state.database(), &store).await?;
    bootstrap_cached(&state).await
}

#[tauri::command]
pub(crate) async fn set_thread_pinned(
    thread_id: String,
    pinned: bool,
    state: State<'_, AppState>,
) -> Result<BootstrapData, String> {
    let mut store = storage::read_store(&state.database()).await?;
    store.pinned_threads.retain(|id| id != &thread_id);
    if pinned {
        store.pinned_threads.push(thread_id);
    }
    storage::write_store(&state.database(), &store).await?;
    bootstrap_cached(&state).await
}

#[tauri::command]
pub(crate) async fn remove_project(
    path: String,
    state: State<'_, AppState>,
) -> Result<BootstrapData, String> {
    // Removing a project that a workspace still points at would leave the
    // workspace referencing something the sidebar no longer shows.
    if storage::read_all_workspace_members(&state.database())
        .await?
        .iter()
        .any(|member| member.source_path == path || member.effective_path == path)
    {
        return Err("Remove this project from its workspace before removing it from Pingex".into());
    }
    let mut store = storage::read_store(&state.database()).await?;
    store.projects.retain(|project| project.path != path);
    storage::write_store(&state.database(), &store).await?;
    bootstrap_cached(&state).await
}

/// Move a project one place up or down. Pinned and unpinned projects form two
/// separate groups, so a swap across the boundary is ignored rather than
/// silently unpinning something.
#[tauri::command]
pub(crate) async fn move_project(
    path: String,
    direction: i32,
    state: State<'_, AppState>,
) -> Result<BootstrapData, String> {
    let mut store = storage::read_store(&state.database()).await?;
    if let Some(index) = store
        .projects
        .iter()
        .position(|project| project.path == path)
    {
        let target = index as i64 + i64::from(direction.signum());
        if target >= 0 && (target as usize) < store.projects.len() {
            let target = target as usize;
            if store.projects[target].pinned == store.projects[index].pinned {
                store.projects.swap(index, target);
                storage::write_store(&state.database(), &store).await?;
            }
        }
    }
    bootstrap_cached(&state).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looking_up_a_project_inserts_it_once() {
        let mut store = Store::default();
        stored_project_mut(&mut store, "/a").pinned = true;
        stored_project_mut(&mut store, "/a").archived = true;
        stored_project_mut(&mut store, "/b");

        assert_eq!(store.projects.len(), 2);
        assert!(store.projects[0].pinned && store.projects[0].archived);
        assert_eq!(store.projects[1].path, "/b");
    }
}
