//! Schema creation, back-compat migrations, and the one-time import of the
//! pre-SQLite JSON store.
//!
//! Every table is created with `IF NOT EXISTS` and every later column addition
//! is an `ALTER TABLE` whose failure is ignored, so this runs unconditionally on
//! every open regardless of how old the database on disk is.

use serde_json::Error as JsonError;
use std::fs;
use std::path::Path;
use turso::Database;

use super::db;
use super::projects::{write_store, Store, StoredProject};

const LEGACY_MIGRATION_KEY: &str = "legacy_json_migrated";

const DDL: &str = "CREATE TABLE IF NOT EXISTS metadata (
     key TEXT PRIMARY KEY,
     value TEXT NOT NULL
 );
 CREATE TABLE IF NOT EXISTS projects (
     path TEXT PRIMARY KEY,
     name TEXT,
     pinned INTEGER NOT NULL DEFAULT 0,
     archived INTEGER NOT NULL DEFAULT 0
 );
 CREATE TABLE IF NOT EXISTS project_expansion (
     project_path TEXT PRIMARY KEY,
     expanded INTEGER NOT NULL
 );
 CREATE TABLE IF NOT EXISTS pinned_threads (
     thread_id TEXT PRIMARY KEY
 );
 CREATE TABLE IF NOT EXISTS thread_summaries (
     thread_id TEXT PRIMARY KEY,
     cwd TEXT NOT NULL,
     title TEXT NOT NULL,
     updated_at INTEGER NOT NULL,
     status TEXT NOT NULL,
     parent_thread_id TEXT,
     agent_nickname TEXT,
     agent_role TEXT
 );
 CREATE INDEX IF NOT EXISTS thread_summaries_cwd_updated
     ON thread_summaries(cwd, updated_at DESC);
 CREATE TABLE IF NOT EXISTS thread_name_sources (
     thread_id TEXT PRIMARY KEY,
     source TEXT NOT NULL
 );
 CREATE TABLE IF NOT EXISTS thread_details (
     thread_id TEXT PRIMARY KEY,
     source_updated_at INTEGER NOT NULL,
     payload TEXT NOT NULL
 );
 CREATE TABLE IF NOT EXISTS side_questions (
     side_thread_id TEXT PRIMARY KEY,
     parent_thread_id TEXT NOT NULL,
     title TEXT NOT NULL,
     created_at INTEGER NOT NULL
 );
 CREATE INDEX IF NOT EXISTS side_questions_parent
     ON side_questions(parent_thread_id, created_at DESC);
 CREATE TABLE IF NOT EXISTS user_input_answers (
     item_id TEXT PRIMARY KEY,
     thread_id TEXT NOT NULL,
     turn_id TEXT NOT NULL,
     payload TEXT NOT NULL,
     created_at INTEGER NOT NULL,
     answered_at INTEGER,
     after_item_id TEXT
 );
 CREATE INDEX IF NOT EXISTS user_input_answers_thread
     ON user_input_answers(thread_id, created_at);
 CREATE TABLE IF NOT EXISTS thread_items (
     item_id TEXT NOT NULL,
     thread_id TEXT NOT NULL,
     turn_id TEXT NOT NULL,
     payload TEXT NOT NULL,
     recorded_at INTEGER NOT NULL,
     after_item_id TEXT,
     PRIMARY KEY (thread_id, item_id)
 );
 CREATE INDEX IF NOT EXISTS thread_items_thread
     ON thread_items(thread_id, recorded_at);
 CREATE TABLE IF NOT EXISTS journaled_turns (
     thread_id TEXT NOT NULL,
     turn_id TEXT NOT NULL,
     complete INTEGER NOT NULL DEFAULT 0,
     PRIMARY KEY (thread_id, turn_id)
 );
 CREATE TABLE IF NOT EXISTS turn_settings (
     thread_id TEXT NOT NULL,
     turn_id TEXT NOT NULL,
     model TEXT,
     reasoning_effort TEXT,
     PRIMARY KEY (thread_id, turn_id)
 );
 CREATE TABLE IF NOT EXISTS agent_runs (
     run_id TEXT PRIMARY KEY,
     parent_thread_id TEXT NOT NULL,
     parent_turn_id TEXT NOT NULL,
     call_id TEXT,
     child_thread_id TEXT,
     name TEXT NOT NULL,
     prompt TEXT NOT NULL,
     cwd TEXT NOT NULL,
     model TEXT,
     reasoning_effort TEXT,
     status TEXT NOT NULL,
     result TEXT,
     error TEXT,
     created_at INTEGER NOT NULL,
     finished_at INTEGER
 );
 CREATE INDEX IF NOT EXISTS agent_runs_parent
     ON agent_runs(parent_thread_id, created_at DESC);
 CREATE TABLE IF NOT EXISTS review_drafts (
     provider TEXT NOT NULL,
     repo TEXT NOT NULL,
     pr_number INTEGER NOT NULL,
     head_sha TEXT NOT NULL,
     payload TEXT NOT NULL,
     updated_at INTEGER NOT NULL,
     PRIMARY KEY (provider, repo, pr_number)
 );
 CREATE TABLE IF NOT EXISTS project_instructions (
     project_path TEXT PRIMARY KEY,
     instructions TEXT NOT NULL,
     updated_at INTEGER NOT NULL
 );
 CREATE TABLE IF NOT EXISTS project_sources (
     id TEXT PRIMARY KEY,
     project_path TEXT NOT NULL,
     source_path TEXT NOT NULL,
     kind TEXT NOT NULL,
     added_at INTEGER NOT NULL,
     status TEXT NOT NULL,
     indexed_at INTEGER,
     doc_count INTEGER NOT NULL DEFAULT 0,
     error TEXT
 );
 CREATE INDEX IF NOT EXISTS project_sources_project
     ON project_sources(project_path);
 CREATE TABLE IF NOT EXISTS index_lines (
     source_id TEXT NOT NULL,
     project_path TEXT NOT NULL,
     file_path TEXT NOT NULL,
     file_name TEXT NOT NULL,
     line_number INTEGER NOT NULL,
     content TEXT NOT NULL
 );
 CREATE INDEX IF NOT EXISTS index_lines_source
     ON index_lines(source_id);
 CREATE INDEX IF NOT EXISTS index_lines_project
     ON index_lines(project_path);
 CREATE TABLE IF NOT EXISTS thread_search (
     thread_id TEXT PRIMARY KEY,
     title TEXT NOT NULL,
     preview TEXT NOT NULL,
     project_path TEXT NOT NULL,
     updated_at INTEGER NOT NULL,
     archived INTEGER NOT NULL DEFAULT 0
 );
 CREATE INDEX IF NOT EXISTS thread_search_archived_updated
     ON thread_search(archived, updated_at DESC);
 CREATE TABLE IF NOT EXISTS workspaces (
     id TEXT PRIMARY KEY,
     name TEXT NOT NULL,
     hub_path TEXT NOT NULL UNIQUE,
     archived INTEGER NOT NULL DEFAULT 0
 );
 CREATE TABLE IF NOT EXISTS workspace_members (
     workspace_id TEXT NOT NULL,
     source_path TEXT NOT NULL,
     effective_path TEXT NOT NULL,
     alias TEXT NOT NULL,
     isolated INTEGER NOT NULL DEFAULT 0,
     branch TEXT,
     ordinal INTEGER NOT NULL,
     PRIMARY KEY (workspace_id, source_path),
     UNIQUE (workspace_id, alias)
 );
 CREATE INDEX IF NOT EXISTS workspace_members_effective
     ON workspace_members(effective_path);
 CREATE TABLE IF NOT EXISTS temp_worktrees (
     path TEXT PRIMARY KEY,
     parent_path TEXT NOT NULL
 );
 CREATE TABLE IF NOT EXISTS workspace_threads (
     thread_id TEXT PRIMARY KEY,
     workspace_id TEXT NOT NULL
 );
 CREATE INDEX IF NOT EXISTS workspace_threads_workspace
     ON workspace_threads(workspace_id);
 CREATE TABLE IF NOT EXISTS server_projects (
     project_id TEXT PRIMARY KEY,
     local_key TEXT NOT NULL UNIQUE
 );
 CREATE TABLE IF NOT EXISTS thread_sections (
     id TEXT PRIMARY KEY,
     name TEXT NOT NULL,
     icon TEXT,
     color TEXT,
     ordinal INTEGER NOT NULL
 );
 CREATE TABLE IF NOT EXISTS sidebar_folders (
     id TEXT PRIMARY KEY,
     scope TEXT NOT NULL,
     parent_id TEXT,
     name TEXT NOT NULL,
     expanded INTEGER NOT NULL DEFAULT 1,
     ordinal INTEGER NOT NULL DEFAULT 0
 );
 CREATE TABLE IF NOT EXISTS sidebar_placements (
     item_key TEXT PRIMARY KEY,
     scope TEXT NOT NULL,
     parent_id TEXT,
     ordinal INTEGER NOT NULL
 );";

