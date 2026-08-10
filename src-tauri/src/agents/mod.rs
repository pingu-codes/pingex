//! App-owned subagents.
//!
//! Codex can spawn subagents itself, but the app cannot see into them or steer
//! them. So when the user turns this on, the app declares its own `pingex_*`
//! tools to the parent agent (as `thread/start.dynamicTools`) and answers the
//! calls by running a genuinely separate `codex` process per agent, which it
//! supervises and shows in the GUI.
//!
//! The whole round trip stays in Rust: `wait_agents` can be outstanding for
//! many minutes, and a response held by the frontend would not survive a
//! webview reload or the user navigating away.

pub(crate) mod commands;
pub(crate) mod supervisor;
pub(crate) mod tools;

use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

use crate::agents::supervisor::AgentRunState;
use crate::util::json::str_at;
use crate::AppState;

/// Answer one `item/tool/call` for a tool we own.
///
/// Always responds, even on failure: an unanswered request blocks the parent's
/// turn, and a model that is told what went wrong can usually recover, whereas
/// one left hanging cannot.
pub(crate) async fn handle_tool_call(app: AppHandle, request_id: i64, params: Value) {
    let response = dispatch(&app, &params).await.unwrap_or_else(tools::render_error);
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let _ = state.session.respond(request_id, response).await;
}

async fn dispatch(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let state = app.try_state::<AppState>().ok_or("The app is shutting down.")?;
    let tool = str_at(params, "tool").ok_or("The tool call named no tool.")?;
    let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

    match tool {
        tools::SPAWN => spawn(app, &state, params, &arguments).await,
        tools::WAIT => wait(&state, &arguments).await,
        tools::SEND_INPUT => send_input(app, &state, &arguments).await,
        tools::KILL => kill(app, &state, &arguments).await,
        other => Err(format!("Unknown tool: {other}")),
    }
}

async fn spawn(
    app: &AppHandle,
    state: &AppState,
    params: &Value,
    arguments: &Value,
) -> Result<Value, String> {
    let args = tools::parse_spawn_args(arguments)?;
    let parent_thread_id = str_at(params, "threadId").ok_or("The tool call named no thread.")?;
    let parent_turn_id = str_at(params, "turnId").unwrap_or_default();
    let call_id = str_at(params, "callId");

    // The parent's own cwd bounds where an agent may run, so it is read from
    // the thread rather than taken from the tool call.
    let parent_cwd = parent_thread_cwd(app, state, parent_thread_id).await?;
    let settings = crate::settings::prefs::read_agent_settings(
        &crate::settings::prefs::settings_path(),
    );

    let run = supervisor::spawn_agent(
        app,
        state,
        &settings,
        supervisor::SpawnContext {
            parent_thread_id,
            parent_turn_id,
            call_id,
            parent_cwd: &parent_cwd,
        },
        args,
    )
    .await?;

    Ok(tools::render_result(
        json!({
            "agentId": run.id,
            "name": run.name,
            "status": "running",
            "note": "Spawn any other agents you need, then call pingex_wait_agents once with all of their ids.",
        }),
        true,
    ))
}

/// Where the parent thread is working — the boundary a spawned agent may not
/// escape.
///
/// The cached summaries are tried first because they cost nothing, but a thread
/// created moments ago is not in them yet (they are refreshed on bootstrap), and
/// spawning from a thread's very first message is the common case. So Codex is
/// asked directly when the cache misses.
async fn parent_thread_cwd(
    app: &AppHandle,
    state: &AppState,
    thread_id: &str,
) -> Result<std::path::PathBuf, String> {
    // Remembered when the thread was started: the only source that is both
    // current and free of a request back to the app-server.
    if let Some(cwd) = state.agents.cwd_for(thread_id) {
        return Ok(std::path::PathBuf::from(cwd));
    }
    let summaries = crate::storage::read_thread_summaries(&state.database()).await?;
    if let Some(summary) = summaries.into_iter().find(|summary| summary.id == thread_id) {
        return Ok(std::path::PathBuf::from(summary.cwd));
    }
    // Last resort — a thread started by a previous run of the app and not yet
    // in the cache. Bounded, because this asks Codex about a thread whose turn
    // is blocked on the very call we are answering.
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        state
            .session
            .request(app, "thread/read", json!({"threadId": thread_id})),
    )
    .await
    .map_err(|_| "Timed out resolving this thread's working directory.".to_string())??;
    let cwd = response
        .get("thread")
        .and_then(|thread| str_at(thread, "cwd"))
        .ok_or_else(|| "Could not resolve this thread's working directory.".to_string())?;
    state.agents.remember_cwd(thread_id, cwd);
    Ok(std::path::PathBuf::from(cwd))
}

