//! The frontend database: everything Pingex persists itself, as opposed to what
//! it reads back from the Codex app-server.
//!
//! One SQLite file per Codex home (`<codex_home>/pingex.db`), so
//! switching homes switches the whole local dataset. Submodules are split by
//! table group; each owns its stored types and the queries over them, and all
//! of them are re-exported here so callers keep saying `storage::read_store(..)`
//! without caring which file it lives in.

use std::fs;
use std::path::{Path, PathBuf};
use turso::{Builder, Database};

pub(crate) mod db;

mod account;
mod agent_runs;
mod items;
mod projects;
mod questions;
mod review;
mod schema;
mod search;
mod sources;
mod threads;
mod turn_settings;
mod workspaces;

pub(crate) use account::{read_account_cache, write_account_cache};
pub(crate) use agent_runs::{
    copy_agent_runs, delete_agent_runs, orphan_running_agent_runs, read_agent_run,
    read_agent_run_children, read_agent_runs, record_agent_run, retain_agent_runs,
    update_agent_run, AgentRunRow, STATUS_DONE, STATUS_FAILED, STATUS_KILLED, STATUS_RUNNING,
};
pub(crate) use items::{
    copy_thread_items, delete_thread_items, mark_turn_complete, read_complete_turns,
    read_running_turns, read_thread_items, record_thread_item, record_turn_start,
    retain_thread_turns, JournaledItem,
};
pub(crate) use projects::{
    read_store, read_temp_worktrees, record_temp_worktree, write_store, Store, StoredProject,
};
pub(crate) use questions::{
    add_pending_user_input, add_side_question, add_user_input_answer, delete_side_question,
    list_threads_with_unanswered_user_inputs, read_side_questions, read_user_input_answers,
    SideQuestion, UserInputAnswer,
};
pub(crate) use review::{delete_review_draft, read_review_draft, write_review_draft, ReviewDraft};
pub(crate) use search::{
    delete_thread_search, rename_thread_search, search_thread_index, set_thread_search_archived,
    upsert_thread_search, StoredThreadSearch,
};
pub(crate) use sources::{
    delete_project_source, insert_project_source, read_all_project_instructions,
    read_all_project_sources, read_instructions_for_cwd, read_project_sources, read_source,
    replace_source_lines, search_index_lines, set_source_status, write_project_instructions,
    IndexedLine, StoredProjectSource,
};
pub(crate) use threads::{
    delete_thread_summary, invalidate_thread_detail, read_hidden_thread_ids, read_thread_detail,
    read_thread_name_source, read_thread_summaries, rename_thread_summary,
    replace_thread_summaries, search_thread_summaries, thread_updated_at, write_thread_detail,
    write_thread_name_source, StoredThreadSummary,
};
pub(crate) use turn_settings::{
    copy_turn_settings, delete_turn_settings, read_turn_settings, record_turn_settings,
    retain_turn_settings, TurnSettings,
};
pub(crate) use workspaces::{
    assign_thread_workspace, create_workspace, read_all_workspace_members, read_workspace_members,
    read_workspaces, unassign_thread_workspace, update_workspace, workspace_for_thread,
    workspace_thread_map, StoredWorkspace, StoredWorkspaceMember,
};

pub(crate) fn database_path(codex_home: &Path) -> PathBuf {
    codex_home.join("pingex.db")
}

fn legacy_database_path(codex_home: &Path) -> PathBuf {
    codex_home.join("pingu-frontend.db")
}

/// Open (creating if needed) the database for `codex_home`, bringing its schema
/// up to date and importing the pre-SQLite JSON store on first run.
pub(crate) async fn open(codex_home: &Path) -> Result<Database, String> {
    fs::create_dir_all(codex_home)
        .map_err(|error| format!("Could not create CODEX_HOME: {error}"))?;
    let path = database_path(codex_home);
    // A database copy is intentionally source-preserving. Pingu Codex should
    // be closed for its final changes to be flushed before first opening Pingex.
    crate::util::migration::copy_file_if_missing(&legacy_database_path(codex_home), &path)?;
    let path = path
        .to_str()
        .ok_or_else(|| format!("Database path is not valid UTF-8: {}", path.display()))?;
    let database = Builder::new_local(path)
        .build()
        .await
        .map_err(|error| format!("Could not open Pingex database: {error}"))?;
    schema::initialize(&database, codex_home).await?;
    Ok(database)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn copies_legacy_database_before_opening_pingex_database() {
        let directory = tempfile::tempdir().unwrap();
        let home = directory.path();
        let database = open(home).await.unwrap();
        drop(database);
        let pingex = database_path(home);
        let legacy = legacy_database_path(home);
        fs::rename(&pingex, &legacy).unwrap();

        let migrated = open(home).await.unwrap();
        drop(migrated);
        assert!(legacy.exists());
        assert!(pingex.exists());
    }
}
