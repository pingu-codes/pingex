//! Mirroring the sidebar to app-server projects (`project/*`, experimental in
//! Codex ≥0.149; absent from 0.146).
//!
//! The sidebar stays path-keyed and local — it works the same against a Codex
//! that has never heard of projects. What the server adds is a durable
//! assignment: a thread carries `projectId`, so it stays under the project it
//! was started in even when its cwd drifts (a temporary worktree, a moved
//! checkout) and other Codex clients see the same grouping. Every sidebar
//! entry is imported as a server project tagged with its local key in
//! `metadata`, and [`sync`] keeps the id ↔ key mapping on disk for the cached
//! bootstrap path.
//!
//! Everything here is best-effort: a refusal (older Codex, capability not
//! granted) leaves the mapping empty and the cwd-based grouping in charge.

use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use tauri::AppHandle;

use crate::codex::compat::Feature;
use crate::codex::requests;
use crate::storage::{self, Store, StoredWorkspace, StoredWorkspaceMember};
use crate::util::json::{arr_or_empty, str_at};
use crate::HomeContext;

/// The `metadata` entry that ties a server project back to a sidebar entry.
const KEY_METADATA: &str = "pingex.key";

/// One sidebar entry as the server should know it.
pub(crate) struct LocalProject {
    /// Project path, or a workspace's hub path — the same key the local
    /// tables (instructions, expansion) use.
    pub(crate) key: String,
    pub(crate) name: String,
    pub(crate) roots: Vec<String>,
    /// Threads the cwd rules currently file under this entry, handed to
    /// `project/import` so the first sync adopts them.
    pub(crate) threads: Vec<String>,
}

/// The sidebar's folders and workspaces as [`LocalProject`]s, with each
/// top-level thread claimed by the entry whose key is the longest prefix of
/// its cwd (workspace membership wins outright).
pub(crate) fn local_projects(
    store: &Store,
    workspaces: &[StoredWorkspace],
    members: &[StoredWorkspaceMember],
    workspace_threads: &HashMap<String, String>,
    threads: &[(String, String)],
) -> Vec<LocalProject> {
    let mut locals: Vec<LocalProject> = store
        .projects
        .iter()
        .filter(|project| !project.archived)
        .map(|project| LocalProject {
            key: project.path.clone(),
            name: project.name.clone().unwrap_or_else(|| folder_name(&project.path)),
            roots: vec![project.path.clone()],
            threads: Vec::new(),
        })
        .collect();
    for workspace in workspaces.iter().filter(|workspace| !workspace.archived) {
        let mut roots: Vec<String> = members
            .iter()
            .filter(|member| member.workspace_id == workspace.id)
            .map(|member| member.effective_path.clone())
            .collect();
        if roots.is_empty() {
            roots.push(workspace.hub_path.clone());
        }
        locals.push(LocalProject {
            key: workspace.hub_path.clone(),
            name: workspace.name.clone(),
            roots,
            threads: Vec::new(),
        });
    }
    let hub_by_workspace: HashMap<&str, &str> = workspaces
        .iter()
        .map(|workspace| (workspace.id.as_str(), workspace.hub_path.as_str()))
        .collect();
    for (thread_id, cwd) in threads {
        let key = workspace_threads
            .get(thread_id)
            .and_then(|workspace_id| hub_by_workspace.get(workspace_id.as_str()).copied())
            .map(str::to_string)
            .or_else(|| key_for_cwd(locals.iter().map(|local| local.key.as_str()), cwd));
        if let Some(key) = key {
            if let Some(local) = locals.iter_mut().find(|local| local.key == key) {
                local.threads.push(thread_id.clone());
            }
        }
    }
    locals
}

/// The local key whose path is the longest prefix of `cwd`, if any.
pub(crate) fn key_for_cwd<'a>(keys: impl Iterator<Item = &'a str>, cwd: &str) -> Option<String> {
    keys.filter(|key| Path::new(cwd).starts_with(Path::new(key)))
        .max_by_key(|key| key.len())
        .map(str::to_string)
}

fn folder_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(path)
        .to_string()
}

