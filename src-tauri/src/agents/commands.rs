//! What the GUI asks about agent runs.

use tauri::{AppHandle, State};

use crate::agents::supervisor;
use crate::storage::{self, AgentRunRow};
use crate::AppState;

/// Every agent a thread has spawned, oldest first.
///
/// Read from the database rather than the in-memory registry so runs from
/// previous app launches are included; live state arrives separately on
/// `codex:agentRun`.
#[tauri::command]
pub(crate) async fn list_agent_runs(
    thread_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<AgentRunRow>, String> {
    storage::read_agent_runs(&state.database(), &thread_id).await
}

/// Stop a running agent from the GUI.
#[tauri::command]
pub(crate) async fn kill_agent_run(
    run_id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let run = state
        .agents
        .get(&run_id)
        // A run whose process is gone (a previous launch, say) has nothing to
        // kill, but its row may still say `running`; reconcile it instead.
        .ok_or_else(|| "That agent is no longer running.".to_string())?;
    supervisor::kill(&app, &run, Some("stopped from the app")).await;
    Ok(())
}

/// The thread id to open when the user clicks into an agent.
#[tauri::command]
pub(crate) async fn open_agent_thread(
    run_id: String,
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    Ok(storage::read_agent_run(&state.database(), &run_id)
        .await?
        .and_then(|run| run.child_thread_id))
}