async fn wait(state: &AppState, arguments: &Value) -> Result<Value, String> {
    let ids: Vec<String> = arguments
        .get("agentIds")
        .and_then(Value::as_array)
        .map(|ids| {
            ids.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    if ids.is_empty() {
        return Err("`agentIds` must list at least one agent.".into());
    }
    let timeout = tools::wait_timeout_seconds(
        arguments.get("timeoutSeconds").and_then(Value::as_f64),
    );

    let runs: Vec<_> = ids
        .iter()
        .map(|id| (id.clone(), state.agents.get(id)))
        .collect();

    // One deadline shared by every agent, so waiting on N of them costs one
    // timeout rather than N — they are all running concurrently already.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(timeout);
    for run in runs.iter().filter_map(|(_, run)| run.clone()) {
        let mut receiver = run.subscribe();
        while !receiver.borrow().is_terminal() {
            match tokio::time::timeout_at(deadline, receiver.changed()).await {
                // Timed out, or every sender is gone: stop waiting entirely and
                // report whatever each agent has reached.
                Err(_) => break,
                Ok(Err(_)) => break,
                Ok(Ok(())) => {}
            }
        }
        if tokio::time::Instant::now() >= deadline {
            break;
        }
    }

    let mut still_running = false;
    let agents: Vec<Value> = runs
        .into_iter()
        .map(|(id, run)| {
            let Some(run) = run else {
                return json!({"agentId": id, "status": "unknown",
                              "error": "No agent with that id."});
            };
            let state = run.state();
            if !state.is_terminal() {
                still_running = true;
            }
            json!({
                "agentId": run.id,
                "name": run.name,
                "status": state.status(),
                "result": tools::trim_result(&run.last_message()),
                "error": match &state {
                    AgentRunState::Failed(message) => Some(message.clone()),
                    _ => None,
                },
            })
        })
        .collect();

    let mut payload = json!({"agents": agents});
    if still_running {
        payload["note"] = json!(format!(
            "Timed out after {timeout}s with agents still running. This is not a failure — \
             call pingex_wait_agents again with the ids that have not finished."
        ));
    }
    Ok(tools::render_result(payload, true))
}

async fn send_input(app: &AppHandle, state: &AppState, arguments: &Value) -> Result<Value, String> {
    let id = str_at(arguments, "agentId").ok_or("`agentId` is required.")?;
    let text = str_at(arguments, "text").unwrap_or_default().trim().to_string();
    if text.is_empty() {
        return Err("`text` is required and must not be empty.".into());
    }
    let run = state
        .agents
        .get(id)
        .ok_or_else(|| format!("No agent with id {id}."))?;
    supervisor::send_input(app, &run, &text).await?;
    Ok(tools::render_result(
        json!({"agentId": run.id, "status": "running"}),
        true,
    ))
}

async fn kill(app: &AppHandle, state: &AppState, arguments: &Value) -> Result<Value, String> {
    let id = str_at(arguments, "agentId").ok_or("`agentId` is required.")?;
    let reason = str_at(arguments, "reason");
    let run = state
        .agents
        .get(id)
        .ok_or_else(|| format!("No agent with id {id}."))?;
    supervisor::kill(app, &run, reason).await;
    Ok(tools::render_result(
        json!({
            "agentId": run.id,
            "status": run.state().status(),
            "result": tools::trim_result(&run.last_message()),
        }),
        true,
    ))
}