/// Columns added after their table shipped. Re-running these on an up-to-date
/// database fails harmlessly ("duplicate column"), which is the only expected
/// error, so the result is ignored.
const ADDED_COLUMNS: [&str; 8] = [
    "ALTER TABLE projects ADD COLUMN archived INTEGER NOT NULL DEFAULT 0",
    "ALTER TABLE thread_summaries ADD COLUMN project_id TEXT",
    "ALTER TABLE thread_summaries ADD COLUMN section_id TEXT",
    "ALTER TABLE thread_summaries ADD COLUMN parent_thread_id TEXT",
    "ALTER TABLE thread_summaries ADD COLUMN agent_nickname TEXT",
    "ALTER TABLE thread_summaries ADD COLUMN agent_role TEXT",
    "ALTER TABLE user_input_answers ADD COLUMN after_item_id TEXT",
    "ALTER TABLE thread_items ADD COLUMN after_item_id TEXT",
];

pub(super) async fn initialize(database: &Database, codex_home: &Path) -> Result<(), String> {
    let connection = db::conn(database)?;
    connection
        .execute_batch(DDL)
        .await
        .map_err(|error| format!("Could not initialize Pingex database: {error}"))?;

    for statement in ADDED_COLUMNS {
        let _ = connection.execute(statement, ()).await;
    }

    // The table used to hold answered questions only, so every pre-existing row
    // is an answer. Backfill on the run that adds the column, otherwise they
    // would all come back as still-unanswered.
    if connection
        .execute(
            "ALTER TABLE user_input_answers ADD COLUMN answered_at INTEGER",
            (),
        )
        .await
        .is_ok()
    {
        let _ = connection
            .execute(
                "UPDATE user_input_answers SET answered_at = created_at WHERE answered_at IS NULL",
                (),
            )
            .await;
    }

    migrate_legacy_json(database, codex_home).await
}

