//! A window's project list spans Homes without rebinding the window or
//! changing a running conversation's runtime.

use crate::harness::HarnessKind;
use crate::projects::types::BootstrapData;
use crate::storage::{
    self,
    profiles::{self as registry, ProfileHome},
};
use crate::util::host::Host;
use crate::{AppState, HomeContext, RuntimeConfig};
use serde::Serialize;
use std::{path::PathBuf, sync::Arc};
use tauri::{AppHandle, State};

#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HomeSnapshot {
    pub home: ProfileHome,
    pub home_key: String,
    pub data: BootstrapData,
    pub error: Option<String>,
}

#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProfileBootstrap {
    pub profile_key: String,
    pub homes: Vec<HomeSnapshot>,
}

async fn legacy_homes(profile: &HomeContext) -> Result<Vec<ProfileHome>, String> {
    let database = profile.database();
    registry::initialize(&database).await?;
    let runtime = profile.runtime();
    registry::register_home(
        &database,
        HarnessKind::Codex,
        &runtime.host,
        &runtime.codex_home_str(),
        &runtime.codex_binary_str(),
        "Codex",
    )
    .await?;
    let claude = profile.claude.runtime();
    registry::register_home(
        &database,
        HarnessKind::Claude,
        &claude.host,
        &claude.config_dir().to_string_lossy(),
        &claude.binary.to_string_lossy(),
        "Claude Code",
    )
    .await?;
    if claude.host != runtime.host {
        if let Some(directory) = runtime.host.home_dir() {
            registry::register_home(
                &database,
                HarnessKind::Claude,
                &runtime.host,
                &runtime.host.join_str(&directory, ".claude"),
                "claude",
                "Claude Code",
            )
            .await?;
        }
    }
    registry::homes(&database).await
}

fn context_key(profile: &HomeContext, home: &ProfileHome) -> String {
    let runtime = profile.runtime();
    if home.harness == HarnessKind::Codex
        && home.host == runtime.host
        && home.config_dir == runtime.codex_home_str()
    {
        profile.home_key.clone()
    } else {
        format!("profile:{}:{}", profile.home_key, home.id)
    }
}

async fn context_for(
    state: &AppState,
    profile: &HomeContext,
    home: &ProfileHome,
) -> Result<Arc<HomeContext>, String> {
    let key = context_key(profile, home);
    if let Some(context) = state.context_for_home(&key) {
        return Ok(context);
    }
    let legacy_claude = profile.claude.runtime();
    let database = if home.harness == HarnessKind::Claude
        && home.host == legacy_claude.host
        && home.config_dir == legacy_claude.config_dir().to_string_lossy()
    {
        // Existing Claude conversations retain their journal and native IDs.
        profile.database()
    } else {
        let root = state.profile_root_path(&profile.host(), &profile.runtime().codex_home_str());
        let path = root
            .parent()
            .ok_or("Profile path has no parent")?
            .join("homes")
            .join(&home.id)
            .join("pingex.db");
        storage::open_profile_cache(&path).await?
    };
    let claude = crate::claude::driver::ClaudeRuntime {
        binary: PathBuf::from(if home.harness == HarnessKind::Claude {
            &home.binary
        } else {
            "claude"
        }),
        config_dir: if home.harness == HarnessKind::Claude {
            Some(PathBuf::from(&home.config_dir))
        } else {
            None
        },
        host: home.host.clone(),
    };
    let context = HomeContext::in_profile(
        RuntimeConfig {
            host: home.host.clone(),
            codex_home: PathBuf::from(&home.config_dir),
            codex_binary: PathBuf::from(if home.harness == HarnessKind::Codex {
                &home.binary
            } else {
                "codex"
            }),
        },
        database,
        key,
        profile.profile_key.clone(),
        claude,
        Some(home.harness),
    );
    Ok(state.insert_context(context))
}

