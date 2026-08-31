//! Message-version branches: the forks an edited user message produces, shown
//! as versions of one message rather than as threads of their own.

use tauri::State;

use crate::projects::{bootstrap_cached, BootstrapData};
use crate::storage::{self, ThreadBranch};
use crate::util::time::unix_secs;
use crate::AppState;

/// Record a fork as a version of the message whose turn it replaced. Editing
/// an edit adds to the original message's group rather than nesting.
#[tauri::command]
#[specta::specta]
pub(crate) async fn add_thread_branch(
    parent_thread_id: String,
    thread_id: String,
    replaced_turn_id: String,
    inherited_turns: u32,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<BootstrapData, String> {
    let ctx = state.ctx(&window);
    let database = ctx.database();
    let group_turn_id = storage::branch_group_for_turn(&database, &replaced_turn_id)
        .await?
        .unwrap_or_else(|| replaced_turn_id.clone());
    storage::add_thread_branch(
        &database,
        &ThreadBranch {
            thread_id,
            parent_thread_id,
            group_turn_id,
            replaced_turn_id,
            inherited_turns,
            edit_turn_id: None,
            created_at: unix_secs(),
            updated_at: None,
        },
    )
    .await?;
    bootstrap_cached(&ctx).await
}

/// Remember which turn in a branch is the edited message, once Codex has
/// assigned it an id.
#[tauri::command]
#[specta::specta]
pub(crate) async fn set_thread_branch_edit_turn(
    thread_id: String,
    edit_turn_id: String,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let ctx = state.ctx(&window);
    storage::set_branch_edit_turn(&ctx.database(), &thread_id, &edit_turn_id).await
}
