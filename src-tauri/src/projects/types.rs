//! The shape of the payload the app opens with.
//!
//! `BootstrapData` is everything the sidebar needs in one round trip: the
//! project tree, each project's threads, the signed-in account, and the local
//! extras (instructions, sources, workspace membership) that Codex knows nothing
//! about.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::storage::{
    SideQuestion, SidebarLayout, StoredProjectSource, StoredThreadSection, StoredWorkspace,
    StoredWorkspaceMember,
};

#[derive(Clone, Serialize, specta::Type)]
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
    /// The app-server project Codex has the thread filed under, when it has
    /// one (Codex ≥0.149 and the thread was assigned). See `projects::server`.
    pub(crate) project_id: Option<String>,
    /// The thread section it sits in (Codex ≥0.149). See `threads::sections`.
    pub(crate) section_id: Option<String>,
    pub(crate) subagent_count: usize,
    /// Which harness runs the thread; `None` means Codex.
    #[specta(optional)]
    pub(crate) harness: Option<String>,
}

/// One row in the sidebar: a real folder, a Codex-managed worktree, or a
/// workspace hub standing in for several repositories.
#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Project {
    pub(crate) path: String,
    pub(crate) name: String,
    /// "folder" | "worktree" | "multiProject".
    pub(crate) kind: String,
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
    #[serde(default)]
    pub(crate) members: Vec<WorkspaceMember>,
    /// When the project's newest thread was active, in Unix seconds, as the
    /// app-server reports it (`Project.recencyAt`, unreleased Codex). `None`
    /// on released Codex or a project with no threads; the sidebar then keeps
    /// the stored order.
    #[serde(default)]
    pub(crate) recency_at: Option<i64>,
}

#[derive(Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorkspaceMember {
    pub(crate) source_path: String,
    pub(crate) effective_path: String,
    pub(crate) alias: String,
    pub(crate) isolated: bool,
    pub(crate) branch: Option<String>,
    pub(crate) available: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
pub(crate) struct Account {
    pub(crate) label: String,
    pub(crate) plan: Option<String>,
    pub(crate) kind: String,
}

#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BootstrapData {
    pub(crate) codex_home: String,
    pub(crate) codex_binary: String,
    pub(crate) projects: Vec<Project>,
    pub(crate) account: Option<Account>,
    pub(crate) side_questions: Vec<SideQuestion>,
    pub(crate) subagents: Vec<ThreadSummary>,
    /// The app-server's thread sections, in its order; threads reference them
    /// by `section_id`. Empty — and `sections_supported` false — on a Codex
    /// without `threadSection/*`, in which case the sidebar offers none.
    pub(crate) sections: Vec<StoredThreadSection>,
    pub(crate) sections_supported: bool,
    /// User-made sidebar folders and explicit orderings; the sidebar builds
    /// its tree from these plus the flat project/thread lists.
    pub(crate) sidebar_layout: SidebarLayout,
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
    /// `server project id → local key` for every sidebar entry mirrored to
    /// the app-server. Empty on a Codex without `project/*`.
    pub(crate) server_projects: HashMap<String, String>,
    /// `local key → recencyAt` (Unix seconds of the newest thread) as the
    /// app-server reports it. Empty on released Codex (through 0.151), which
    /// has no `Project.recencyAt`.
    pub(crate) project_recency: HashMap<String, i64>,
    pub(crate) sections: Vec<StoredThreadSection>,
    pub(crate) sections_supported: bool,
    pub(crate) sidebar_layout: SidebarLayout,
}
