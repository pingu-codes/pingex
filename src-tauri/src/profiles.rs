//! A window's project list spans Homes without rebinding the window or
//! changing a running conversation's runtime.

use crate::harness::driver::DriverRuntimes;
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
    let claude = profile.drivers.claude().runtime();
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

/// The driver runtimes a Home implies. A Home of one harness still carries
/// the others under their defaults, so a window bound to it can start any
/// conversation.
fn driver_runtimes_for(home: &ProfileHome) -> DriverRuntimes {
    DriverRuntimes {
        claude: crate::claude::driver::ClaudeRuntime {
            binary: PathBuf::from(if home.harness == HarnessKind::Claude {
                &home.binary
            } else {
                "claude"
            }),
            config_dir: (home.harness == HarnessKind::Claude)
                .then(|| PathBuf::from(&home.config_dir)),
            host: home.host.clone(),
        },
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
    let database = if home.shares_profile_db {
        // Existing Claude conversations retain their journal and native IDs.
        // The flag is stored rather than derived from the current Settings
        // default, so repointing that default cannot move a Home's history.
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
    let runtimes = driver_runtimes_for(home);
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
        runtimes,
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
    let homes = legacy_homes(profile).await?;
    for home in &homes {
        let context = context_for(state, profile, home).await?;
        // A Home with its own database lists a thread only under a project in
        // that database, so a Home registered without a copy of the project
        // list (legacy_homes never made one) would drop all its threads.
        if !home.shares_profile_db && context.home_key != profile.home_key {
            let codex = homes.iter().find(|candidate| {
                candidate.harness == HarnessKind::Codex
                    && candidate.host == home.host
                    && candidate.is_default
            });
            let source = match codex {
                Some(codex) if codex.id != home.id => {
                    Some(context_for(state, profile, codex).await?.database())
                }
                _ => (home.host == profile.host()).then(|| profile.database()),
            };
            if let Some(source) = source {
                seed_projects(&context.database(), &source).await?;
            }
        }
        let home = home.clone();
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
        // A context lists every thread in its database; each Home shows only
        // the ones its harness runs.
        let runs_here = |harness: &Option<String>| {
            harness.as_deref().unwrap_or(HarnessKind::Codex.as_str()) == home.harness.as_str()
        };
        for project in &mut data.projects {
            project.threads.retain(|thread| runs_here(&thread.harness));
        }
        data.subagents.retain(|thread| runs_here(&thread.harness));
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
        seed_projects(&target.database(), &source.database()).await?;
    }
    Ok(home)
}

/// Copy the projects (and their instructions) `source` lists but `target`
/// lacks. Threads are never copied. Idempotent.
async fn seed_projects(target: &turso::Database, source: &turso::Database) -> Result<(), String> {
    let seeds = storage::read_store(source).await?.projects;
    storage::update_store(target, |store| {
        for project in seeds {
            if !store
                .projects
                .iter()
                .any(|existing| existing.path == project.path)
            {
                store.projects.push(project);
            }
        }
    })
    .await?;
    let existing = storage::read_all_project_instructions(target).await?;
    for (path, instructions) in storage::read_all_project_instructions(source).await? {
        if !existing.iter().any(|(key, _)| key == &path) {
            storage::write_project_instructions(target, &path, &instructions).await?;
        }
    }
    Ok(())
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

/// The Home `legacy_homes` re-registers from the Settings override cannot be
/// edited or removed here: it would come back on the next launch. Send the
/// reader to the field that actually owns it.
fn guard_settings_home(home: &ProfileHome) -> Result<(), String> {
    if home.harness != HarnessKind::Claude {
        return Ok(());
    }
    let runtime = crate::claude::driver::ClaudeRuntime::resolve(
        &crate::settings::prefs::read_overrides(&crate::settings::prefs::settings_path()),
    );
    // `legacy_homes` also pins a `~/.claude` Home on the Codex Host when the
    // two Hosts differ; that one can still reappear after a removal.
    if home.host == runtime.host && home.config_dir == runtime.config_dir_str() {
        return Err(format!(
            "{} is the Claude configuration directory in Settings. Change it there first.",
            home.config_dir
        ));
    }
    Ok(())
}

/// The Home as the registry holds it, refusing one that is mid-conversation:
/// repointing a Home tears its context down, and that kills a running turn.
async fn settled_home(
    state: &AppState,
    profile: &HomeContext,
    id: &str,
) -> Result<ProfileHome, String> {
    let home = registry::homes(&profile.database())
        .await?
        .into_iter()
        .find(|home| home.id == id)
        .ok_or("Home does not belong to this Profile")?;
    guard_settings_home(&home)?;
    if let Some(context) = state.context_for_home(&context_key(profile, &home)) {
        if context.drivers.is_busy() {
            return Err("A conversation is running in this Home. Stop it first.".into());
        }
    }
    Ok(home)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn update_profile_home(
    id: String,
    config_dir: String,
    binary: String,
    label: String,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<ProfileHome, String> {
    let profile = state.ctx(&window);
    let home = settled_home(&state, &profile, &id).await?;
    let (_, config_dir) =
        crate::settings::commands::settle_home(&config_dir, Some(home.host.clone()))?;
    if config_dir.is_empty() || binary.trim().is_empty() {
        return Err("Enter a Home directory and binary".into());
    }
    if home.host.resolve_binary(binary.trim()).is_none() {
        return Err(crate::codex::binary::missing_message_on(
            &home.host,
            binary.trim(),
        ));
    }
    let updated = registry::update_home(
        &profile.database(),
        &id,
        &home.host.canonical(&config_dir),
        binary.trim(),
        label.trim(),
    )
    .await?;
    // The cached context still holds the old runtime; the next use rebuilds it.
    if let Some(stale) = state.forget_context(&context_key(&profile, &home)) {
        stale.shutdown();
    }
    Ok(updated)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn remove_profile_home(
    id: String,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let profile = state.ctx(&window);
    let home = settled_home(&state, &profile, &id).await?;
    registry::delete_home(&profile.database(), &id).await?;
    // The Home's cache database stays on disk, so re-adding the directory
    // brings its conversations back.
    if let Some(stale) = state.forget_context(&context_key(&profile, &home)) {
        stale.shutdown();
    }
    Ok(())
}

/// Whether one Home's Claude CLI is installed and logged in. Probed per Home
/// rather than folded into the bootstrap snapshot: `status` shells out twice
/// and a WSL Home's probe can hang, which is not worth putting on the launch
/// path for something only the Settings list reads.
#[tauri::command]
#[specta::specta]
pub(crate) async fn read_home_claude_status(
    id: String,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<crate::claude::driver::ClaudeStatus, String> {
    let profile = state.ctx(&window);
    let home = registry::homes(&profile.database())
        .await?
        .into_iter()
        .find(|home| home.id == id)
        .ok_or("Home does not belong to this Profile")?;
    if home.harness != HarnessKind::Claude {
        return Err("This Home does not run Claude Code".into());
    }
    let runtime = driver_runtimes_for(&home).claude;
    tauri::async_runtime::spawn_blocking(move || crate::claude::driver::status(&runtime))
        .await
        .map_err(|error| error.to_string())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::StoredProject;

    fn project(path: &str) -> StoredProject {
        StoredProject {
            path: path.into(),
            name: None,
            pinned: false,
            archived: false,
            worktree: false,
        }
    }

    #[tokio::test]
    async fn seeding_fills_a_home_database_that_has_no_projects() {
        let directory = tempfile::tempdir().unwrap();
        let source = storage::open_profile_cache(&directory.path().join("a/pingex.db"))
            .await
            .unwrap();
        let target = storage::open_profile_cache(&directory.path().join("b/pingex.db"))
            .await
            .unwrap();
        let mut store = storage::read_store(&source).await.unwrap();
        store.projects = vec![project("/work/one"), project("/work/two")];
        storage::write_store(&source, &store).await.unwrap();
        storage::write_project_instructions(&source, "/work/one", "be brief")
            .await
            .unwrap();
        let mut existing = storage::read_store(&target).await.unwrap();
        existing.projects = vec![project("/work/two")];
        storage::write_store(&target, &existing).await.unwrap();

        seed_projects(&target, &source).await.unwrap();
        seed_projects(&target, &source).await.unwrap();

        let paths: Vec<_> = storage::read_store(&target)
            .await
            .unwrap()
            .projects
            .into_iter()
            .map(|project| project.path)
            .collect();
        assert_eq!(paths, ["/work/two", "/work/one"]);
        assert_eq!(
            storage::read_all_project_instructions(&target)
                .await
                .unwrap(),
            [("/work/one".to_string(), "be brief".to_string())]
        );
    }
}
