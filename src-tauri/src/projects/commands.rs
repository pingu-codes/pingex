//! Commands that edit the project list.
//!
//! Every mutation returns a freshly built `BootstrapData` rather than a partial
//! diff, so the sidebar re-renders from a single source of truth. They rebuild
//! from cache, since none of them change anything Codex knows about.

use serde_json::json;
use std::fs;
use tauri::{AppHandle, State};

use super::bootstrap::{bootstrap_cached, bootstrap_inner};
use super::types::BootstrapData;
use crate::storage::{self, SiblingRef, Store, StoredProject};
use crate::util::json::Json;
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
#[specta::specta]
pub(crate) async fn bootstrap(
    app: AppHandle,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<BootstrapData, String> {
    let ctx = state.ctx(&window);
    bootstrap_inner(&app, &ctx).await
}

/// Read the account's rolling rate-limit windows (5h / weekly). Codex also
/// pushes `account/rateLimits/updated` during turns; this is the cold-start
/// read so the usage meter is populated before the first turn.
#[tauri::command]
#[specta::specta]
pub(crate) async fn read_account_rate_limits(
    app: AppHandle,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Json, String> {
    let ctx = state.ctx(&window);
    ctx.session
        .request(&app, "account/rateLimits/read", json!({}))
        .await
        .map(Json)
}

/// Per-thread usage estimate: `account/usage/read` scoped by `threadId`.
#[tauri::command]
#[specta::specta]
pub(crate) async fn read_thread_usage(
    thread_id: String,
    app: AppHandle,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Json, String> {
    let ctx = state.ctx(&window);
    ctx.session
        .request(&app, "account/usage/read", json!({"threadId": thread_id}))
        .await
        .map(Json)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn add_project(
    path: String,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<BootstrapData, String> {
    let ctx = state.ctx(&window);
    let canonical =
        fs::canonicalize(&path).map_err(|error| format!("Could not open {path}: {error}"))?;
    if !canonical.is_dir() {
        return Err(format!("{} is not a folder", canonical.display()));
    }
    let mut store = storage::read_store(&ctx.database()).await?;
    stored_project_mut(&mut store, &canonical.display().to_string());
    storage::write_store(&ctx.database(), &store).await?;
    bootstrap_cached(&ctx).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn rename_project(
    path: String,
    name: String,
    app: AppHandle,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<BootstrapData, String> {
    let ctx = state.ctx(&window);
    let mut store = storage::read_store(&ctx.database()).await?;
    let trimmed = name.trim();
    // A blank name clears the override so the folder name is used again.
    let entry = stored_project_mut(&mut store, &path);
    entry.name = if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    };
    let server_name = entry.name.clone().unwrap_or_else(|| {
        std::path::Path::new(&path)
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or(&path)
            .to_string()
    });
    storage::write_store(&ctx.database(), &store).await?;
    super::server::rename(&app, &ctx, &path, &server_name).await;
    bootstrap_cached(&ctx).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn set_project_pinned(
    path: String,
    pinned: bool,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<BootstrapData, String> {
    let ctx = state.ctx(&window);
    let mut store = storage::read_store(&ctx.database()).await?;
    stored_project_mut(&mut store, &path).pinned = pinned;
    storage::write_store(&ctx.database(), &store).await?;
    bootstrap_cached(&ctx).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn set_project_archived(
    path: String,
    archived: bool,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<BootstrapData, String> {
    let ctx = state.ctx(&window);
    let mut store = storage::read_store(&ctx.database()).await?;
    stored_project_mut(&mut store, &path).archived = archived;
    storage::write_store(&ctx.database(), &store).await?;
    bootstrap_cached(&ctx).await
}

/// Persist a sidebar-only preference without rebuilding the project tree.
#[tauri::command]
#[specta::specta]
pub(crate) async fn set_project_expanded(
    path: String,
    expanded: bool,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let ctx = state.ctx(&window);
    storage::set_project_expanded(&ctx.database(), &path, expanded).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn create_sidebar_folder(
    scope: String,
    parent_id: Option<String>,
    name: String,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<BootstrapData, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("A folder needs a name".into());
    }
    let ctx = state.ctx(&window);
    storage::create_sidebar_folder(&ctx.database(), &scope, parent_id.as_deref(), name).await?;
    bootstrap_cached(&ctx).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn rename_sidebar_folder(
    id: String,
    name: String,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<BootstrapData, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("A folder needs a name".into());
    }
    let ctx = state.ctx(&window);
    storage::rename_sidebar_folder(&ctx.database(), &id, name).await?;
    bootstrap_cached(&ctx).await
}

/// Remove a folder; its contents move up to the folder's parent.
#[tauri::command]
#[specta::specta]
pub(crate) async fn delete_sidebar_folder(
    id: String,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<BootstrapData, String> {
    let ctx = state.ctx(&window);
    storage::delete_sidebar_folder(&ctx.database(), &id).await?;
    bootstrap_cached(&ctx).await
}

/// Persist a sidebar-only preference without rebuilding the project tree.
#[tauri::command]
#[specta::specta]
pub(crate) async fn set_sidebar_folder_expanded(
    id: String,
    expanded: bool,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let ctx = state.ctx(&window);
    storage::set_sidebar_folder_expanded(&ctx.database(), &id, expanded).await
}

/// The one drag-and-drop primitive: put `item` under `parent_id` and record
/// `siblings` as the full order of that parent's children. The frontend owns
/// the tree, so it sends the resulting order rather than an index.
#[tauri::command]
#[specta::specta]
pub(crate) async fn place_sidebar_item(
    scope: String,
    item: SiblingRef,
    parent_id: Option<String>,
    siblings: Vec<SiblingRef>,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<BootstrapData, String> {
    let ctx = state.ctx(&window);
    if let (true, Some(parent)) = (item.kind == "folder", parent_id.as_deref()) {
        let layout = storage::read_sidebar_layout(&ctx.database()).await?;
        if storage::is_folder_or_descendant(&layout.folders, &item.id, parent) {
            return Err("A folder cannot be moved inside itself".into());
        }
    }
    if !siblings.iter().any(|sibling| *sibling == item) {
        return Err("The moved item must be among its siblings".into());
    }
    storage::place_sidebar_item(
        &ctx.database(),
        &scope,
        &item,
        parent_id.as_deref(),
        &siblings,
    )
    .await?;
    bootstrap_cached(&ctx).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn set_thread_pinned(
    thread_id: String,
    pinned: bool,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<BootstrapData, String> {
    let ctx = state.ctx(&window);
    let mut store = storage::read_store(&ctx.database()).await?;
    store.pinned_threads.retain(|id| id != &thread_id);
    if pinned {
        store.pinned_threads.push(thread_id);
    }
    storage::write_store(&ctx.database(), &store).await?;
    bootstrap_cached(&ctx).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn remove_project(
    path: String,
    app: AppHandle,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<BootstrapData, String> {
    let ctx = state.ctx(&window);
    // Removing a project that a workspace still points at would leave the
    // workspace referencing something the sidebar no longer shows.
    if storage::read_all_workspace_members(&ctx.database())
        .await?
        .iter()
        .any(|member| member.source_path == path || member.effective_path == path)
    {
        return Err("Remove this project from its workspace before removing it from Pingex".into());
    }
    let mut store = storage::read_store(&ctx.database()).await?;
    store.projects.retain(|project| project.path != path);
    storage::write_store(&ctx.database(), &store).await?;
    storage::forget_sidebar_scope(&ctx.database(), &path).await?;
    // Its threads keep their Codex assignment until the project is gone
    // server-side too; otherwise they would vanish from the sidebar.
    super::server::delete(&app, &ctx, &path).await?;
    bootstrap_cached(&ctx).await
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
