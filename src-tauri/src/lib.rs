//! Pingex — an independent desktop frontend for the Codex CLI's app-server.
//!
//! Module map (each directory is one domain; `commands.rs` inside a domain
//! holds its `#[tauri::command]` entry points):
//!
//! - `codex`       — the app-server client: CLI discovery, the child process, pairing
//! - `agents`      — app-owned subagents: the `pingex_*` tools and the processes behind them
//! - `storage`     — the frontend SQLite database, one file per Codex home
//! - `projects`    — the project/thread tree and the bootstrap payload the app opens with
//! - `threads`     — reading a thread, running turns, and thread lifecycle
//! - `workspaces`  — virtual projects stitched from several repositories
//! - `git`         — repository status, worktrees, recent commits
//! - `review`      — pull requests and review submission via the `gh` CLI
//! - `sources`     — attached project sources, content indexing, workspace search
//! - `integrations`— MCP servers and skills declared in `config.toml`
//! - `connections` — paired remote-control devices
//! - `handoff`     — `codex://` deep links and handing a thread to a terminal
//! - `composer`    — attachments, drafts, and the quick-chat window
//! - `settings`    — the active runtime, Pingex prefs, and Codex `config.toml`
//! - `files`       — project file listing and `@`-mention search
//! - `os`          — handing a path or URL to the desktop environment
//! - `util`        — cross-cutting helpers owned by no single domain

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use tauri::Manager;

mod agents;
mod claude;
mod codex;
mod composer;
mod connections;
mod files;
mod git;
mod handoff;
mod harness;
mod home_window;
mod integrations;
mod os;
mod profiles;
mod projects;
mod review;
mod settings;
mod sources;
mod storage;
mod threads;
mod util;
mod workspaces;

use codex::CodexSession;
pub(crate) use home_window::HomeWindow;
use util::host::Host;

/// The pieces the live end-to-end suite (`tests/live_codex.rs`) replays against
/// a real `codex` binary: the exact request payloads the app sends and the
/// parsers it reads responses with. Not an API for anything else.
pub mod e2e {
    pub use crate::agents::supervisor::{collect_model_ids, sandbox_tag, AGENT_PREAMBLE};
    pub use crate::agents::tools::{specs as agent_tool_specs, DELEGATION_POLICY};
    pub use crate::claude::child::BASE_ARGS as CLAUDE_BASE_ARGS;
    pub use crate::claude::driver::turn_args as claude_turn_args;
    pub use crate::claude::permissions::permission_result as claude_permission_result;
    pub use crate::codex::binary::{missing_message, resolve as resolve_codex_binary};
    pub use crate::codex::child::kill_orphaned_app_servers;
    pub use crate::codex::child::APP_SERVER_ARGS as CODEX_APP_SERVER_ARGS;
    pub use crate::codex::compat::{method_unsupported, speed_tier_unsupported, Feature};
    pub use crate::codex::requests;
    pub use crate::integrations::app_server::parse_skills;
    pub use crate::integrations::SkillSummary;
    pub use crate::projects::worktrees::{
        is_temp_worktree_path_under, temp_worktrees_root, worktree_parent_project,
    };
    pub use crate::storage::{
        add_side_question, delete_side_question, open as open_database, read_side_questions,
        read_temp_worktrees, record_temp_worktree, SideQuestion,
    };
    pub use crate::threads::autoname::NAMER_INSTRUCTIONS;
    pub use crate::threads::side_questions::MAX_TITLE_CHARS;
    pub use crate::util::host::{orphan_pids, Host};
    pub use crate::util::process::{run as run_process, CommandOutput, Run};
}
use util::time::unix_secs;

#[derive(Clone)]
pub(crate) struct RuntimeConfig {
    /// A host path: what `host` sees. Read it through `host.to_local` before
    /// opening anything in it from this process.
    pub(crate) codex_home: PathBuf,
    pub(crate) codex_binary: PathBuf,
    /// Where the home, the binary and every process of this home live.
    pub(crate) host: Host,
}

impl RuntimeConfig {
    pub(crate) fn codex_home_str(&self) -> String {
        self.codex_home.to_string_lossy().into_owned()
    }

    pub(crate) fn codex_binary_str(&self) -> String {
        self.codex_binary.to_string_lossy().into_owned()
    }

    /// The home as a local path this process can open: identity on a native
    /// host, the `\\wsl.localhost` share on WSL.
    pub(crate) fn local_home(&self) -> PathBuf {
        self.host.to_local(&self.codex_home_str())
    }
}

