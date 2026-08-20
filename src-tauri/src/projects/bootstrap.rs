//! Assembling the payload the app opens with.
//!
//! Two entry points, one builder. `bootstrap_inner` asks the app-server for the
//! live thread list and refreshes the local caches; `bootstrap_cached` reads
//! those caches instead, so a mutation that only changed local state (renaming a
//! project, pinning a thread) re-renders without a round trip.

use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use tauri::AppHandle;

use super::summary::{descendant_counts, thread_search_row, thread_summary_from};
use super::types::{
    Account, BootstrapData, BootstrapExtras, Project, ThreadSummary, WorkspaceMember,
};
use super::worktrees::{
    discover_worktrees, is_temp_worktree_path, is_worktree_path, worktree_parent_project,
};
use crate::storage::{
    self, SideQuestion, Store, StoredProject, StoredProjectSource, StoredThreadSummary,
    StoredWorkspaceMember,
};
use crate::util::json::{arr_or_empty, str_at};
use crate::{HomeContext, RuntimeConfig};

/// How many threads a single bootstrap will page through before stopping.
const MAX_THREADS: usize = 1000;
/// Page size for `thread/list`.
const THREAD_PAGE: usize = 200;

pub(crate) fn account_from(value: &Value) -> Option<Account> {
    let account = value.get("account")?.as_object()?;
    let kind = account
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let label = account
        .get("email")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| match kind.as_str() {
            "apiKey" => "API key account".into(),
            "amazonBedrock" => "Amazon Bedrock".into(),
            _ => "Codex user".into(),
        });
    let plan = account
        .get("planType")
        .and_then(Value::as_str)
        .map(str::to_string);
    Some(Account { label, plan, kind })
}

/// Fetch the live thread list, refresh the summary and search caches from it,
/// then build the payload.
pub(crate) async fn bootstrap_inner(
    app: &AppHandle,
    ctx: &HomeContext,
) -> Result<BootstrapData, String> {
    let store = storage::read_store(&ctx.database()).await?;
    let account_value = ctx
        .session
        .request(app, "account/read", json!({"refreshToken": false}))
        .await?;
    let pinned_threads: HashSet<&str> = store.pinned_threads.iter().map(String::as_str).collect();

    let mut all_threads = Vec::new();
    let mut search_rows = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let mut params = json!({
            "limit": THREAD_PAGE,
            "sortKey": "updated_at",
            "sortDirection": "desc",
            "archived": false
        });
        if let Some(cursor) = &cursor {
            params["cursor"] = json!(cursor);
        }
        let list_value = ctx.session.request(app, "thread/list", params).await?;
        let threads = arr_or_empty(&list_value, "data");
        search_rows.extend(
            threads
                .iter()
                .filter_map(|thread| thread_search_row(thread, false)),
        );
        all_threads.extend(
            threads
                .iter()
                .filter_map(|thread| thread_summary_from(thread, &pinned_threads)),
        );
        cursor = str_at(&list_value, "nextCursor").map(str::to_string);
        if cursor.is_none() || all_threads.len() >= MAX_THREADS {
            break;
        }
    }

    let stored_summaries: Vec<_> = all_threads.iter().map(StoredThreadSummary::from).collect();
    storage::replace_thread_summaries(&ctx.database(), &stored_summaries).await?;
    // Keep the local search index in step with the active thread listing.
    storage::upsert_thread_search(&ctx.database(), &search_rows).await?;

    let account = account_from(&account_value);
    let account_json = account
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| error.to_string())?;
    storage::write_account_cache(&ctx.database(), account_json.as_deref()).await?;
    let side_questions = storage::read_side_questions(&ctx.database()).await?;
    let extras = read_bootstrap_extras(ctx).await?;
    build_bootstrap(
        &ctx.runtime(),
        store,
        all_threads,
        account,
        side_questions,
        extras,
    )
}

