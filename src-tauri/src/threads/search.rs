//! History search and paging.
//!
//! Two different mechanisms: `list_threads_page` pages the app-server's own
//! listing (forwarding its opaque cursor), while `search_threads` queries the
//! local index so typing stays responsive and works offline.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use tauri::{AppHandle, State};

use crate::codex::requests;
use crate::projects::{thread_search_row, thread_summary_from, ThreadSummary};
use crate::storage::{self, StoredThreadSearch};
use crate::util::json::{arr_or_empty, str_at};
use crate::AppState;

/// Results per search page.
const SEARCH_PAGE: i64 = 20;
/// Default page size when the caller does not ask for one.
const DEFAULT_PAGE_SIZE: u32 = 50;

/// A single page of threads with an opaque cursor to continue from. The cursor
/// is the app-server's own `nextCursor`, forwarded to the frontend so paging
/// stays stateless in the Rust layer.
#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ThreadsPage {
    items: Vec<ThreadSummary>,
    next_cursor: Option<String>,
}

/// Page through the app-server's thread listing, forwarding its opaque cursor.
/// Used for the archived section's `Load more` so large homes stay responsive
/// instead of loading a fixed cap up front. Every page also refreshes the local
/// search index for the threads it touches.
#[tauri::command]
#[specta::specta]
pub(crate) async fn list_threads_page(
    cursor: Option<String>,
    page_size: Option<u32>,
    archived: Option<bool>,
    project_path: Option<String>,
    app: AppHandle,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<ThreadsPage, String> {
    let ctx = state.ctx(&window);
    let archived = archived.unwrap_or(false);
    let response = ctx
        .session
        .send(
            &app,
            requests::thread_list(
                page_size.unwrap_or(DEFAULT_PAGE_SIZE),
                cursor.as_deref(),
                project_path.as_deref(),
                archived,
            ),
        )
        .await?;
    let data = arr_or_empty(&response, "data").to_vec();

    let search_rows: Vec<_> = data
        .iter()
        .filter_map(|thread| thread_search_row(thread, archived))
        .collect();
    storage::upsert_thread_search(&ctx.database(), &search_rows).await?;

    let store = storage::read_store(&ctx.database()).await?;
    let pinned: HashSet<&str> = store.pinned_threads.iter().map(String::as_str).collect();
    // Codex has no idea some of its threads are ours: a side question and a
    // subagent's thread are both ordinary threads in an ordinary cwd. The
    // bootstrap payload filters them, and so must every other listing, or they
    // reappear here as if the user had started them.
    let hidden = storage::read_hidden_thread_ids(&ctx.database()).await?;
    Ok(ThreadsPage {
        items: data
            .iter()
            .filter_map(|thread| thread_summary_from(thread, &pinned))
            .filter(|summary| !hidden.contains(&summary.id))
            .map(|mut summary| {
                summary.hidden = store.hidden_threads.contains(&summary.id);
                summary
            })
            .collect(),
        next_cursor: str_at(&response, "nextCursor").map(str::to_string),
    })
}

#[derive(Default, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct SearchFilter {
    archived: bool,
    project_path: Option<String>,
}

#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ThreadSearchItem {
    id: String,
    title: String,
    preview: String,
    cwd: String,
    updated_at: i64,
    archived: bool,
}

impl From<StoredThreadSearch> for ThreadSearchItem {
    fn from(row: StoredThreadSearch) -> Self {
        Self {
            id: row.thread_id,
            title: row.title,
            preview: row.preview,
            cwd: row.project_path,
            updated_at: row.updated_at,
            archived: row.archived,
        }
    }
}

/// One page of search results plus the total match count (for "N of M"
/// labels), an opaque offset cursor, and the caller's generation echoed back so
/// stale responses from an outrun query can be dropped by the frontend.
#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ThreadSearchPage {
    items: Vec<ThreadSearchItem>,
    next_cursor: Option<String>,
    total: i64,
    generation: u64,
}

/// Search the local index. Queries are cheap LIKE scans, so no true
/// cancellation is needed: the `generation` value is echoed back and the
/// frontend simply ignores results whose generation is older than the latest
/// keystroke.
#[tauri::command]
#[specta::specta]
pub(crate) async fn search_threads(
    query: String,
    cursor: Option<String>,
    filter: Option<SearchFilter>,
    generation: u64,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<ThreadSearchPage, String> {
    let ctx = state.ctx(&window);
    let filter = filter.unwrap_or_default();
    let offset: i64 = cursor
        .as_deref()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    let project_path = filter
        .project_path
        .as_deref()
        .filter(|value| !value.is_empty());
    let (rows, total) = storage::search_thread_index(
        &ctx.database(),
        &query,
        filter.archived,
        project_path,
        offset,
        SEARCH_PAGE,
    )
    .await?;
    let next_offset = offset + rows.len() as i64;
    Ok(ThreadSearchPage {
        items: rows.into_iter().map(ThreadSearchItem::from).collect(),
        next_cursor: (next_offset < total).then(|| next_offset.to_string()),
        total,
        generation,
    })
}