/// The active Codex home, shared with `CodexSession` so a home switch is
/// visible the next time the app-server child is (re)spawned.
pub(crate) type SharedRuntime = Arc<RwLock<RuntimeConfig>>;

/// A home's canonical identity: the registry key and the `codexHome` tag on
/// every event payload. Falls back to the lexical path when the folder cannot
/// be canonicalized (e.g. it does not exist yet). A WSL home's key carries
/// its distribution, so the same path on two hosts is two keys.
pub(crate) fn home_key_for(host: &Host, path: &str) -> String {
    host.home_key(&host.canonical(path))
}

/// Everything the app runs against one Codex home: the runtime identity, the
/// frontend database, the long-lived app-server child, and the subagents it
/// spawned. Windows bind to one of these; two windows on the same home share
/// one context.
pub(crate) struct HomeContext {
    pub(crate) harness_kind: Option<harness::HarnessKind>,
    /// The Profile this runtime belongs to. Legacy contexts start a Profile
    /// of their own; additional Homes retain the Profile's original key.
    pub(crate) profile_key: String,
    /// Canonical home path — the registry key, and the `codexHome` field the
    /// frontend filters events on.
    pub(crate) home_key: String,
    runtime: SharedRuntime,
    database: RwLock<turso::Database>,
    pub(crate) session: CodexSession,
    /// The Claude Code driver for this home: one `claude` process per active
    /// Claude thread. Spawned lazily, like the app-server child.
    pub(crate) claude: claude::ClaudeDriver,
    /// Subagent processes this home owns. Same lifetime as `session`: reached
    /// from Tauri commands and from the session's reader thread.
    pub(crate) agents: agents::supervisor::AgentSupervisor,
}

impl HomeContext {
    fn new(runtime: RuntimeConfig, database: turso::Database) -> Arc<Self> {
        let home_key = home_key_for(&runtime.host, &runtime.codex_home_str());
        let claude = claude::driver::ClaudeRuntime::resolve(&settings::prefs::read_overrides(
            &settings::prefs::settings_path(),
        ));
        Self::in_profile(runtime, database, home_key.clone(), home_key, claude, None)
    }

    pub(crate) fn in_profile(
        runtime: RuntimeConfig,
        database: turso::Database,
        home_key: String,
        profile_key: String,
        claude_runtime: claude::driver::ClaudeRuntime,
        harness_kind: Option<harness::HarnessKind>,
    ) -> Arc<Self> {
        let runtime: SharedRuntime = Arc::new(RwLock::new(runtime));
        let session = CodexSession::new(runtime.clone(), home_key.clone());
        let claude =
            claude::ClaudeDriver::new(home_key.clone(), claude_runtime, session.wire().clone());
        Arc::new(Self {
            harness_kind,
            profile_key,
            session,
            claude,
            agents: agents::supervisor::AgentSupervisor::default(),
            runtime,
            database: RwLock::new(database),
            home_key,
        })
    }

    /// A snapshot of this home's runtime config. Cheap to clone and never held
    /// across await points, so a concurrent binary switch is observed fresh.
    pub(crate) fn runtime(&self) -> RuntimeConfig {
        self.runtime.read().expect("runtime lock poisoned").clone()
    }

    /// Where this home lives. Fixed for the life of the context.
    pub(crate) fn host(&self) -> Host {
        self.runtime
            .read()
            .expect("runtime lock poisoned")
            .host
            .clone()
    }

    /// A handle to this home's frontend database (an `Arc` clone internally).
    pub(crate) fn database(&self) -> turso::Database {
        self.database
            .read()
            .expect("database lock poisoned")
            .clone()
    }

    /// Point this home at a different Codex CLI. The caller resets the session
    /// so the next request respawns with the new binary.
    pub(crate) fn set_binary(&self, binary: PathBuf) {
        self.runtime
            .write()
            .expect("runtime lock poisoned")
            .codex_binary = binary;
    }

    /// Kill everything this home is running. Agents first: they are children
    /// of this process too, and nothing else will reap them once dropped.
    pub(crate) fn shutdown(&self) {
        self.agents.kill_all();
        self.claude.kill_all();
        self.session.kill_child();
    }
}

/// The frontend database for one home, reached from background tasks (journal
/// writes, agent persistence) that only carry the home's key.
pub(crate) fn database_for(app: &tauri::AppHandle, home_key: &str) -> Option<turso::Database> {
    app.try_state::<AppState>()
        .and_then(|state| state.context_for_home(home_key))
        .map(|context| context.database())
}

