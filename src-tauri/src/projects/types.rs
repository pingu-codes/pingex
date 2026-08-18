//! The shape of the payload the app opens with.
//!
//! `BootstrapData` is everything the sidebar needs in one round trip: the
//! project tree, each project's threads, the signed-in account, and the local
//! extras (instructions, sources, workspace membership) that Codex knows nothing
//! about.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::storage::{SideQuestion, StoredProjectSource, StoredWorkspace, StoredWorkspaceMember};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ThreadSummary {
    pub(crate) id: String,
    pub(crate) cwd: String,
    pub(crate) title: String,
    pub(crate) updated_at: i64,
    pub(crate) status: String,
    pub(crate) pinned: bool,
    pub(crate) parent_thread_id: Option<String>,
    pub(crate) agent_nickname: Option<String>,
    pub(crate) agent_role: Option<String>,
    pub(crate) subagent_count: usize,
}

/// One row in the sidebar: a real folder, a Codex-managed worktree, or a
/// workspace hub standing in for several repositories.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Project {
    pub(crate) path: String,
    pub(crate) name: String,
    /// "folder" | "worktree" | "multiProject".
    pub(crate) kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) workspace_id: Option<String>,
    pub(crate) pinned: bool,
    pub(crate) archived: bool,
    /// Whether the sidebar shows this project's threads. Missing preferences
    /// default to true so new and pre-existing projects start expanded.
    pub(crate) expanded: bool,
    pub(crate) threads: Vec<ThreadSummary>,
    /// Free-form project instructions; empty string when none are stored.
    pub(crate) instructions: String,
    /// Attached, indexable folder/file sources for this project.
    pub(crate) sources: Vec<StoredProjectSource>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) members: Vec<WorkspaceMember>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorkspaceMember {
    pub(crate) source_path: String,
    pub(crate) effective_path: String,
    pub(crate) alias: String,
    pub(crate) isolated: bool,
    pub(crate) branch: Option<String>,
    pub(crate) available: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct Account {
    pub(crate) label: String,
    pub(crate) plan: Option<String>,
    pub(crate) kind: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BootstrapData {
    pub(crate) codex_home: String,
    pub(crate) codex_binary: String,
    pub(crate) projects: Vec<Project>,
    pub(crate) account: Option<Account>,
    pub(crate) side_questions: Vec<SideQuestion>,
    pub(crate) subagents: Vec<ThreadSummary>,
}

/// State stored by Pingex rather than supplied by the app-server. Keeping this
/// together prevents the bootstrap builder from becoming a positional-argument
/// dump as the local project model grows.
pub(crate) struct BootstrapExtras {
    pub(crate) instructions: HashMap<String, String>,
    pub(crate) sources_by_project: HashMap<String, Vec<StoredProjectSource>>,
    pub(crate) project_expansion: HashMap<String, bool>,
    pub(crate) workspaces: Vec<StoredWorkspace>,
    pub(crate) workspace_members: Vec<StoredWorkspaceMember>,
    pub(crate) workspace_threads: HashMap<String, String>,
    /// `(agent thread, the thread that spawned it)` for every app-owned
    /// subagent. Codex does not set `parentThreadId` on them — they run in
    /// separate processes — so without this they would show up as ordinary
    /// top-level threads and count against nobody.
    pub(crate) agent_children: Vec<(String, String)>,
    /// `(temporary worktree, the repository it was cut from)`. Temporary
    /// worktrees are throwaway, so their threads are listed under that
    /// repository instead of under a project of their own.
    pub(crate) temp_worktree_parents: Vec<(String, String)>,
}