/// Import `pingu-frontend.json` (the pre-SQLite store) exactly once. The file is
/// left in place as a manual escape hatch; a marker row records that it was
/// consumed so a later edit to it is not re-imported.
async fn migrate_legacy_json(database: &Database, codex_home: &Path) -> Result<(), String> {
    let connection = db::conn(database)?;
    if db::exists(
        &connection,
        "SELECT 1 FROM metadata WHERE key = ?",
        (LEGACY_MIGRATION_KEY,),
    )
    .await?
    {
        return Ok(());
    }

    let legacy_path = codex_home.join("pingu-frontend.json");
    if legacy_path.exists() {
        let text = fs::read_to_string(&legacy_path)
            .map_err(|error| format!("Could not read {}: {error}", legacy_path.display()))?;
        let store = parse_legacy_store(&text)
            .map_err(|error| format!("Could not parse {}: {error}", legacy_path.display()))?;
        write_store(database, &store).await?;
    }

    db::exec(
        &connection,
        "INSERT OR REPLACE INTO metadata(key, value) VALUES (?, ?)",
        (LEGACY_MIGRATION_KEY, "1"),
    )
    .await
}

/// The legacy file went through two shapes: an object with `projects`, and
/// before that a bare array of project paths.
fn parse_legacy_store(text: &str) -> Result<Store, JsonError> {
    if let Ok(store) = serde_json::from_str::<Store>(text) {
        return Ok(store);
    }
    let paths = serde_json::from_str::<Vec<String>>(text)?;
    Ok(Store {
        projects: paths
            .into_iter()
            .map(|path| StoredProject {
                path,
                name: None,
                pinned: false,
                archived: false,
            })
            .collect(),
        pinned_threads: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{open, read_store};

    #[test]
    fn parses_plain_path_array_from_legacy_store() {
        let store = parse_legacy_store(r#"["/tmp/one","/tmp/two"]"#).unwrap();
        assert_eq!(store.projects.len(), 2);
        assert_eq!(store.projects[0].path, "/tmp/one");
        assert!(store.pinned_threads.is_empty());
    }

    #[tokio::test]
    async fn migrates_legacy_json_once_without_removing_it() {
        let directory = tempfile::tempdir().unwrap();
        let legacy_path = directory.path().join("pingu-frontend.json");
        fs::write(
            &legacy_path,
            r#"{
                "projects": [{"path": "/tmp/legacy", "name": "Legacy", "pinned": true}],
                "pinnedThreads": ["thread-legacy"]
            }"#,
        )
        .unwrap();

        let database = open(directory.path()).await.unwrap();
        let store = read_store(&database).await.unwrap();
        assert_eq!(store.projects[0].path, "/tmp/legacy");
        assert_eq!(store.pinned_threads, vec!["thread-legacy"]);
        assert!(legacy_path.exists());

        fs::write(&legacy_path, r#"["/tmp/should-not-import"]"#).unwrap();
        drop(database);
        let reopened = open(directory.path()).await.unwrap();
        assert_eq!(read_store(&reopened).await.unwrap(), store);
    }
}
