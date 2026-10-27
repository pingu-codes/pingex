//! Tauri commands over the memories of every Home in the window's Profile.

use std::fs;
use std::path::PathBuf;

use tauri::State;

use super::{claude, codex, files, MemoryHome};
use crate::harness::HarnessKind;
use crate::storage::profiles::{self as registry, ProfileHome};
use crate::AppState;

async fn profile_homes(
    window: &tauri::WebviewWindow,
    state: &State<'_, AppState>,
) -> Result<Vec<ProfileHome>, String> {
    registry::homes(&state.ctx(window).database()).await
}

async fn profile_home(
    id: &str,
    window: &tauri::WebviewWindow,
    state: &State<'_, AppState>,
) -> Result<ProfileHome, String> {
    profile_homes(window, state)
        .await?
        .into_iter()
        .find(|home| home.id == id)
        .ok_or_else(|| "Home does not belong to this Profile".to_string())
}

fn read_home(home: &ProfileHome, projects: &[String]) -> MemoryHome {
    let host = &home.host;
    let local = host.to_local(&home.config_dir);
    let (stores, toggles) = match home.harness {
        HarnessKind::Codex => (
            codex::stores(host, &home.config_dir),
            codex::toggles(&local),
        ),
        HarnessKind::Claude => (
            claude::stores(host, &home.config_dir, projects),
            claude::toggles(&local),
        ),
        // maki's memory tool keeps its own store, not one this page edits.
        HarnessKind::Maki => (Vec::new(), Ok(Vec::new())),
    };
    let (toggles, error) = match toggles {
        Ok(toggles) => (toggles, None),
        Err(error) => (Vec::new(), Some(error)),
    };
    MemoryHome {
        home_id: home.id.clone(),
        label: home.label.clone(),
        config_dir: home.config_dir.clone(),
        toggles,
        stores,
        error,
    }
}

/// Local store roots a path must fall under to be read, edited or deleted.
fn local_roots(home: &ProfileHome) -> Vec<PathBuf> {
    match home.harness {
        HarnessKind::Codex => codex::roots(&home.host, &home.config_dir)
            .into_iter()
            .map(|(_, local, _)| local)
            .collect(),
        HarnessKind::Claude => claude::roots(&home.host, &home.config_dir)
            .into_iter()
            .map(|(_, local, _)| local)
            .collect(),
        HarnessKind::Maki => Vec::new(),
    }
}

async fn list(homes: Vec<ProfileHome>, projects: Vec<String>) -> Result<Vec<MemoryHome>, String> {
    // A WSL Home reads through the network share; keep that off the runtime.
    tauri::async_runtime::spawn_blocking(move || {
        homes
            .iter()
            .map(|home| read_home(home, &projects))
            .collect()
    })
    .await
    .map_err(|error| error.to_string())
}

/// The memories of every Home in the Profile. `projects` are the project
/// paths the window knows, used to name Claude's per-project stores.
#[tauri::command]
#[specta::specta]
pub(crate) async fn list_memories(
    projects: Vec<String>,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Vec<MemoryHome>, String> {
    list(profile_homes(&window, &state).await?, projects).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn read_memory(
    home_id: String,
    path: String,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let home = profile_home(&home_id, &window, &state).await?;
    let (file, _) = files::guard(&home.host.to_local(&path), &local_roots(&home))?;
    fs::read_to_string(&file).map_err(|error| format!("Could not read {path}: {error}"))
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn write_memory(
    home_id: String,
    path: String,
    contents: String,
    projects: Vec<String>,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Vec<MemoryHome>, String> {
    let homes = profile_homes(&window, &state).await?;
    let home = homes
        .iter()
        .find(|home| home.id == home_id)
        .ok_or("Home does not belong to this Profile")?;
    let (file, _) = files::guard(&home.host.to_local(&path), &local_roots(home))?;
    files::write_atomic(&file, &contents)?;
    list(homes, projects).await
}

/// Delete a memory. A Claude memory's line in its `MEMORY.md` goes with it.
#[tauri::command]
#[specta::specta]
pub(crate) async fn delete_memory(
    home_id: String,
    path: String,
    projects: Vec<String>,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Vec<MemoryHome>, String> {
    let homes = profile_homes(&window, &state).await?;
    let home = homes
        .iter()
        .find(|home| home.id == home_id)
        .ok_or("Home does not belong to this Profile")?;
    let (file, root) = files::guard(&home.host.to_local(&path), &local_roots(home))?;
    match home.harness {
        HarnessKind::Claude => claude::delete(&file, &root)?,
        HarnessKind::Maki => return Err("maki memories are not managed here".into()),
        HarnessKind::Codex => {
            fs::remove_file(&file).map_err(|error| format!("Could not delete {path}: {error}"))?
        }
    }
    list(homes, projects).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn set_memory_toggle(
    home_id: String,
    key: String,
    value: bool,
    projects: Vec<String>,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Vec<MemoryHome>, String> {
    let homes = profile_homes(&window, &state).await?;
    let home = homes
        .iter()
        .find(|home| home.id == home_id)
        .ok_or("Home does not belong to this Profile")?;
    let local = home.host.to_local(&home.config_dir);
    match home.harness {
        HarnessKind::Codex => codex::set_toggle(&local, &key, value)?,
        HarnessKind::Claude => claude::set_toggle(&local, &key, value)?,
        HarnessKind::Maki => return Err("maki memories are not managed here".into()),
    }
    list(homes, projects).await
}