/// Reconcile the server's projects with the sidebar: read what it has, import
/// every local entry it does not, and persist the resulting
/// `server id → local key` mapping. Returns that mapping — empty when this
/// Codex has no project APIs.
pub(crate) async fn sync(
    app: &AppHandle,
    ctx: &HomeContext,
    locals: &[LocalProject],
) -> Result<HashMap<String, String>, String> {
    let mut mapping: HashMap<String, String> = HashMap::new();
    let mut cursor: Option<String> = None;
    loop {
        let page = match ctx
            .session
            .send_gated(
                app,
                Feature::PROJECTS,
                requests::project_list(cursor.as_deref()),
                |_| None,
            )
            .await
        {
            Ok(page) => page,
            Err(error) if error.starts_with(Feature::PROJECTS.error_prefix) => {
                storage::replace_server_projects(&ctx.database(), &mapping).await?;
                return Ok(mapping);
            }
            Err(error) => return Err(error),
        };
        for project in arr_or_empty(&page, "data") {
            let key = project
                .get("metadata")
                .and_then(|metadata| str_at(metadata, KEY_METADATA));
            if let (Some(id), Some(key)) = (str_at(project, "id"), key) {
                mapping.insert(id.to_string(), key.to_string());
            }
        }
        cursor = str_at(&page, "nextCursor").map(str::to_string);
        if cursor.is_none() {
            break;
        }
    }

    let mirrored: HashSet<String> = mapping.values().cloned().collect();
    for local in locals.iter().filter(|local| !mirrored.contains(&local.key)) {
        // One entry failing to import (a root that no longer exists, say) must
        // not take the whole bootstrap down with it.
        let imported = ctx
            .session
            .send(
                app,
                requests::project_import(
                    &local.name,
                    &local.roots,
                    json!({KEY_METADATA: local.key}),
                    &local.threads,
                    &local.key,
                ),
            )
            .await;
        match imported {
            Ok(response) => {
                if let Some(id) = response.get("project").and_then(|project| str_at(project, "id")) {
                    mapping.insert(id.to_string(), local.key.clone());
                }
            }
            Err(error) => eprintln!("could not mirror {} to a Codex project: {error}", local.key),
        }
    }
    storage::replace_server_projects(&ctx.database(), &mapping).await?;
    Ok(mapping)
}

/// The server project standing for `key`, if the sidebar entry was mirrored.
pub(crate) async fn project_id_for(ctx: &HomeContext, key: &str) -> Result<Option<String>, String> {
    Ok(storage::read_server_projects(&ctx.database())
        .await?
        .into_iter()
        .find(|(_, local)| local == key)
        .map(|(id, _)| id))
}

/// File `thread_id` under the project mirrored from `key` (or under none).
/// Silently does nothing when the entry is not mirrored — the next sync's
/// import adopts the thread by cwd instead.
pub(crate) async fn assign_thread(
    app: &AppHandle,
    ctx: &HomeContext,
    thread_id: &str,
    key: Option<&str>,
) -> Result<(), String> {
    let mapping = storage::read_server_projects(&ctx.database()).await?;
    if mapping.is_empty() {
        return Ok(());
    }
    let project_id = key.and_then(|key| {
        mapping
            .iter()
            .find(|(_, local)| local.as_str() == key)
            .map(|(id, _)| id.clone())
    });
    if key.is_some() && project_id.is_none() {
        return Ok(());
    }
    if let Err(error) = ctx
        .session
        .send(app, requests::thread_set_project(thread_id, project_id.as_deref()))
        .await
    {
        eprintln!("could not file thread {thread_id} under a Codex project: {error}");
    }
    Ok(())
}

/// File `thread_id` under the project mirrored from a workspace.
pub(crate) async fn assign_thread_to_workspace(
    app: &AppHandle,
    ctx: &HomeContext,
    thread_id: &str,
    workspace_id: &str,
) -> Result<(), String> {
    let hub_path = storage::read_workspaces(&ctx.database())
        .await?
        .into_iter()
        .find(|workspace| workspace.id == workspace_id)
        .map(|workspace| workspace.hub_path);
    assign_thread(app, ctx, thread_id, hub_path.as_deref()).await
}