/// Rebuild the payload from local caches only. Used after any mutation that
/// changed Pingex's own state rather than Codex's.
pub(crate) async fn bootstrap_cached(ctx: &HomeContext) -> Result<BootstrapData, String> {
    let store = storage::read_store(&ctx.database()).await?;
    let pinned_threads: HashSet<String> = store.pinned_threads.iter().cloned().collect();
    let all_threads = storage::read_thread_summaries(&ctx.database())
        .await?
        .into_iter()
        .map(|stored| {
            let mut summary = ThreadSummary::from(stored);
            summary.pinned = pinned_threads.contains(&summary.id);
            summary
        })
        .collect();
    let account = storage::read_account_cache(&ctx.database())
        .await?
        .map(|json| serde_json::from_str(&json))
        .transpose()
        .map_err(|error| format!("Could not parse cached account: {error}"))?;
    let side_questions = storage::read_side_questions(&ctx.database()).await?;
    let extras = read_bootstrap_extras(ctx).await?;
    build_bootstrap(
        &ctx.runtime(),
        store,
        all_threads,
        account,
        side_questions,
        extras,
    )
}

/// Load per-project instructions and attached sources, grouped by project path,
/// for the bootstrap payload.
async fn read_project_extras(
    ctx: &HomeContext,
) -> Result<
    (
        HashMap<String, String>,
        HashMap<String, Vec<StoredProjectSource>>,
    ),
    String,
> {
    let instructions: HashMap<String, String> =
        storage::read_all_project_instructions(&ctx.database())
            .await?
            .into_iter()
            .collect();
    let mut sources_by_project: HashMap<String, Vec<StoredProjectSource>> = HashMap::new();
    for source in storage::read_all_project_sources(&ctx.database()).await? {
        sources_by_project
            .entry(source.project_path.clone())
            .or_default()
            .push(source);
    }
    Ok((instructions, sources_by_project))
}

/// The repository each known temporary worktree belongs to.
///
/// Links are recorded when the worktree is created, but worktrees that predate
/// that (or were made outside the app) are backfilled here from git while the
/// directory is still on disk — after it is removed, only the stored link can
/// keep its threads attached to a repository.
async fn temp_worktree_parents(
    ctx: &HomeContext,
    runtime: &RuntimeConfig,
) -> Result<Vec<(String, String)>, String> {
    let mut links = storage::read_temp_worktrees(&ctx.database()).await?;
    let known: HashSet<String> = links.iter().map(|(path, _)| path.clone()).collect();
    for path in discover_worktrees(runtime) {
        if known.contains(&path) || !is_temp_worktree_path(runtime, &path) {
            continue;
        }
        let Some(parent) = worktree_parent_project(&path) else {
            continue;
        };
        storage::record_temp_worktree(&ctx.database(), &path, &parent).await?;
        links.push((path, parent));
    }
    Ok(links)
}

async fn read_bootstrap_extras(ctx: &HomeContext) -> Result<BootstrapExtras, String> {
    let (instructions, sources_by_project) = read_project_extras(ctx).await?;
    let temp_worktree_parents = temp_worktree_parents(ctx, &ctx.runtime()).await?;
    Ok(BootstrapExtras {
        temp_worktree_parents,
        instructions,
        sources_by_project,
        project_expansion: storage::read_project_expansion(&ctx.database()).await?,
        workspaces: storage::read_workspaces(&ctx.database()).await?,
        workspace_members: storage::read_all_workspace_members(&ctx.database()).await?,
        workspace_threads: storage::workspace_thread_map(&ctx.database()).await?,
        agent_children: storage::read_agent_run_children(&ctx.database()).await?,
    })
}

