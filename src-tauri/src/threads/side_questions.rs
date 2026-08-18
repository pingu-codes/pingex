//! Side questions: throwaway threads spawned from a parent thread and shown
//! under it, rather than as top-level entries in the project.

use tauri::State;

use crate::projects::{bootstrap_cached, BootstrapData};
use crate::storage::{self, SideQuestion};
use crate::util::time::unix_secs;
use crate::AppState;

/// Side-question titles are one-line labels in the sidebar, so they are capped.
pub const MAX_TITLE_CHARS: usize = 120;

#[tauri::command]
pub(crate) async fn add_side_question(
    parent_thread_id: String,
    side_thread_id: String,
    title: String,
    state: State<'_, AppState>,
) -> Result<BootstrapData, String> {
    storage::add_side_question(
        &state.database(),
        &SideQuestion {
            side_thread_id,
            parent_thread_id,
            title: title.trim().chars().take(MAX_TITLE_CHARS).collect(),
            created_at: unix_secs(),
        },
    )
    .await?;
    bootstrap_cached(&state).await
}

/// Stop tracking a thread as a side question. The thread itself survives and
/// reappears as an ordinary thread in its project.
#[tauri::command]
pub(crate) async fn remove_side_question(
    side_thread_id: String,
    state: State<'_, AppState>,
) -> Result<BootstrapData, String> {
    storage::delete_side_question(&state.database(), &side_thread_id).await?;
    bootstrap_cached(&state).await
}
