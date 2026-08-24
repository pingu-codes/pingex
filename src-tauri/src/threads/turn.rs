//! Starting threads and running turns, plus the two things a running turn asks
//! back of the user: approvals and `request_user_input` questions.

use serde_json::Value;
use tauri::{AppHandle, State};

use crate::codex::requests;
use crate::util::json::str_at;
use crate::{storage, workspaces, AppState};

pub(crate) use crate::codex::requests::TurnOptions;

#[tauri::command]
pub(crate) async fn start_thread(
    cwd: Option<String>,
    workspace_id: Option<String>,
    app_subagents: Option<bool>,
    app: AppHandle,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let ctx = state.ctx(&window);
    // Surface the containing project's stored instructions to Codex as this
    // thread's developer instructions. The `thread/start` protocol accepts
    // `developerInstructions` (app-server-protocol v2 `ThreadStartParams`), so
    // instructions are a real context hook, not just UI metadata.
    let workspace = match workspace_id.as_deref() {
        Some(workspace_id) => Some(workspaces::runtime_for_workspace(&ctx, workspace_id).await?),
        None => None,
    };
    let cwd = workspace
        .as_ref()
        .map(|workspace| workspace.cwd.clone())
        .or(cwd)
        .filter(|cwd| !cwd.trim().is_empty())
        .ok_or("Choose a project directory before starting a thread")?;
    let mut instructions = storage::read_instructions_for_cwd(&ctx.database(), &cwd)
        .await?
        .unwrap_or_default();
    if let Some(workspace) = &workspace {
        if !instructions.is_empty() {
            instructions.push_str("\n\n");
        }
        instructions.push_str(&workspace.context);
    }
    // The app's own agent tools are declared here or not at all: `dynamicTools`
    // is only accepted on `thread/start`, so this is a property of the thread
    // rather than something that can be toggled part-way through. (Codex does
    // restore them across `thread/resume`, so the choice survives a restart.)
    let app_subagents = app_subagents.unwrap_or_else(|| {
        crate::settings::prefs::read_agent_settings(&crate::settings::prefs::settings_path())
            .enabled
    });
    let mut dynamic_tools = None;
    if app_subagents {
        // Fetched here, while nothing is in flight, so the spawn tool can offer
        // the real slugs as an enum. Best effort: an unavailable list leaves the
        // field free-form rather than blocking the tools entirely.
        let models = ctx
            .session
            .send(&app, requests::model_list(100, false))
            .await
            .map(|response| crate::agents::supervisor::collect_model_ids(&response))
            .unwrap_or_default();
        dynamic_tools = Some(crate::agents::tools::specs(&models));
        // Codex's built-in subagents cannot be switched off, so the delegation
        // policy is what actually steers the model onto ours.
        if !instructions.is_empty() {
            instructions.push_str("\n\n");
        }
        instructions.push_str(crate::agents::tools::DELEGATION_POLICY);
    }
    let mut request = requests::thread_start(
        &cwd,
        workspace
            .as_ref()
            .map(|workspace| workspace.roots.as_slice()),
        Some(&instructions),
        dynamic_tools,
    );
    // File the thread under its sidebar entry's mirrored Codex project from
    // the start (Codex ≥0.149), so the assignment holds even if its cwd
    // later drifts. Unmirrored (older Codex) simply sends no `projectId`.
    let project_key = match &workspace {
        // A workspace's cwd is its hub, which is also its local key.
        Some(workspace) => Some(workspace.cwd.clone()),
        None => {
            let store = storage::read_store(&ctx.database()).await?;
            crate::projects::server::key_for_cwd(
                store.projects.iter().map(|project| project.path.as_str()),
                &cwd,
            )
        }
    };
    let project_id = match project_key.as_deref() {
        Some(key) => crate::projects::server::project_id_for(&ctx, key).await?,
        None => None,
    };
    requests::apply_project(&mut request.params, project_id.as_deref());
    let response = ctx.session.send(&app, request).await?;
    let thread = response
        .get("thread")
        .cloned()
        .ok_or_else(|| "Codex returned no thread data".to_string())?;
    if let Some(id) = str_at(&thread, "id") {
        ctx.session.mark_resumed(&app, id).await?;
        // Remembered now so a `pingex_spawn_agent` on this thread's very first
        // message can bound the agent without asking Codex about a thread whose
        // turn is at that moment blocked waiting for the tool's answer.
        ctx.agents.remember_cwd(id, &cwd);
        if let Some(workspace) = &workspace {
            storage::assign_thread_workspace(&ctx.database(), id, &workspace.workspace_id)
                .await?;
        }
    }
    Ok(thread)
}