fn build_bootstrap(
    runtime: &RuntimeConfig,
    store: Store,
    all_threads: Vec<ThreadSummary>,
    account: Option<Account>,
    side_questions: Vec<SideQuestion>,
    extras: BootstrapExtras,
) -> Result<BootstrapData, String> {
    let BootstrapExtras {
        instructions,
        mut sources_by_project,
        project_expansion,
        workspaces,
        workspace_members,
        workspace_threads,
        agent_children,
        temp_worktree_parents,
    } = extras;
    // Threads that belong under something else rather than in a project:
    // side questions, and the threads app-owned subagents run in.
    let hidden_ids: HashSet<&str> = side_questions
        .iter()
        .map(|question| question.side_thread_id.as_str())
        .chain(agent_children.iter().map(|(child, _)| child.as_str()))
        .collect();
    // How many app-owned agents each thread spawned. Codex's own descendant
    // links cannot see these, so they are counted separately and added on.
    let mut agent_counts: HashMap<&str, usize> = HashMap::new();
    for (_, parent) in &agent_children {
        *agent_counts.entry(parent.as_str()).or_default() += 1;
    }
    let visible_threads: Vec<_> = all_threads
        .iter()
        .filter(|thread| !hidden_ids.contains(thread.id.as_str()))
        .cloned()
        .collect();
    let descendant_counts = descendant_counts(&visible_threads);
    let visible_threads: Vec<_> = visible_threads
        .into_iter()
        .map(|mut thread| {
            thread.subagent_count = descendant_counts.get(&thread.id).copied().unwrap_or(0)
                + agent_counts.get(thread.id.as_str()).copied().unwrap_or(0);
            thread
        })
        .collect();
    let subagents = visible_threads
        .iter()
        .filter(|thread| thread.parent_thread_id.is_some())
        .cloned()
        .collect();

    let mut members_by_workspace: HashMap<String, Vec<StoredWorkspaceMember>> = HashMap::new();
    for member in workspace_members {
        members_by_workspace
            .entry(member.workspace_id.clone())
            .or_default()
            .push(member);
    }
    // An isolated member's worktree belongs to its workspace, not to the
    // sidebar as a project of its own.
    let workspace_effective_paths: HashSet<String> = members_by_workspace
        .values()
        .flatten()
        .filter(|member| member.isolated)
        .map(|member| member.effective_path.clone())
        .collect();

    let mut entries = store.projects.clone();
    // A temporary worktree is never a project of its own — it is scaffolding
    // for one thread, and both it and its threads belong to the repository it
    // was cut from.
    entries.retain(|entry| !is_temp_worktree_path(runtime, &entry.path));
    let mut known: HashSet<String> = entries.iter().map(|entry| entry.path.clone()).collect();
    for path in discover_worktrees(runtime) {
        if let Some(parent) = worktree_parent_project(&path) {
            if known.insert(parent.clone()) {
                entries.push(StoredProject {
                    path: parent,
                    name: None,
                    pinned: false,
                    archived: false,
                });
            }
        }
        if is_temp_worktree_path(runtime, &path) {
            continue;
        }
        if known.insert(path.clone()) {
            entries.push(StoredProject {
                path,
                name: None,
                pinned: false,
                archived: false,
            });
        }
    }
    // Every repository a temporary worktree points at must be listed, even when
    // the worktree itself is gone, or its threads would have nowhere to live.
    for (_, parent) in &temp_worktree_parents {
        if Path::new(parent).is_dir() && known.insert(parent.clone()) {
            entries.push(StoredProject {
                path: parent.clone(),
                name: None,
                pinned: false,
                archived: false,
            });
        }
    }
    entries.retain(|entry| !workspace_effective_paths.contains(&entry.path));
    entries.sort_by_key(|entry| !entry.pinned);

    let mut projects = Vec::new();
    for entry in entries {
        let worktree = is_worktree_path(runtime, &entry.path);
        // A worktree that has been deleted on disk is simply gone; a plain
        // folder is kept so the user can still remove it from the sidebar.
        if worktree && !Path::new(&entry.path).is_dir() {
            continue;
        }
        let name = entry
            .name
            .clone()
            .unwrap_or_else(|| default_project_name(&entry.path, worktree));
        let mut threads: Vec<_> = visible_threads
            .iter()
            .filter(|thread| thread.parent_thread_id.is_none())
            .filter(|thread| !workspace_threads.contains_key(&thread.id))
            .filter(|thread| {
                Path::new(home_path(&thread.cwd, &temp_worktree_parents))
                    .starts_with(Path::new(&entry.path))
            })
            .cloned()
            .collect();
        threads.sort_by_key(|thread| !thread.pinned);
        projects.push(Project {
            kind: if worktree { "worktree" } else { "folder" }.into(),
            name,
            workspace_id: None,
            pinned: entry.pinned,
            archived: entry.archived,
            expanded: project_expansion.get(&entry.path).copied().unwrap_or(true),
            threads,
            instructions: instructions.get(&entry.path).cloned().unwrap_or_default(),
            sources: sources_by_project.remove(&entry.path).unwrap_or_default(),
            members: Vec::new(),
            path: entry.path,
        });
    }

    for workspace in workspaces
        .into_iter()
        .filter(|workspace| !workspace.archived)
    {
        let members = members_by_workspace
            .remove(&workspace.id)
            .unwrap_or_default();
        let mut threads: Vec<_> = visible_threads
            .iter()
            .filter(|thread| thread.parent_thread_id.is_none())
            .filter(|thread| workspace_threads.get(&thread.id) == Some(&workspace.id))
            .cloned()
            .collect();
        threads.sort_by_key(|thread| !thread.pinned);
        // A workspace has its own hub-level instructions. Member instructions
        // are supplied at turn time by the workspace runtime context.
        projects.push(Project {
            name: workspace.name,
            kind: "multiProject".into(),
            workspace_id: Some(workspace.id),
            pinned: false,
            archived: false,
            expanded: project_expansion
                .get(&workspace.hub_path)
                .copied()
                .unwrap_or(true),
            threads,
            instructions: instructions
                .get(&workspace.hub_path)
                .cloned()
                .unwrap_or_default(),
            sources: sources_by_project
                .remove(&workspace.hub_path)
                .unwrap_or_default(),
            members: members
                .into_iter()
                .map(|member| WorkspaceMember {
                    source_path: member.source_path,
                    effective_path: member.effective_path.clone(),
                    alias: member.alias,
                    isolated: member.isolated,
                    branch: member.branch,
                    available: Path::new(&member.effective_path).is_dir(),
                })
                .collect(),
            path: workspace.hub_path,
        });
    }

    Ok(BootstrapData {
        codex_home: runtime.codex_home.display().to_string(),
        codex_binary: runtime.codex_binary.display().to_string(),
        projects,
        account,
        side_questions,
        subagents,
    })
}

