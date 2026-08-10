//! Commands for project instructions, attached sources, and workspace search.
//!
//! Indexing and every filesystem walk happen here (or in `indexer`), never in
//! the Svelte renderer. A source is indexed asynchronously on a background task;
//! its `status` moves pending -> indexed | error and the frontend is nudged to
//! refresh via a `sources://updated` event.

use std::collections::BTreeSet;
use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use turso::Database;

use crate::files::fuzzy;
use crate::storage::{self, StoredProjectSource};
use crate::util::id::unique_suffix;
use crate::AppState;

pub(crate) mod indexer;

/// How many items each search group returns per page.
const SEARCH_PAGE: usize = 20;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FileMatch {
    /// Path relative to the project root (folder) or the source name (file).
    path: String,
    file_name: String,
    /// Present for a content match; absent for a file-name match.
    line_number: Option<i64>,
    preview: Option<String>,
    /// True when the file name (not its content) matched the query.
    name_match: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ThreadMatch {
    thread_id: String,
    title: String,
    cwd: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SearchGroup<T> {
    items: Vec<T>,
    next_cursor: Option<String>,
    has_more: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorkspaceResults {
    project_files: SearchGroup<FileMatch>,
    threads: SearchGroup<ThreadMatch>,
    /// Matching thread-message text. Empty here — a full thread-content index is
    /// out of scope — but shaped for cursor paging so it can be extended later.
    messages: SearchGroup<ThreadMatch>,
    /// Echo of the client's generation token so stale responses can be dropped.
    generation: u64,
}

/// Parse an opaque cursor into a row offset. A missing/garbage cursor means the
/// first page.
fn cursor_offset(cursor: Option<&str>) -> usize {
    cursor.and_then(|value| value.parse().ok()).unwrap_or(0)
}

/// Split a fetched window (which contains up to `limit + 1` rows) into the page
/// items plus the next cursor when there is more.
fn paginate<T>(mut rows: Vec<T>, offset: usize, limit: usize) -> (Vec<T>, bool, Option<String>) {
    let has_more = rows.len() > limit;
    if has_more {
        rows.truncate(limit);
    }
    let next_cursor = has_more.then(|| (offset + limit).to_string());
    (rows, has_more, next_cursor)
}

fn build_preview(content: &str) -> String {
    content.chars().take(200).collect()
}

async fn index_source_now(
    database: &Database,
    source: &StoredProjectSource,
) -> Result<i64, String> {
    let root = PathBuf::from(&source.source_path);
    let kind = source.kind.clone();
    let lines = tauri::async_runtime::spawn_blocking(move || indexer::index_source(&root, &kind))
        .await
        .map_err(|error| format!("Indexing failed: {error}"))?;
    let doc_count = lines
        .iter()
        .map(|line| line.file_path.as_str())
        .collect::<BTreeSet<_>>()
        .len() as i64;
    storage::replace_source_lines(database, &source.id, &source.project_path, &lines).await?;
    Ok(doc_count)
}

/// Kick off (or refresh) the index for one source on a background task, moving
/// its status to indexed/error and emitting `sources://updated` when done.
fn spawn_index(app: AppHandle, database: Database, source: StoredProjectSource) {
    tauri::async_runtime::spawn(async move {
        let project_path = source.project_path.clone();
        let result = index_source_now(&database, &source).await;
        let _ = match result {
            Ok(doc_count) => {
                storage::set_source_status(
                    &database,
                    &source.id,
                    "indexed",
                    Some(crate::util::time::unix_secs()),
                    doc_count,
                    None,
                )
                .await
            }
            Err(error) => {
                storage::set_source_status(&database, &source.id, "error", None, 0, Some(&error))
                    .await
            }
        };
        let _ = app.emit("sources://updated", project_path);
    });
}

#[tauri::command]
pub(crate) async fn save_project_instructions(
    project_path: String,
    instructions: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    storage::write_project_instructions(&state.database(), &project_path, &instructions).await
}

#[tauri::command]
pub(crate) async fn list_project_sources(
    project_path: String,
    state: State<'_, AppState>,
) -> Result<Vec<StoredProjectSource>, String> {
    storage::read_project_sources(&state.database(), &project_path).await
}

#[tauri::command]
pub(crate) async fn add_project_source(
    project_path: String,
    source_path: String,
    kind: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<StoredProjectSource>, String> {
    if kind != "folder" && kind != "file" {
        return Err(format!("Unknown source kind: {kind}"));
    }
    let path = PathBuf::from(&source_path);
    let canonical = std::fs::canonicalize(&path)
        .map_err(|error| format!("Could not open {source_path}: {error}"))?;
    let matches_kind = if kind == "folder" {
        canonical.is_dir()
    } else {
        canonical.is_file()
    };
    if !matches_kind {
        return Err(format!("{} is not a {kind}", canonical.display()));
    }
    let canonical = canonical.display().to_string();
    let database = state.database();
    let existing = storage::read_project_sources(&database, &project_path).await?;
    if existing
        .iter()
        .any(|source| source.source_path == canonical)
    {
        return Err("That source is already attached".to_string());
    }
    let source = StoredProjectSource {
        id: format!("src-{}", unique_suffix()),
        project_path: project_path.clone(),
        source_path: canonical,
        kind,
        added_at: crate::util::time::unix_secs(),
        status: "pending".into(),
        indexed_at: None,
        doc_count: 0,
        error: None,
    };
    storage::insert_project_source(&database, &source).await?;
    spawn_index(app, database.clone(), source);
    storage::read_project_sources(&database, &project_path).await
}

#[tauri::command]
pub(crate) async fn remove_project_source(
    id: String,
    project_path: String,
    state: State<'_, AppState>,
) -> Result<Vec<StoredProjectSource>, String> {
    let database = state.database();
    storage::delete_project_source(&database, &id).await?;
    storage::read_project_sources(&database, &project_path).await
}

#[tauri::command]
pub(crate) async fn reindex_source(
    id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let database = state.database();
    let Some(source) = storage::read_source(&database, &id).await? else {
        return Err("Source no longer exists".to_string());
    };
    storage::set_source_status(&database, &id, "pending", None, 0, None).await?;
    spawn_index(app, database, source);
    Ok(())
}

#[tauri::command]
pub(crate) async fn search_workspace(
    project_path: String,
    query: String,
    cursor: Option<String>,
    generation: Option<u64>,
    state: State<'_, AppState>,
) -> Result<WorkspaceResults, String> {
    let generation = generation.unwrap_or(0);
    let trimmed = query.trim().to_string();
    let offset = cursor_offset(cursor.as_deref());
    if trimmed.is_empty() {
        return Ok(WorkspaceResults {
            project_files: SearchGroup {
                items: Vec::new(),
                next_cursor: None,
                has_more: false,
            },
            threads: SearchGroup {
                items: Vec::new(),
                next_cursor: None,
                has_more: false,
            },
            messages: SearchGroup {
                items: Vec::new(),
                next_cursor: None,
                has_more: false,
            },
            generation,
        });
    }
    let database = state.database();

    // Content matches (paged). One extra row tells us whether there is more.
    let content_hits =
        storage::search_index_lines(&database, &project_path, &trimmed, SEARCH_PAGE, offset)
            .await?;
    let (content_hits, files_more, files_cursor) = paginate(content_hits, offset, SEARCH_PAGE);
    let mut file_items: Vec<FileMatch> = Vec::new();
    // File-name matches only surface on the first page so paging stays simple.
    if offset == 0 {
        let root = PathBuf::from(&project_path);
        if root.is_dir() {
            let query_for_names = trimmed.clone();
            let name_hits = tauri::async_runtime::spawn_blocking(move || {
                fuzzy::search_files(&root, &query_for_names, SEARCH_PAGE)
            })
            .await
            .map_err(|error| format!("File search failed: {error}"))?;
            file_items.extend(name_hits.into_iter().filter(|hit| !hit.is_dir).map(|hit| {
                FileMatch {
                    path: hit.path,
                    file_name: hit.file_name,
                    line_number: None,
                    preview: None,
                    name_match: true,
                }
            }));
        }
    }
    file_items.extend(content_hits.into_iter().map(|hit| FileMatch {
        preview: Some(build_preview(&hit.content)),
        line_number: Some(hit.line_number),
        path: hit.file_path,
        file_name: hit.file_name,
        name_match: false,
    }));

    // Local chats: cached thread-summary title matches under this project.
    let thread_hits =
        storage::search_thread_summaries(&database, &project_path, &trimmed, SEARCH_PAGE, offset)
            .await?;
    let (thread_hits, threads_more, threads_cursor) = paginate(thread_hits, offset, SEARCH_PAGE);
    let thread_items: Vec<ThreadMatch> = thread_hits
        .into_iter()
        .map(|summary| ThreadMatch {
            thread_id: summary.id,
            title: summary.title,
            cwd: summary.cwd,
        })
        .collect();

    Ok(WorkspaceResults {
        project_files: SearchGroup {
            items: file_items,
            next_cursor: files_cursor,
            has_more: files_more,
        },
        threads: SearchGroup {
            items: thread_items,
            next_cursor: threads_cursor,
            has_more: threads_more,
        },
        messages: SearchGroup {
            items: Vec::new(),
            next_cursor: None,
            has_more: false,
        },
        generation,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_offset_defaults_to_zero() {
        assert_eq!(cursor_offset(None), 0);
        assert_eq!(cursor_offset(Some("garbage")), 0);
        assert_eq!(cursor_offset(Some("40")), 40);
    }

    #[test]
    fn paginate_reports_more_and_next_cursor() {
        // limit + 1 rows fetched -> there is another page.
        let (items, more, next) = paginate(vec![1, 2, 3], 0, 2);
        assert_eq!(items, vec![1, 2]);
        assert!(more);
        assert_eq!(next.as_deref(), Some("2"));

        // Exactly `limit` rows -> last page.
        let (items, more, next) = paginate(vec![1, 2], 20, 2);
        assert_eq!(items, vec![1, 2]);
        assert!(!more);
        assert!(next.is_none());
    }
}