pub(crate) struct AppState {
    profile_cache_root: PathBuf,
    /// One context per open Codex home, keyed by canonical path.
    contexts: RwLock<HashMap<String, Arc<HomeContext>>>,
    /// Which home each window is bound to (window label → home key). Windows
    /// missing here (the pre-pick `main`, `quick`) use the default context.
    bindings: RwLock<HashMap<String, String>>,
    /// The home unbound windows and the quick window fall back to — the one
    /// the app launched with, following the `main` window's switches.
    default_home: RwLock<String>,
    /// Monotonic label counter for extra windows (`main-2`, `main-3`, …).
    window_counter: AtomicU64,
}

impl AppState {
    fn new(context: Arc<HomeContext>, launch_explicit: bool) -> Self {
        let default_home = context.home_key.clone();
        let mut contexts = HashMap::new();
        contexts.insert(context.home_key.clone(), context);
        let mut bindings = HashMap::new();
        // The main window opens on the launch home; a non-explicit launch
        // shows the picker first (an unbound window), and the binding is
        // created when the user picks.
        if launch_explicit {
            bindings.insert("main".to_string(), default_home.clone());
        }
        Self {
            profile_cache_root: dirs::data_dir()
                .unwrap_or_else(std::env::temp_dir)
                .join("pingex")
                .join("profiles"),
            contexts: RwLock::new(contexts),
            bindings: RwLock::new(bindings),
            default_home: RwLock::new(default_home),
            window_counter: AtomicU64::new(1),
        }
    }

    pub(crate) fn default_home(&self) -> String {
        self.default_home
            .read()
            .expect("default home lock poisoned")
            .clone()
    }

    pub(crate) fn set_default_home(&self, home_key: &str) {
        *self
            .default_home
            .write()
            .expect("default home lock poisoned") = home_key.to_string();
    }

    /// The context unbound windows fall back to. Always present: it is only
    /// ever repointed at another live context, never removed.
    pub(crate) fn default_context(&self) -> Arc<HomeContext> {
        let key = self.default_home();
        self.context_for_home(&key)
            .expect("default context missing")
    }

    pub(crate) fn context_for_home(&self, home_key: &str) -> Option<Arc<HomeContext>> {
        self.contexts
            .read()
            .expect("contexts lock poisoned")
            .get(home_key)
            .cloned()
    }

    /// The context a window works against: its binding, else the default.
    pub(crate) fn ctx_for_label(&self, label: &str) -> Arc<HomeContext> {
        let bound = self
            .bindings
            .read()
            .expect("bindings lock poisoned")
            .get(label)
            .cloned();
        bound
            .and_then(|key| self.context_for_home(&key))
            .unwrap_or_else(|| self.default_context())
    }

    pub(crate) fn ctx(&self, window: &impl home_window::ContextWindow) -> Arc<HomeContext> {
        window.context(self)
    }

    pub(crate) fn context_for_route(
        &self,
        label: &str,
        home_key: Option<&str>,
    ) -> Result<Arc<HomeContext>, String> {
        let profile = self.ctx_for_label(label);
        let Some(home_key) = home_key else {
            return Ok(profile);
        };
        let context = self
            .context_for_home(home_key)
            .ok_or("Home is not open in this Profile")?;
        if context.profile_key != profile.profile_key {
            return Err("Home does not belong to this window's Profile".into());
        }
        Ok(context)
    }

    pub(crate) fn window_bound(&self, label: &str) -> bool {
        self.bindings
            .read()
            .expect("bindings lock poisoned")
            .contains_key(label)
    }

    /// Every (window label, home key) binding, for deep-link routing.
    pub(crate) fn window_bindings(&self) -> Vec<(String, String)> {
        self.bindings
            .read()
            .expect("bindings lock poisoned")
            .iter()
            .map(|(label, key)| (label.clone(), key.clone()))
            .collect()
    }

    pub(crate) fn all_contexts(&self) -> Vec<Arc<HomeContext>> {
        self.contexts
            .read()
            .expect("contexts lock poisoned")
            .values()
            .cloned()
            .collect()
    }

    pub(crate) fn insert_context(&self, context: Arc<HomeContext>) -> Arc<HomeContext> {
        self.contexts
            .write()
            .expect("contexts lock poisoned")
            .entry(context.home_key.clone())
            .or_insert(context)
            .clone()
    }

    pub(crate) fn profile_root_path(&self, host: &Host, home: &str) -> PathBuf {
        storage::profile_root_path_in(&self.profile_cache_root, host, home)
    }