/// Which project a thread is listed under: its own working directory, unless
/// that directory is a temporary worktree — those are discarded, so the thread
/// is listed under the repository the worktree came from and survives it.
fn home_path<'a>(cwd: &'a str, temp_worktree_parents: &'a [(String, String)]) -> &'a str {
    temp_worktree_parents
        .iter()
        .find(|(worktree, _)| Path::new(cwd).starts_with(Path::new(worktree)))
        .map_or(cwd, |(_, parent)| parent.as_str())
}

/// The folder name, suffixed so a worktree is distinguishable from the
/// repository it was cut from.
fn default_project_name(path: &str, worktree: bool) -> String {
    let base = Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(path)
        .to_string();
    if worktree {
        format!("{base}-permanent-worktree")
    } else {
        base
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::StoredWorkspace;
    use std::path::PathBuf;

    fn thread(id: &str, cwd: &str, updated_at: i64) -> ThreadSummary {
        ThreadSummary {
            id: id.into(),
            cwd: cwd.into(),
            title: id.into(),
            updated_at,
            status: "idle".into(),
            pinned: false,
            parent_thread_id: None,
            agent_nickname: None,
            agent_role: None,
            subagent_count: 0,
        }
    }

    #[test]
    fn reads_the_account_label_and_falls_back_by_kind() {
        let named = account_from(
            &json!({"account": {"type": "chatgpt", "email": "me@example.com", "planType": "pro"}}),
        )
        .unwrap();
        assert_eq!(named.label, "me@example.com");
        assert_eq!(named.plan.as_deref(), Some("pro"));

        let api_key = account_from(&json!({"account": {"type": "apiKey"}})).unwrap();
        assert_eq!(api_key.label, "API key account");
        assert_eq!(api_key.plan, None);

        assert!(account_from(&json!({})).is_none());
    }

    #[test]
    fn names_worktrees_by_their_kind() {
        assert_eq!(default_project_name("/repo/api", false), "api");
        assert_eq!(
            default_project_name("/wt/api", true),
            "api-permanent-worktree"
        );
    }

    #[test]
    fn bootstrap_keeps_workspace_threads_and_isolated_members_out_of_normal_projects() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("api");
        let isolated = directory.path().join("workspace-api");
        let hub = directory.path().join("hub");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::create_dir_all(&isolated).unwrap();
        std::fs::create_dir_all(&hub).unwrap();
        let source_path = source.display().to_string();
        let isolated_path = isolated.display().to_string();
        let hub_path = hub.display().to_string();
        let workspace = StoredWorkspace {
            id: "workspace-1".into(),
            name: "API + Web".into(),
            hub_path: hub_path.clone(),
            archived: false,
        };
        let data = build_bootstrap(
            &RuntimeConfig {
                codex_home: directory.path().join("codex-home"),
                codex_binary: PathBuf::from("codex"),
            },
            Store {
                projects: vec![
                    StoredProject {
                        path: source_path.clone(),
                        name: Some("API".into()),
                        pinned: false,
                        archived: false,
                    },
                    StoredProject {
                        path: isolated_path.clone(),
                        name: Some("Managed worktree".into()),
                        pinned: false,
                        archived: false,
                    },
                ],
                pinned_threads: Vec::new(),
            },
            vec![
                thread("ordinary-thread", &source_path, 1),
                thread("workspace-thread", &source_path, 2),
            ],
            None,
            Vec::new(),
            BootstrapExtras {
                instructions: HashMap::new(),
                sources_by_project: HashMap::new(),
                project_expansion: HashMap::from([
                    (source_path.clone(), false),
                    (hub_path.clone(), false),
                ]),
                workspaces: vec![workspace],
                workspace_members: vec![StoredWorkspaceMember {
                    workspace_id: "workspace-1".into(),
                    source_path,
                    effective_path: isolated_path,
                    alias: "api".into(),
                    isolated: true,
                    branch: Some("codex/workspace-1/api".into()),
                    ordinal: 0,
                }],
                workspace_threads: HashMap::from([(
                    "workspace-thread".into(),
                    "workspace-1".into(),
                )]),
                agent_children: Vec::new(),
                temp_worktree_parents: Vec::new(),
            },
        )
        .unwrap();

        assert_eq!(data.projects.len(), 2);
        let api = data
            .projects
            .iter()
            .find(|project| project.name == "API")
            .unwrap();
        assert_eq!(api.threads.len(), 1);
        assert_eq!(api.threads[0].id, "ordinary-thread");
        assert!(!api.expanded);
        assert!(data
            .projects
            .iter()
            .all(|project| project.name != "Managed worktree"));
        let workspace = data
            .projects
            .iter()
            .find(|project| project.workspace_id.as_deref() == Some("workspace-1"))
            .unwrap();
        assert_eq!(workspace.kind, "multiProject");
        assert!(!workspace.expanded);
        assert_eq!(workspace.threads[0].id, "workspace-thread");
        assert_eq!(workspace.members[0].alias, "api");
        assert!(workspace.members[0].available);
    }

    #[test]
    fn side_question_threads_are_hidden_from_project_listings() {
        let directory = tempfile::tempdir().unwrap();
        let project = directory.path().join("proj");
        std::fs::create_dir_all(&project).unwrap();
        let project_path = project.display().to_string();

        let data = build_bootstrap(
            &RuntimeConfig {
                codex_home: directory.path().join("codex-home"),
                codex_binary: PathBuf::from("codex"),
            },
            Store {
                projects: vec![StoredProject {
                    path: project_path.clone(),
                    name: Some("Proj".into()),
                    pinned: false,
                    archived: false,
                }],
                pinned_threads: Vec::new(),
            },
            vec![
                thread("main-thread", &project_path, 2),
                thread("side-thread", &project_path, 1),
            ],
            None,
            vec![SideQuestion {
                side_thread_id: "side-thread".into(),
                parent_thread_id: "main-thread".into(),
                title: "What about tests?".into(),
                created_at: 1,
            }],
            BootstrapExtras {
                instructions: HashMap::new(),
                sources_by_project: HashMap::new(),
                project_expansion: HashMap::new(),
                workspaces: Vec::new(),
                workspace_members: Vec::new(),
                workspace_threads: HashMap::new(),
                agent_children: Vec::new(),
                temp_worktree_parents: Vec::new(),
            },
        )
        .unwrap();

        let threads = &data.projects[0].threads;
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].id, "main-thread");
    }

    #[test]
    fn temporary_worktree_threads_are_listed_under_the_repository_and_outlive_it() {
        let directory = tempfile::tempdir().unwrap();
        let home = directory.path().join("codex-home");
        let project = directory.path().join("proj");
        std::fs::create_dir_all(&project).unwrap();
        let project_path = project.display().to_string();
        // One temporary worktree still on disk, one already discarded.
        let live = home.join("worktrees-tmp/proj/live");
        std::fs::create_dir_all(&live).unwrap();
        let live_path = live.display().to_string();
        let gone_path = home.join("worktrees-tmp/proj/gone").display().to_string();

        let data = build_bootstrap(
            &RuntimeConfig {
                codex_home: home,
                codex_binary: PathBuf::from("codex"),
            },
            Store {
                projects: vec![StoredProject {
                    path: project_path.clone(),
                    name: Some("Proj".into()),
                    pinned: false,
                    archived: false,
                }],
                pinned_threads: Vec::new(),
            },
            vec![
                thread("own-thread", &project_path, 3),
                thread("live-worktree-thread", &live_path, 2),
                thread("gone-worktree-thread", &gone_path, 1),
            ],
            None,
            Vec::new(),
            BootstrapExtras {
                instructions: HashMap::new(),
                sources_by_project: HashMap::new(),
                project_expansion: HashMap::new(),
                workspaces: Vec::new(),
                workspace_members: Vec::new(),
                workspace_threads: HashMap::new(),
                agent_children: Vec::new(),
                temp_worktree_parents: vec![
                    (live_path, project_path.clone()),
                    (gone_path, project_path),
                ],
            },
        )
        .unwrap();

        // A temporary worktree is never a project of its own, and its threads
        // stay with the repository whether or not it still exists.
        assert_eq!(data.projects.len(), 1);
        let ids: Vec<_> = data.projects[0]
            .threads
            .iter()
            .map(|thread| thread.id.as_str())
            .collect();
        assert_eq!(
            ids,
            ["own-thread", "live-worktree-thread", "gone-worktree-thread"]
        );
    }

    #[test]
    fn app_owned_agent_threads_are_hidden_and_counted_against_their_parent() {
        let directory = tempfile::tempdir().unwrap();
        let project = directory.path().join("proj");
        std::fs::create_dir_all(&project).unwrap();
        let project_path = project.display().to_string();

        let data = build_bootstrap(
            &RuntimeConfig {
                codex_home: directory.path().join("codex-home"),
                codex_binary: PathBuf::from("codex"),
            },
            Store {
                projects: vec![StoredProject {
                    path: project_path.clone(),
                    name: Some("Proj".into()),
                    pinned: false,
                    archived: false,
                }],
                pinned_threads: Vec::new(),
            },
            vec![
                thread("main-thread", &project_path, 3),
                thread("agent-thread-1", &project_path, 2),
                thread("agent-thread-2", &project_path, 1),
            ],
            None,
            Vec::new(),
            BootstrapExtras {
                instructions: HashMap::new(),
                sources_by_project: HashMap::new(),
                project_expansion: HashMap::new(),
                workspaces: Vec::new(),
                workspace_members: Vec::new(),
                workspace_threads: HashMap::new(),
                agent_children: vec![
                    ("agent-thread-1".into(), "main-thread".into()),
                    ("agent-thread-2".into(), "main-thread".into()),
                ],
                temp_worktree_parents: Vec::new(),
            },
        )
        .unwrap();

        // The agents' own threads are real threads in the same cwd, so without
        // hiding they would sit alongside the thread that spawned them.
        let threads = &data.projects[0].threads;
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].id, "main-thread");
        assert_eq!(threads[0].subagent_count, 2);
    }
}