#[tauri::command]
pub(crate) async fn start_turn(
    thread_id: String,
    input: Vec<Value>,
    options: Option<TurnOptions>,
    app: AppHandle,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let ctx = state.ctx(&window);
    ctx.session.ensure_resumed(&app, &thread_id).await?;
    storage::invalidate_thread_detail(&ctx.database(), &thread_id).await?;
    let resolved = options
        .as_ref()
        .map(|options| {
            (
                options.resolved_model.clone(),
                options.resolved_effort.clone(),
            )
        })
        .unwrap_or_default();
    let mut request = requests::turn_start(&thread_id, input, options);
    if let Some(workspace_id) = storage::workspace_for_thread(&ctx.database(), &thread_id).await?
    {
        let workspace = workspaces::runtime_for_workspace(&ctx, &workspace_id).await?;
        requests::apply_workspace_params(
            &mut request.params,
            &workspace.cwd,
            &workspace.roots,
            &workspace.context,
        );
        // The workspace hub is where this turn actually runs, so it is also
        // what bounds any agent the turn spawns.
        ctx.agents.remember_cwd(&thread_id, &workspace.cwd);
    }
    let response = ctx.session.send(&app, request).await?;
    let turn = response
        .get("turn")
        .cloned()
        .ok_or_else(|| "Codex returned no turn data".to_string())?;
    if let Some(turn_id) = str_at(&turn, "id") {
        let (model, effort) = &resolved;
        storage::record_turn_settings(
            &ctx.database(),
            &thread_id,
            turn_id,
            model.as_deref(),
            effort.as_deref(),
        )
        .await?;
    }
    Ok(turn)
}

/// The turn id Codex names as actually active, pulled out of a `turn/interrupt`
/// rejection.
///
/// Codex validates the turn id against its own view of the active turn, and the
/// two can legitimately disagree: a review turn emits no `turn/started`, so
/// Codex only adopts it once the first review item is streamed, and until then
/// it still considers the previous turn active. Rather than guess, take the id
/// out of the complaint.
///
/// The interrupt message is unquoted — `turn/steer` reports the same mismatch
/// with the ids in backticks, so this deliberately does not match that shape.
fn active_turn_mismatch(error: &str) -> Option<&str> {
    let found = error
        .split_once("expected active turn id ")?
        .1
        .split_once(" but found ")?
        .1;
    let found = found.trim_end_matches(['"', '}', ' ']);
    (!found.is_empty() && !found.starts_with('`')).then_some(found)
}