    /// Reuse the context for `home` on `host` or open its local Profile cache.
    /// `home` is a host path.
    pub(crate) async fn ensure_context(
        &self,
        host: Host,
        home: String,
    ) -> Result<Arc<HomeContext>, String> {
        let key = home_key_for(&host, &home);
        if let Some(existing) = self.context_for_home(&key) {
            return Ok(existing);
        }
        let database =
            storage::open_profile_root_at(&self.profile_root_path(&host, &home), &host, &home)
                .await?;
        // Resolve again in case the Host became available during the import.
        let key = home_key_for(&host, &home);
        if let Some(existing) = self.context_for_home(&key) {
            return Ok(existing);
        }
        composer::attachments::cleanup_on_startup(&host.to_local(&home));
        let _ = storage::orphan_running_agent_runs(&database).await;
        let default = self.default_context().runtime();
        let codex_binary = if default.host == host {
            default.codex_binary
        } else {
            PathBuf::from("codex")
        };
        let context = HomeContext::new(
            RuntimeConfig {
                codex_home: PathBuf::from(home),
                codex_binary,
                host,
            },
            database,
        );
        let mut contexts = self.contexts.write().expect("contexts lock poisoned");
        // A concurrent open of the same home wins by arriving first.
        Ok(contexts
            .entry(key)
            .or_insert_with(|| context.clone())
            .clone())
    }

    /// Bind a window to a home, returning the previously bound context when
    /// this orphaned it (no other window bound, not the default) — the caller
    /// shuts it down.
    pub(crate) fn bind_window(&self, label: &str, home_key: &str) -> Option<Arc<HomeContext>> {
        let previous = {
            let mut bindings = self.bindings.write().expect("bindings lock poisoned");
            bindings.insert(label.to_string(), home_key.to_string())
        };
        previous.and_then(|old| self.release_if_orphaned(&old))
    }

    /// Drop a closed window's binding, returning its context when nothing else
    /// uses it any more — the caller shuts it down.
    pub(crate) fn unbind_window(&self, label: &str) -> Option<Arc<HomeContext>> {
        let removed = self
            .bindings
            .write()
            .expect("bindings lock poisoned")
            .remove(label);
        removed.and_then(|key| self.release_if_orphaned(&key))
    }

    /// Remove `home_key`'s context from the registry when no window is bound
    /// to it and it is not the default (which quick chat and unbound windows
    /// rely on). Returns the removed context for the caller to shut down.
    fn release_if_orphaned(&self, home_key: &str) -> Option<Arc<HomeContext>> {
        if home_key == self.default_home() {
            return None;
        }
        let bindings = self.bindings.read().expect("bindings lock poisoned");
        if bindings.values().any(|key| key == home_key) {
            return None;
        }
        drop(bindings);
        let mut contexts = self.contexts.write().expect("contexts lock poisoned");
        let member_keys: Vec<_> = contexts
            .values()
            .filter(|context| context.profile_key == home_key && context.home_key != home_key)
            .map(|context| context.home_key.clone())
            .collect();
        let members: Vec<_> = member_keys
            .into_iter()
            .filter_map(|key| contexts.remove(&key))
            .collect();
        let removed = contexts.remove(home_key);
        drop(contexts);
        for member in members {
            member.shutdown();
        }
        removed
    }

    pub(crate) fn next_window_label(&self) -> String {
        format!(
            "main-{}",
            self.window_counter.fetch_add(1, Ordering::SeqCst) + 1
        )
    }
}

