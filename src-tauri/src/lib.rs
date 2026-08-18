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

use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tauri::Manager;

mod agents;
mod codex;
mod composer;
mod connections;
mod files;
mod git;
mod handoff;
mod integrations;
mod os;
mod projects;
mod review;
mod settings;
mod sources;
mod storage;
mod threads;
mod util;
mod workspaces;

use codex::CodexSession;

/// The pieces the live end-to-end suite (`tests/live_codex.rs`) replays against
/// a real `codex` binary: the exact request payloads the app sends and the
/// parsers it reads responses with. Not an API for anything else.
pub mod e2e {
    pub use crate::agents::supervisor::{collect_model_ids, sandbox_tag, AGENT_PREAMBLE};
    pub use crate::agents::tools::{specs as agent_tool_specs, DELEGATION_POLICY};
    pub use crate::codex::binary::{missing_message, resolve as resolve_codex_binary};
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
}
use util::time::unix_secs;

#[derive(Clone)]
pub(crate) struct RuntimeConfig {
    pub(crate) codex_home: PathBuf,
    pub(crate) codex_binary: PathBuf,
}

/// The active Codex home, shared with `CodexSession` so a home switch is
/// visible the next time the app-server child is (re)spawned.
pub(crate) type SharedRuntime = Arc<RwLock<RuntimeConfig>>;

pub(crate) struct AppState {
    /// Live runtime config; swapped by `select_codex_home` behind a lock.
    runtime: SharedRuntime,
    /// Frontend database, reopened against the active home on a switch.
    database: RwLock<turso::Database>,
    pub(crate) session: CodexSession,
    /// Subagent processes the app owns. Same lifetime as `session`: reached
    /// from Tauri commands and from the session's reader thread.
    pub(crate) agents: agents::supervisor::AgentSupervisor,
    /// Whether the launch home came from `--codex-home`/`CODEX_HOME`; when
    /// false the frontend shows the home picker before booting.
    pub(crate) launch_explicit: bool,
}

impl AppState {
    /// A snapshot of the active runtime config. Cheap to clone and never held
    /// across await points, so a concurrent switch is always observed fresh.
    pub(crate) fn runtime(&self) -> RuntimeConfig {
        self.runtime.read().expect("runtime lock poisoned").clone()
    }

    /// A handle to the active frontend database (an `Arc` clone internally).
    pub(crate) fn database(&self) -> turso::Database {
        self.database
            .read()
            .expect("database lock poisoned")
            .clone()
    }

    /// Point the app at a different Codex home. The caller is responsible for
    /// resetting the session so the next request respawns with the new home.
    pub(crate) fn set_active(&self, runtime: RuntimeConfig, database: turso::Database) {
        *self.runtime.write().expect("runtime lock poisoned") = runtime;
        *self.database.write().expect("database lock poisoned") = database;
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let (runtime, launch_explicit) = settings::runtime::parse_runtime();
    let database = tauri::async_runtime::block_on(storage::open(&runtime.codex_home))
        .expect("error while opening Pingex database");
    // Evict stale staged attachments so the directory stays bounded across runs.
    composer::attachments::cleanup_on_startup(&runtime.codex_home);
    // Agent processes died with the previous run, so any row still claiming to
    // be running never will be; otherwise the GUI shows a permanent spinner.
    let _ = tauri::async_runtime::block_on(storage::orphan_running_agent_runs(&database));
    // An explicitly-chosen home boots straight away; record it so it shows up
    // in the picker on a later, non-explicit launch.
    if launch_explicit {
        let _ = settings::prefs::record_recent_home(
            &settings::prefs::settings_path(),
            &runtime.codex_home.display().to_string(),
            unix_secs(),
        );
    }
    let runtime: SharedRuntime = Arc::new(RwLock::new(runtime));
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
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
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
        .manage(AppState {
            session: CodexSession::new(runtime.clone()),
            agents: agents::supervisor::AgentSupervisor::default(),
            runtime,
            database: RwLock::new(database),
            launch_explicit,
        })
        .invoke_handler(tauri::generate_handler![
            // Projects and the bootstrap payload
            projects::commands::bootstrap,
            projects::commands::add_project,
            projects::commands::rename_project,
            projects::commands::remove_project,
            projects::commands::move_project,
            projects::commands::set_project_pinned,
            projects::commands::set_project_archived,
            projects::commands::set_thread_pinned,
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
            threads::turn::respond_approval,
            threads::turn::respond_user_input,
            threads::turn::respond_server_request,
            threads::turn::record_user_input_request,
            threads::turn::threads_with_unanswered_questions,
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
            threads::lifecycle::fork_thread,
            threads::lifecycle::rollback_thread,
            threads::lifecycle::revert_thread,
            threads::queue::queue_add,
            threads::queue::queue_list,
            threads::queue::queue_update,
            threads::queue::queue_delete,
            threads::queue::queue_reorder,
            threads::queue::queue_start,
            threads::lifecycle::thread_goal_set,
            threads::lifecycle::thread_goal_get,
            threads::lifecycle::thread_goal_clear,
            threads::subagents::list_subagents,
            threads::subagents::update_subagent_policy,
            threads::side_questions::add_side_question,
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
            settings::commands::check_codex_binary,
            settings::commands::set_codex_binary,
            settings::commands::select_codex_home,
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
            integrations::commands::add_mcp_server,
            integrations::commands::remove_mcp_server,
            integrations::commands::set_mcp_enabled,
            // Live state only Codex can answer for: startup status, tool
            // schemas, and OAuth.
            integrations::app_server::list_mcp_server_status,
            integrations::app_server::mcp_oauth_login,
            integrations::app_server::reload_mcp_servers,
            integrations::app_server::list_skills_for,
            integrations::app_server::set_skill_enabled,
            // Desktop integration
            os::reveal_in_finder,
            os::open_external_url,
            os::open_in_zed,
        ])
        .build(tauri::generate_context!())
        .expect("error while running Pingex")
        .run(|app, event| {
            match &event {
                tauri::RunEvent::Exit => {
                    if let Some(state) = app.try_state::<AppState>() {
                        // Agents first: they are children of this process too,
                        // and nothing else will reap them once we are gone.
                        state.agents.kill_all();
                        state.session.kill_child();
                    }
                }
                // macOS delivers `codex://` links opened while (or as) the app
                // launches through this event.
                tauri::RunEvent::Opened { urls } => {
                    for url in urls {
                        handoff::handle_deep_link_url(app, url.as_str());
                    }
                }
                _ => {}
            }
        });
}