/// Keep the server project's name in step with a sidebar rename.
pub(crate) async fn rename(app: &AppHandle, ctx: &HomeContext, key: &str, name: &str) {
    let Ok(Some(project_id)) = project_id_for(ctx, key).await else {
        return;
    };
    if let Err(error) = ctx
        .session
        .send(app, requests::project_update(&project_id, Some(name), None))
        .await
    {
        eprintln!("could not rename Codex project for {key}: {error}");
    }
}

/// Drop the server project mirrored from `key` along with the mapping.
pub(crate) async fn delete(app: &AppHandle, ctx: &HomeContext, key: &str) -> Result<(), String> {
    if let Some(project_id) = project_id_for(ctx, key).await? {
        if let Err(error) = ctx
            .session
            .send(app, requests::project_delete(&project_id))
            .await
        {
            eprintln!("could not delete Codex project for {key}: {error}");
        }
    }
    storage::remove_server_project(&ctx.database(), key).await
}

/// Pull the mapping into a form `build_bootstrap` can index: only entries whose
/// key is still a sidebar entry count, so a thread filed under a project the
/// user has since removed falls back to its cwd.
pub(crate) fn assigned_key<'a>(
    thread_project_id: Option<&str>,
    server_projects: &'a HashMap<String, String>,
    known_keys: &HashSet<String>,
) -> Option<&'a str> {
    let key = server_projects.get(thread_project_id?)?;
    known_keys.contains(key).then_some(key.as_str())
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
        }
    }

    #[test]
    fn claims_each_thread_for_the_longest_matching_entry() {
        let store = Store {
            projects: vec![project("/repo"), project("/repo/packages/web")],
            pinned_threads: Vec::new(),
        };
        let locals = local_projects(
            &store,
            &[],
            &[],
            &HashMap::new(),
            &[
                ("root".into(), "/repo".into()),
                ("web".into(), "/repo/packages/web/src".into()),
                ("elsewhere".into(), "/other".into()),
            ],
        );
        assert_eq!(locals[0].name, "repo");
        assert_eq!(locals[0].threads, vec!["root".to_string()]);
        assert_eq!(locals[1].threads, vec!["web".to_string()]);
    }

    #[test]
    fn workspaces_become_multi_root_projects_that_own_their_threads() {
        let store = Store {
            projects: vec![project("/repo/api")],
            pinned_threads: Vec::new(),
        };
        let workspace = StoredWorkspace {
            id: "ws".into(),
            name: "API + Web".into(),
            hub_path: "/hub".into(),
            archived: false,
        };
        let member = StoredWorkspaceMember {
            workspace_id: "ws".into(),
            source_path: "/repo/api".into(),
            effective_path: "/repo/api".into(),
            alias: "api".into(),
            isolated: false,
            branch: None,
            ordinal: 0,
        };
        let locals = local_projects(
            &store,
            &[workspace],
            &[member],
            &HashMap::from([("ws-thread".to_string(), "ws".to_string())]),
            &[
                ("ws-thread".into(), "/repo/api".into()),
                ("plain".into(), "/repo/api".into()),
            ],
        );
        let hub = locals.iter().find(|local| local.key == "/hub").unwrap();
        assert_eq!(hub.roots, vec!["/repo/api".to_string()]);
        assert_eq!(hub.threads, vec!["ws-thread".to_string()]);
        let api = locals.iter().find(|local| local.key == "/repo/api").unwrap();
        assert_eq!(api.threads, vec!["plain".to_string()]);
    }

    #[test]
    fn an_assignment_only_counts_for_entries_still_in_the_sidebar() {
        let mapping = HashMap::from([
            ("srv-1".to_string(), "/repo".to_string()),
            ("srv-gone".to_string(), "/removed".to_string()),
        ]);
        let known = HashSet::from(["/repo".to_string()]);
        assert_eq!(assigned_key(Some("srv-1"), &mapping, &known), Some("/repo"));
        assert_eq!(assigned_key(Some("srv-gone"), &mapping, &known), None);
        assert_eq!(assigned_key(Some("unknown"), &mapping, &known), None);
        assert_eq!(assigned_key(None, &mapping, &known), None);
    }
}