/// Every IPC command, registered once for both the Tauri invoke handler and
/// the generated TypeScript bindings (`src/lib/bindings.ts`).
fn specta_builder() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::<tauri::Wry>::new()
        // Generated commands reject on `Err`, matching raw `invoke` semantics.
        .error_handling(tauri_specta::ErrorHandlingMode::Throw)
        // Ids and unix timestamps are `i64`; the frontend treats them as plain numbers.
        .dangerously_cast_bigints_to_number()
        // One TS type per struct; phased mode overflows the stack in rc.25 and
        // unified mode cannot express `skip_serializing_if`, so we do not use it.
        .disable_serde_phases()
        .commands(tauri_specta::collect_commands![
            // Projects and the bootstrap payload
            profiles::bootstrap_profile,
            profiles::add_profile_project,
            profiles::register_profile_home,
            profiles::set_profile_default_home,
            profiles::prepare_profile_workspace,
            projects::commands::bootstrap,
            projects::commands::add_project,
            projects::commands::add_worktree_project,
            projects::commands::rename_project,
            projects::commands::remove_project,
            projects::commands::set_project_pinned,
            projects::commands::set_project_archived,
            projects::commands::set_project_expanded,
            projects::commands::create_sidebar_folder,
            projects::commands::rename_sidebar_folder,
            projects::commands::delete_sidebar_folder,
            projects::commands::set_sidebar_folder_expanded,
            projects::commands::place_sidebar_item,
            projects::commands::set_thread_pinned,
            projects::commands::set_threads_hidden,
            projects::commands::reset_sidebar_order,
            projects::commands::apply_session_focus,
            projects::commands::read_account_rate_limits,
            projects::commands::read_thread_usage,
            // Workspaces
            workspaces::commands::create_workspace,
            workspaces::commands::update_workspace,
            workspaces::commands::move_thread_to_workspace,
            // Threads: reading, turns, lifecycle, subagents, search
            threads::read::read_thread,
            threads::turn::start_thread,
            threads::turn::start_turn,
            threads::turn::interrupt_turn,
            threads::turn::update_turn_settings,
            threads::turn::respond_approval,
            threads::turn::respond_user_input,
            threads::turn::respond_server_request,
            threads::turn::record_user_input_request,
            threads::turn::threads_with_unanswered_questions,
            threads::turn::threads_with_active_turns,
            threads::lifecycle::rename_thread,
            threads::autoname::auto_name_thread,
            threads::lifecycle::invalidate_thread_cache,
            threads::lifecycle::compact_thread,
            threads::lifecycle::start_review,
            threads::lifecycle::archive_thread,
            threads::lifecycle::unarchive_thread,
            threads::lifecycle::delete_thread,
            threads::lifecycle::list_archived_threads,
            threads::lifecycle::list_models,
            threads::lifecycle::list_harness_models,
            threads::lifecycle::read_claude_status,
            threads::lifecycle::fork_thread,
            threads::lifecycle::rollback_thread,
            threads::lifecycle::revert_thread,
            threads::queue::queue_add,
            threads::queue::queue_list,
            threads::queue::queue_update,
            threads::queue::queue_delete,
            threads::queue::queue_reorder,
            threads::queue::queue_start,
            // Thread sections (Codex ≥0.149)
            threads::sections::create_thread_section,
            threads::sections::update_thread_section,
            threads::sections::delete_thread_section,
            threads::sections::move_thread_to_section,
            threads::lifecycle::thread_goal_set,
            threads::lifecycle::thread_goal_get,
            threads::lifecycle::thread_goal_clear,
            threads::subagents::list_subagents,
            threads::subagents::update_subagent_policy,
            threads::side_questions::add_side_question,
            threads::branches::add_thread_branch,
            threads::branches::set_thread_branch_edit_turn,
            threads::side_questions::remove_side_question,
            threads::search::search_threads,
            threads::search::list_threads_page,
            // Files and mentions
            files::search_project_files,
            files::list_project_files,
            // Composer: drafts, attachments, quick chat
            composer::drafts::save_draft,
            composer::drafts::load_draft,
            composer::drafts::delete_draft,
            composer::attachments::stage_attachment,
            composer::attachments::stage_clipboard_image,
            composer::attachments::remove_staged,
            composer::quick::get_quick_shortcut,
            composer::quick::set_quick_shortcut,
            composer::quick::quick_open_full_thread,
            // Settings, runtime identity, and the launch picker
            settings::commands::read_runtime_settings,
            settings::commands::update_runtime_settings,
            settings::commands::read_launch_state,
            settings::commands::read_codex_server_info,
            settings::commands::check_codex_binary,
            settings::commands::list_wsl_distros,
            settings::commands::set_codex_binary,
            settings::commands::select_codex_home,
            settings::commands::open_home_window,
            settings::commands::remove_recent_home,
            settings::commands::read_home_overview,
            settings::commands::read_config_settings,
            settings::commands::write_config_setting,
            settings::commands::read_agent_settings,
            settings::commands::write_agent_settings,
            // App-owned subagents
            agents::commands::list_agent_runs,
            agents::commands::kill_agent_run,
            agents::commands::open_agent_thread,
            // Codex app-server pairing
            codex::pairing::remote_pairing_start,
            codex::pairing::remote_pairing_status,
            // Message log (Advanced settings)
            codex::wire::set_wire_logging,
            codex::wire::read_wire_log,
            codex::wire::clear_wire_log,
            // Git
            git::commands::git_repo_info,
            git::commands::git_status,
            git::commands::git_worktrees,
            git::commands::git_recent_commits,
            git::commands::git_branches,
            git::commands::git_worktree_add,
            git::commands::git_worktree_remove,
            git::commands::git_worktree_prune,
            git::commands::git_worktree_lock,
            git::commands::git_worktree_unlock,
            git::commands::git_changes_summary,
            git::commands::git_file_diff,
            git::commands::git_worktree_handoff_preflight,
            git::commands::git_worktree_handoff,
            git::commands::git_context,
            git::commands::git_stage,
            git::commands::git_unstage,
            git::commands::git_discard,
            git::commands::git_commit,
            git::commands::git_fetch,
            git::commands::git_pull,
            git::commands::git_push,
            git::commands::git_checkout_branch,
            git::commands::git_create_branch,
            git::commands::git_staged_file_diff,
            // Handoff to a terminal or another Codex home
            handoff::commands::handoff_command,
            handoff::commands::handoff_thread_link,
            handoff::commands::handoff_copy,
            handoff::commands::handoff_launch_terminal,
            // Pull request review
            review::commands::review_provider_status,
            review::commands::review_list_prs,
            review::commands::review_pr_detail,
            review::commands::review_check_fresh,
            review::commands::review_local_diff,
            review::commands::review_submit,
            review::commands::review_reply,
            review::commands::review_resolve_thread,
            review::commands::review_save_draft,
            review::commands::review_load_draft,
            review::commands::review_delete_draft,
            // Project sources and workspace search
            sources::save_project_instructions,
            sources::list_project_sources,
            sources::add_project_source,
            sources::remove_project_source,
            sources::reindex_source,
            sources::search_workspace,
            // Paired devices
            connections::commands::list_connections,
            connections::commands::refresh_connections,
            connections::commands::rename_connection,
            connections::commands::disconnect_connection,
            connections::commands::revoke_connection,
            // MCP servers and skills
            integrations::commands::list_integrations,
            integrations::commands::save_mcp_server,
            integrations::commands::remove_mcp_server,
            integrations::commands::set_mcp_enabled,
            // Live state only Codex can answer for: startup status, tool
            // schemas, and OAuth.
            integrations::app_server::list_mcp_server_status,
            integrations::app_server::mcp_oauth_login,
            integrations::app_server::reload_mcp_servers,
            integrations::app_server::list_skills_for,
            integrations::scope::set_integration_enabled,
            integrations::app_server::set_skill_enabled,
            // Skill files on disk: read, scaffold, delete (user scope only).
            integrations::skills_fs::read_skill,
            integrations::skills_fs::create_skill,
            integrations::skills_fs::delete_skill,
            // Desktop integration
            os::reveal_in_finder,
            os::open_external_url,
            os::open_in_zed,
        ])
        // Everything the backend pushes at the webview; see `codex::events`.
        .events(tauri_specta::collect_events![
            codex::events::CodexEvent,
            codex::events::CodexServerRequest,
            codex::events::CodexDisconnected,
            harness::HarnessEventEnvelope,
            harness::HarnessRequestEnvelope,
            agents::supervisor::CodexAgentRun,
        ])
}