#[tauri::command]
pub(crate) async fn interrupt_turn(
    thread_id: String,
    turn_id: String,
    app: AppHandle,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let ctx = state.ctx(&window);
    let mut turn_id = turn_id;
    // One retry, and only for a turn-id mismatch: Codex has told us which turn
    // it considers active, so resending is the same Stop the user asked for
    // rather than a blind repeat.
    for attempt in 0..2 {
        let error = match ctx
            .session
            .send(&app, requests::turn_interrupt(&thread_id, &turn_id))
            .await
        {
            Ok(_) => return Ok(()),
            Err(error) => error,
        };
        // The turn ended on its own between the click and the request. Nothing
        // left to stop, which is the outcome Stop wanted.
        if error.contains("no active turn to interrupt") {
            return Ok(());
        }
        match active_turn_mismatch(&error) {
            Some(active) if attempt == 0 && active != turn_id => turn_id = active.to_string(),
            _ => return Err(error),
        }
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn respond_approval(
    request_id: i64,
    decision: String,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let ctx = state.ctx(&window);
    ctx
        .session
        .respond(request_id, requests::approval_result(&decision))
        .await
}

/// Answer a server request whose response is not a bare `{decision}` — a
/// permission grant, an MCP elicitation. The frontend builds the whole result
/// object because each of these has its own shape, and Codex keeps adding more.
#[tauri::command]
pub(crate) async fn respond_server_request(
    request_id: i64,
    result: Value,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let ctx = state.ctx(&window);
    ctx.session.respond(request_id, result).await
}

/// Record a question the moment Codex asks it, so it is still readable if the
/// app-server (and with it the request) dies before the user answers.
#[tauri::command]
pub(crate) async fn record_user_input_request(
    thread_id: String,
    turn_id: String,
    item_id: String,
    item: Value,
    after_item_id: Option<String>,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let ctx = state.ctx(&window);
    storage::add_pending_user_input(
        &ctx.database(),
        &thread_id,
        &turn_id,
        &item_id,
        &item,
        after_item_id.as_deref(),
    )
    .await
}

#[tauri::command]
pub(crate) async fn threads_with_unanswered_questions(
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    let ctx = state.ctx(&window);
    storage::list_threads_with_unanswered_user_inputs(&ctx.database()).await
}

/// `request_id` is `None` when answering a question whose request died with an
/// earlier session: there is nothing left to respond to, so the answer is only
/// persisted (the caller sends it on as a fresh turn).
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn respond_user_input(
    request_id: Option<i64>,
    answers: Value,
    thread_id: Option<String>,
    turn_id: Option<String>,
    item_id: Option<String>,
    item: Option<Value>,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let ctx = state.ctx(&window);
    if let Some(request_id) = request_id {
        ctx
            .session
            .respond(request_id, requests::user_input_result(answers))
            .await?;
    }
    // Codex's thread/read projection has no item for request_user_input, so the
    // answered question (a client-built item, secrets already masked) is
    // persisted here and merged back in at read time.
    if let (Some(thread_id), Some(turn_id), Some(item_id), Some(item)) =
        (thread_id, turn_id, item_id, item)
    {
        storage::add_user_input_answer(&ctx.database(), &thread_id, &turn_id, &item_id, &item)
            .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use requests::apply_turn_options;
    use serde_json::json;

    #[test]
    fn maps_supported_turn_options() {
        let mut params = json!({});
        apply_turn_options(
            &mut params,
            TurnOptions {
                model: Some("gpt-5.6".into()),
                effort: Some("high".into()),
                approval_policy: Some("on-request".into()),
                sandbox_mode: Some("workspace-write".into()),
                collaboration_mode: Some(json!({"mode": "plan"})),
                subagent_model_policy: Some(json!({"mode": "allow"})),
                subagent_reasoning_effort_policy: Some(json!({"mode": "inherit"})),
                resolved_model: Some("gpt-5.6".into()),
                resolved_effort: Some("high".into()),
            },
        );
        assert_eq!(params["model"], "gpt-5.6");
        assert_eq!(params["effort"], "high");
        assert_eq!(params["approvalPolicy"], "on-request");
        assert_eq!(params["sandboxPolicy"], json!({"type": "workspaceWrite"}));
        assert_eq!(params["collaborationMode"], json!({"mode": "plan"}));
        assert_eq!(params["subagentModelPolicy"], json!({"mode": "allow"}));
        assert_eq!(
            params["subagentReasoningEffortPolicy"],
            json!({"mode": "inherit"})
        );
    }

    #[test]
    fn reads_the_active_turn_out_of_an_interrupt_rejection() {
        // Exactly the shape that reaches us: the error object stringified.
        let error = r#"Codex request failed: {"code":-32600,"message":"expected active turn id 019fd778-0946 but found 019fd778-09cf"}"#;
        assert_eq!(active_turn_mismatch(error), Some("019fd778-09cf"));
    }

    #[test]
    fn ignores_the_steer_mismatch_and_unrelated_errors() {
        // `turn/steer` quotes its ids, and its active turn is not ours to take.
        let steer = r#"Codex request failed: {"code":-32600,"message":"expected active turn id `a` but found `b`"}"#;
        assert_eq!(active_turn_mismatch(steer), None);
        assert_eq!(
            active_turn_mismatch("Codex request failed: client not found"),
            None
        );
        assert_eq!(active_turn_mismatch("expected active turn id a"), None);
    }

    #[test]
    fn ignores_unknown_sandbox_mode() {
        let mut params = json!({});
        apply_turn_options(
            &mut params,
            TurnOptions {
                sandbox_mode: Some("unknown".into()),
                ..TurnOptions::default()
            },
        );
        assert!(params.get("sandboxPolicy").is_none());
    }

    #[test]
    fn maps_each_supported_sandbox_mode() {
        for (input, expected) in [
            ("read-only", "readOnly"),
            ("workspace-write", "workspaceWrite"),
            ("danger-full-access", "dangerFullAccess"),
        ] {
            let mut params = json!({});
            apply_turn_options(
                &mut params,
                TurnOptions {
                    sandbox_mode: Some(input.into()),
                    ..TurnOptions::default()
                },
            );
            assert_eq!(params["sandboxPolicy"]["type"], expected);
        }
    }

    #[test]
    fn the_resolved_pair_is_recorded_locally_not_sent_to_codex() {
        let mut params = json!({});
        apply_turn_options(
            &mut params,
            TurnOptions {
                resolved_model: Some("gpt-5.2".into()),
                resolved_effort: Some("medium".into()),
                ..TurnOptions::default()
            },
        );
        assert_eq!(params, json!({}));
    }

    #[test]
    fn the_agent_tool_specs_are_a_start_only_concern() {
        // `dynamicTools` is not accepted on `turn/start`, so nothing in the
        // per-turn options may try to smuggle it through.
        let mut params = json!({});
        apply_turn_options(&mut params, TurnOptions::default());
        assert!(params.get("dynamicTools").is_none());
    }

    #[test]
    fn absent_options_add_nothing() {
        let mut params = json!({"threadId": "t1"});
        apply_turn_options(&mut params, TurnOptions::default());
        assert_eq!(params, json!({"threadId": "t1"}));
    }
}