async fn snapshot(
    app: &AppHandle,
    state: &AppState,
    profile: &HomeContext,
    refresh: bool,
) -> Result<ProfileBootstrap, String> {
    let mut snapshots = Vec::new();
    for home in legacy_homes(profile).await? {
        let context = context_for(state, profile, &home).await?;
        let (mut data, error) = if refresh && home.harness == HarnessKind::Codex {
            match crate::projects::bootstrap::bootstrap_inner(app, &context).await {
                Ok(data) => (data, None),
                Err(error) => (
                    crate::projects::bootstrap::bootstrap_cached(&context).await?,
                    Some(error),
                ),
            }
        } else {
            (
                crate::projects::bootstrap::bootstrap_cached(&context).await?,
                None,
            )
        };
        let is_claude = home.harness == HarnessKind::Claude;
        for project in &mut data.projects {
            project
                .threads
                .retain(|thread| (thread.harness.as_deref() == Some("claude")) == is_claude);
        }
        data.subagents
            .retain(|thread| (thread.harness.as_deref() == Some("claude")) == is_claude);
        snapshots.push(HomeSnapshot {
            home_key: context.home_key.clone(),
            home,
            data,
            error,
        });
    }
    Ok(ProfileBootstrap {
        profile_key: profile.profile_key.clone(),
        homes: snapshots,
    })
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn bootstrap_profile(
    refresh: bool,
    app: AppHandle,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<ProfileBootstrap, String> {
    let profile = state.ctx(&window);
    snapshot(&app, &state, &profile, refresh).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn register_profile_home(
    harness: HarnessKind,
    host: Host,
    config_dir: String,
    binary: String,
    label: String,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<ProfileHome, String> {
    let profile = state.ctx(&window);
    legacy_homes(&profile).await?;
    let (host, config_dir) = crate::settings::commands::settle_home(&config_dir, Some(host))?;
    if config_dir.is_empty() || binary.trim().is_empty() {
        return Err("Enter a Home directory and binary".into());
    }
    let home = registry::register_home(
        &profile.database(),
        harness,
        &host,
        &host.canonical(&config_dir),
        binary.trim(),
        label.trim(),
    )
    .await?;
    let target = context_for(&state, &profile, &home).await?;
    // A new account starts with this Host's project metadata, never its threads.
    if let Some(source_home) =
        registry::homes(&profile.database())
            .await?
            .iter()
            .find(|candidate| {
                candidate.host == home.host && candidate.id != home.id && candidate.is_default
            })
    {
        let source = context_for(&state, &profile, source_home).await?;
        let mut store = storage::read_store(&target.database()).await?;
        for project in storage::read_store(&source.database()).await?.projects {
            if !store
                .projects
                .iter()
                .any(|existing| existing.path == project.path)
            {
                store.projects.push(project);
            }
        }
        storage::write_store(&target.database(), &store).await?;
        let existing = storage::read_all_project_instructions(&target.database()).await?;
        for (path, instructions) in
            storage::read_all_project_instructions(&source.database()).await?
        {
            if !existing.iter().any(|(key, _)| key == &path) {
                storage::write_project_instructions(&target.database(), &path, &instructions)
                    .await?;
            }
        }
    }
    Ok(home)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn set_profile_default_home(
    id: String,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    registry::set_default_home(&state.ctx(&window).database(), &id).await
}

/// A workspace's directories belong to its Host. Another harness on that
/// Host uses the same hub and worktrees, with its own conversation records.
#[tauri::command]
#[specta::specta]
pub(crate) async fn prepare_profile_workspace(
    source_home_key: String,
    workspace_id: String,
    target_home_key: String,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let source = state.context_for_route(window.label(), Some(&source_home_key))?;
    let target = state.context_for_route(window.label(), Some(&target_home_key))?;
    if source.host() != target.host() {
        return Err("A workspace can only contain projects on the same Host".into());
    }
    if source.home_key == target.home_key {
        return Ok(());
    }
    static COPYING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _copying = COPYING.lock().await;
    let workspace = storage::read_workspaces(&source.database())
        .await?
        .into_iter()
        .find(|workspace| workspace.id == workspace_id)
        .ok_or("Workspace does not exist in its Home")?;
    let members = storage::read_workspace_members(&source.database(), &workspace_id).await?;
    if storage::read_workspaces(&target.database())
        .await?
        .iter()
        .any(|existing| existing.id == workspace_id)
    {
        storage::update_workspace(&target.database(), &workspace, &members).await
    } else {
        storage::create_workspace(&target.database(), &workspace, &members).await
    }
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn add_profile_project(
    path: String,
    host: Option<Host>,
    app: AppHandle,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<ProfileBootstrap, String> {
    let profile = state.ctx(&window);
    let (host, path) = match host {
        Some(host) => {
            let path = host.to_host_path(path.trim());
            (host, path)
        }
        None => Host::from_local(path.trim()),
    };
    let path = crate::projects::commands::canonical_project_path(&host, &path)?;
    let mut homes = legacy_homes(&profile).await?;
    for harness in [HarnessKind::Codex, HarnessKind::Claude] {
        if !homes
            .iter()
            .any(|home| home.host == host && home.harness == harness)
        {
            let directory = host
                .home_dir()
                .ok_or("Could not read the selected Host's home directory")?;
            let (folder, binary, label) = match harness {
                HarnessKind::Codex => (".codex", "codex", "Codex"),
                HarnessKind::Claude => (".claude", "claude", "Claude Code"),
            };
            homes.push(
                registry::register_home(
                    &profile.database(),
                    harness,
                    &host,
                    &host.join_str(&directory, folder),
                    binary,
                    label,
                )
                .await?,
            );
        }
    }
    registry::register_project(&profile.database(), &host, &path).await?;
    for home in homes.iter().filter(|home| home.host == host) {
        let context = context_for(&state, &profile, home).await?;
        crate::projects::commands::add_project_in(&context, &path).await?;
    }
    snapshot(&app, &state, &profile, false).await
}