/// Regenerates `src/lib/bindings.ts`. Run via `deno task typegen`.
#[cfg(test)]
#[test]
fn export_bindings() {
    write_bindings();
}

fn write_bindings() {
    let path = "../src/lib/bindings.ts";
    specta_builder()
        .export(
            specta_typescript::Typescript::default()
                .header("// @ts-nocheck\n// Generated by tauri-specta from src-tauri — do not edit by hand."),
            path,
        )
        .expect("failed to export TypeScript bindings");
    // HomeWindow is dependency injection with an optional explicit route.
    // Specta exports Option as nullable, but callers may omit the route to
    // use the window's Profile default.
    let bindings = std::fs::read_to_string(path)
        .expect("read exported bindings")
        .replace(
            "window: {\n\thomeKey: string,\n} | null",
            "window: HomeRoute | null",
        );
    let mut arguments = std::collections::BTreeMap::<String, Vec<String>>::new();
    let mut pending = String::new();
    for line in bindings.lines() {
        if pending.is_empty() && !(line.starts_with('\t') && line.contains(": (")) {
            continue;
        }
        pending.push_str(line);
        if !pending.contains(") => __TAURI_INVOKE") {
            continue;
        }
        let declaration = std::mem::take(&mut pending);
        let Some((name, signature)) = declaration.trim().split_once(": (") else {
            continue;
        };
        let Some((signature, _)) = signature.split_once(") => __TAURI_INVOKE") else {
            continue;
        };
        let mut depth = 0;
        let mut start = 0;
        let mut parameters = Vec::new();
        for (index, ch) in signature
            .char_indices()
            .chain(std::iter::once((signature.len(), ',')))
        {
            match ch {
                '<' | '{' | '[' | '(' => depth += 1,
                '>' | '}' | ']' | ')' => depth -= 1,
                ',' if depth == 0 => {
                    if let Some((name, _)) = signature[start..index].trim().split_once(':') {
                        parameters.push(name.trim().to_string());
                    }
                    start = index + 1;
                }
                _ => {}
            }
        }
        arguments.insert(name.to_string(), parameters);
    }
    let bindings = format!("{}\n/** Command argument names for explicit Home routing. */\nexport const commandArguments = {} as const;\n",
        bindings.replace("window: HomeRoute | null", "window: HomeRoute | null = null"),
        serde_json::to_string(&arguments).expect("command argument metadata"));
    std::fs::write(path, bindings).expect("write optional Home routes");
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let (runtime, launch_explicit) = settings::runtime::parse_runtime();
    let database = tauri::async_runtime::block_on(storage::open_profile_root(
        &runtime.host,
        &runtime.codex_home_str(),
    ))
    .expect("error while opening Pingex database");
    // Evict stale staged attachments so the directory stays bounded across runs.
    composer::attachments::cleanup_on_startup(&runtime.host.to_local(&runtime.codex_home_str()));
    // Agent processes died with the previous run, so any row still claiming to
    // be running never will be; otherwise the GUI shows a permanent spinner.
    let _ = tauri::async_runtime::block_on(storage::orphan_running_agent_runs(&database));
    // An explicitly-chosen home boots straight away; record it so it shows up
    // in the picker on a later, non-explicit launch.
    if launch_explicit {
        let _ = settings::prefs::record_recent_home(
            &settings::prefs::settings_path(),
            &runtime.codex_home_str(),
            &runtime.host,
            unix_secs(),
        );
    }
    let context = HomeContext::new(runtime, database);
    let specta = specta_builder();
    // Regenerate the frontend bindings on every debug launch; the file is
    // committed so `deno task check` works without a Rust toolchain.
    #[cfg(debug_assertions)]
    write_bindings();
    let invoke_handler = specta.invoke_handler();
    tauri::Builder::default()
        // Single-instance must be registered first so a second launch forwards
        // its `codex://` argument to the already-running window instead of
        // opening a new one.
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            for arg in argv.iter().skip(1) {
                if arg.starts_with("codex://") {
                    handoff::handle_deep_link_url(app, arg);
                }
            }
            // Focus the main window, else any other app window that is open.
            let window = app
                .get_webview_window("main")
                .or_else(|| app.webview_windows().into_values().next());
            if let Some(window) = window {
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(move |app| {
            // Typed events panic on emit until their registry is mounted, so
            // this precedes anything that could spawn an app-server child.
            specta.mount_events(app);
            // Register the `codex://` scheme at runtime (needed on Linux/Windows
            // dev; a no-op where the OS already routes via the bundle config).
            #[cfg(any(target_os = "linux", all(debug_assertions, windows)))]
            {
                use tauri_plugin_deep_link::DeepLinkExt;
                let _ = app.deep_link().register("codex");
            }
            // Quick-chat and global hotkeys: register the persisted (or
            // default) shortcut that toggles the floating composer.
            composer::quick::register_saved_shortcut(app.handle());
            Ok(())
        })
        .manage(AppState::new(context, launch_explicit))
        // Closing a window releases its home: when no other window shares the
        // context (and it is not the default), its agents and app-server die.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                let state = window.app_handle().state::<AppState>();
                if let Some(orphaned) = state.unbind_window(window.label()) {
                    orphaned.shutdown();
                }
            }
        })
        .invoke_handler(invoke_handler)
        .build(tauri::generate_context!())
        .expect("error while running Pingex")
        .run(|app, event| {
            match &event {
                tauri::RunEvent::Exit => {
                    if let Some(state) = app.try_state::<AppState>() {
                        for context in state.all_contexts() {
                            context.shutdown();
                        }
                    }
                }
                // macOS delivers `codex://` links opened while (or as) the app
                // launches through this event; other platforms use the
                // single-instance plugin.
                #[cfg(any(target_os = "macos", target_os = "ios"))]
                tauri::RunEvent::Opened { urls } => {
                    for url in urls {
                        handoff::handle_deep_link_url(app, url.as_str());
                    }
                }
                _ => {}
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    async fn state_with_home(dir: &Path) -> AppState {
        let database = storage::open(dir).await.expect("open db");
        let context = HomeContext::new(
            RuntimeConfig {
                codex_home: dir.to_path_buf(),
                codex_binary: PathBuf::from("codex"),
                host: Host::Native,
            },
            database,
        );
        let mut state = AppState::new(context, true);
        state.profile_cache_root = dir.join("profiles");
        state
    }

    fn native(path: &Path) -> (Host, String) {
        (Host::Native, path.display().to_string())
    }

    #[tokio::test]
    async fn routes_are_profile_scoped_and_resolved_contexts_survive_navigation() {
        let directory = tempfile::tempdir().unwrap();
        let state = state_with_home(directory.path()).await;
        let root = state.default_context();
        let member = HomeContext::in_profile(
            RuntimeConfig {
                host: Host::wsl("Ubuntu"),
                codex_home: "/home/u/.codex".into(),
                codex_binary: "codex".into(),
            },
            root.database(),
            "member".into(),
            root.profile_key.clone(),
            root.claude.runtime(),
            Some(harness::HarnessKind::Codex),
        );
        state.insert_context(member.clone());
        let captured = state.context_for_route("main", Some("member")).unwrap();
        assert!(Arc::ptr_eq(&captured, &member));
        let other = tempfile::tempdir().unwrap();
        let foreign = state
            .ensure_context(Host::Native, other.path().display().to_string())
            .await
            .unwrap();
        assert!(state
            .context_for_route("main", Some(&foreign.home_key))
            .is_err());
        state.bind_window("main", &foreign.home_key);
        assert!(state.context_for_route("main", Some("member")).is_err());
        assert_eq!(captured.host(), Host::wsl("Ubuntu"));
        assert_eq!(captured.profile_key, root.profile_key);
    }

    #[tokio::test]
    async fn contexts_are_keyed_canonically_and_reused() {
        let home = tempfile::tempdir().unwrap();
        let state = state_with_home(home.path()).await;
        // The same home through a non-canonical spelling lands on one context.
        let alias = home
            .path()
            .join(".")
            .join("..")
            .join(home.path().file_name().unwrap());
        let (host, alias) = native(&alias);
        let reused = state.ensure_context(host, alias).await.unwrap();
        assert_eq!(reused.home_key, state.default_context().home_key);
        assert_eq!(state.all_contexts().len(), 1);
    }

    #[tokio::test]
    async fn a_context_is_released_only_when_its_last_window_unbinds() {
        let home = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let state = state_with_home(home.path()).await;
        let (host, other) = native(other.path());
        let context = state.ensure_context(host, other).await.unwrap();
        let key = context.home_key.clone();

        assert!(state.bind_window("main-2", &key).is_none());
        assert!(state.bind_window("main-3", &key).is_none());
        // One window still bound: nothing to release.
        assert!(state.unbind_window("main-2").is_none());
        assert!(state.context_for_home(&key).is_some());
        // Last one out drops the context from the registry.
        let released = state.unbind_window("main-3").expect("orphaned context");
        assert_eq!(released.home_key, key);
        assert!(state.context_for_home(&key).is_none());
    }

    #[tokio::test]
    async fn a_wsl_home_key_never_collides_with_the_native_one() {
        // A WSL home cannot be opened here (no `wsl.exe`), but its key is
        // computed before anything spawns: the lexical fallback plus the
        // distribution prefix.
        let key = home_key_for(&Host::wsl("Ubuntu"), "/home/u//.codex/");
        assert_eq!(key, "wsl:Ubuntu:/home/u/.codex");
        assert_ne!(key, home_key_for(&Host::Native, "/home/u/.codex"));
    }

    #[tokio::test]
    async fn the_default_context_survives_unbinding() {
        let home = tempfile::tempdir().unwrap();
        let state = state_with_home(home.path()).await;
        // `main` was bound at construction (explicit launch); closing it must
        // not tear down the default context quick chat relies on.
        assert!(state.unbind_window("main").is_none());
        assert_eq!(state.all_contexts().len(), 1);
    }

    #[tokio::test]
    async fn rebinding_a_window_releases_the_home_it_left() {
        let home = tempfile::tempdir().unwrap();
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let state = state_with_home(home.path()).await;
        let (host, a) = native(a.path());
        let first = state.ensure_context(host, a).await.unwrap();
        let (host, b) = native(b.path());
        let second = state.ensure_context(host, b).await.unwrap();
        assert!(state.bind_window("main-2", &first.home_key).is_none());
        // Switching the window's home orphans the first context.
        let released = state
            .bind_window("main-2", &second.home_key)
            .expect("orphaned context");
        assert_eq!(released.home_key, first.home_key);
        assert!(state.context_for_home(&second.home_key).is_some());
    }
}
