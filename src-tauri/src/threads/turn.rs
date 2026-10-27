//! Starting threads and running turns, plus the two things a running turn asks
//! back of the user: approvals and `request_user_input` questions.

use tauri::{AppHandle, State};

use crate::codex::compat::Feature;
use crate::codex::requests::{self, Request};
use crate::harness::driver::{unsupported, Driver, RequestAnswer};
use crate::harness::HarnessKind;
use crate::util::json::str_at;
use crate::util::json::Json;
use crate::workspaces::WorkspaceRuntime;
use crate::{storage, workspaces, AppState, HomeContext};

pub(crate) use crate::codex::requests::TurnOptions;

/// Everything `thread/start` needs, already fetched, so the request itself can
/// be built without touching the database or Codex.
struct StartInputs {
    cwd: String,
    workspace: Option<WorkspaceRuntime>,
    /// Stored instructions for the project containing `cwd` (empty when none).
    project_instructions: String,
    /// Codex-side project to file the thread under (`None` before Codex 0.149
    /// or when the cwd is not under a mirrored project).
    project_id: Option<String>,
}

/// A workspace's hub is where its threads run, so it wins over the caller's
/// cwd; a blank cwd means no project has been chosen yet.
fn resolve_start_cwd(
    workspace: Option<&WorkspaceRuntime>,
    cwd: Option<String>,
) -> Result<String, String> {
    workspace
        .map(|workspace| workspace.cwd.clone())
        .or(cwd)
        .filter(|cwd| !cwd.trim().is_empty())
        .ok_or_else(|| "Choose a project directory before starting a thread".to_string())
}

fn append_section(instructions: &mut String, section: &str) {
    if !instructions.is_empty() {
        instructions.push_str("\n\n");
    }
    instructions.push_str(section);
}

/// The `thread/start` request for a set of fetched inputs. Pure: every rule
/// about what the thread is started with lives here.
fn build_start_request(inputs: &StartInputs) -> Request {
    // Surface the containing project's stored instructions to Codex as this
    // thread's developer instructions. The `thread/start` protocol accepts
    // `developerInstructions` (app-server-protocol v2 `ThreadStartParams`), so
    // instructions are a real context hook, not just UI metadata.
    let mut instructions = inputs.project_instructions.clone();
    if let Some(workspace) = &inputs.workspace {
        append_section(&mut instructions, &workspace.context);
    }
    let mut request = requests::thread_start(
        &inputs.cwd,
        inputs
            .workspace
            .as_ref()
            .map(|workspace| workspace.roots.as_slice()),
        Some(&instructions),
    );
    // File the thread under its sidebar entry's mirrored Codex project from
    // the start (Codex ≥0.149), so the assignment holds even if its cwd
    // later drifts. Unmirrored (older Codex) simply sends no `projectId`.
    requests::apply_project(&mut request.params, inputs.project_id.as_deref());
    request
}

/// The I/O half of starting a thread: everything `build_start_request` needs,
/// in the order it has to happen.
async fn fetch_start_inputs(
    ctx: &HomeContext,
    cwd: Option<String>,
    workspace_id: Option<String>,
) -> Result<StartInputs, String> {
    let workspace = match workspace_id.as_deref() {
        Some(workspace_id) => Some(workspaces::runtime_for_workspace(ctx, workspace_id).await?),
        None => None,
    };
    let cwd = resolve_start_cwd(workspace.as_ref(), cwd)?;
    let project_instructions = storage::read_instructions_for_cwd(&ctx.database(), &cwd)
        .await?
        .unwrap_or_default();
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
        Some(key) => crate::projects::server::project_id_for(ctx, key).await?,
        None => None,
    };
    Ok(StartInputs {
        cwd,
        workspace,
        project_instructions,
        project_id,
    })
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn start_thread(
    cwd: Option<String>,
    workspace_id: Option<String>,
    harness: Option<String>,
    app: AppHandle,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<Json, String> {
    let ctx = state.ctx(&window);
    let requested = match harness.as_deref() {
        None => HarnessKind::Codex,
        Some(text) => HarnessKind::parse(text).ok_or_else(|| format!("Unknown harness: {text}"))?,
    };
    if let Some(kind) = ctx.harness_kind {
        if requested != kind {
            return Err("Choose a Home for the selected harness".into());
        }
    }
    if requested != HarnessKind::Codex {
        return start_harness_thread(&ctx, requested, cwd, workspace_id).await;
    }
    let inputs = fetch_start_inputs(&ctx, cwd, workspace_id).await?;
    let request = build_start_request(&inputs);
    let response = ctx.session.send(&app, request).await?;
    let thread = response
        .get("thread")
        .cloned()
        .ok_or_else(|| "Codex returned no thread data".to_string())?;
    if let Some(id) = str_at(&thread, "id") {
        ctx.session.mark_resumed(&app, id, &response).await?;
        if let Some(workspace) = &inputs.workspace {
            storage::assign_thread_workspace(&ctx.database(), id, &workspace.workspace_id).await?;
        }
    }
    Ok(Json(super::read::with_thread_settings(
        thread,
        Some(&response),
    )))
}

/// A harness thread is a row and an id; the process is spawned by its first
/// turn, which is what carries the model and mode to start it with.
async fn start_harness_thread(
    ctx: &HomeContext,
    kind: HarnessKind,
    cwd: Option<String>,
    workspace_id: Option<String>,
) -> Result<Json, String> {
    let workspace = match workspace_id.as_deref() {
        Some(workspace_id) => Some(workspaces::runtime_for_workspace(ctx, workspace_id).await?),
        None => None,
    };
    let cwd = resolve_start_cwd(workspace.as_ref(), cwd)?;
    let id = uuid::Uuid::new_v4().to_string();
    storage::record_harness_thread(
        &ctx.database(),
        &id,
        kind.as_str(),
        &cwd,
        "Untitled thread",
        crate::util::time::unix_secs(),
    )
    .await?;
    if let Some(workspace) = &workspace {
        storage::assign_thread_workspace(&ctx.database(), &id, &workspace.workspace_id).await?;
    }
    Ok(Json(
        serde_json::json!({"id": id, "cwd": cwd, "harness": kind.as_str()}),
    ))
}

/// The driver checks its native transcript before spawning. Journal history
/// prevents silently replacing a conversation whose native session is missing.
async fn start_harness_turn(
    ctx: &HomeContext,
    app: &AppHandle,
    thread: &storage::HarnessThread,
    driver: &dyn Driver,
    input: Vec<Json>,
    options: Option<TurnOptions>,
) -> Result<Json, String> {
    let thread_id = &thread.thread_id;
    let resume = storage::has_thread_items(&ctx.database(), thread_id).await?;
    let resolved = options
        .as_ref()
        .map(|options| {
            (
                options.resolved_model.clone(),
                options.resolved_effort.clone(),
            )
        })
        .unwrap_or_default();
    let parts: Vec<serde_json::Value> = input.into_iter().map(|item| item.0).collect();
    let turn_id = driver
        .start_turn(app, thread, resume, &parts, options.as_ref())
        .await?;
    let (model, effort) = &resolved;
    storage::record_turn_settings(
        &ctx.database(),
        thread_id,
        &turn_id,
        model.as_deref(),
        effort.as_deref(),
    )
    .await?;
    storage::touch_harness_thread(&ctx.database(), thread_id, crate::util::time::unix_secs())
        .await?;
    Ok(Json(
        serde_json::json!({"id": turn_id, "status": "inProgress"}),
    ))
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn start_turn(
    thread_id: String,
    input: Vec<Json>,
    options: Option<TurnOptions>,
    app: AppHandle,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<Json, String> {
    let ctx = state.ctx(&window);
    if let Some((thread, driver)) = ctx.harness_thread(&thread_id).await? {
        return start_harness_turn(&ctx, &app, &thread, driver.as_ref(), input, options).await;
    }
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
    let speed_tier = options
        .as_ref()
        .and_then(|options| options.speed_tier.clone());
    let mut request = requests::turn_start(
        &thread_id,
        input.into_iter().map(|item| item.0).collect(),
        options,
    );
    if let Some(workspace_id) = storage::workspace_for_thread(&ctx.database(), &thread_id).await? {
        let workspace = workspaces::runtime_for_workspace(&ctx, &workspace_id).await?;
        requests::apply_workspace_params(
            &mut request.params,
            &workspace.cwd,
            &workspace.roots,
            &workspace.context,
        );
    }
    let response = ctx.session.send(&app, request).await?;
    if let Some(tier) = speed_tier.as_deref() {
        ctx.session.cache_speed_tier(&app, &thread_id, tier).await?;
    }
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
    Ok(Json(turn))
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
#[specta::specta]
pub(crate) async fn interrupt_turn(
    thread_id: String,
    turn_id: String,
    app: AppHandle,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let ctx = state.ctx(&window);
    if let Some((thread, driver)) = ctx.harness_thread(&thread_id).await? {
        return driver.interrupt(&thread.thread_id).await;
    }
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

/// Change the model and/or reasoning effort of the turn that is running right
/// now (`turn/settings/update`, Codex ≥0.151). Returns the server's
/// `{status}` — `applied` or `targetUnavailable` — and, on a Codex without the
/// API, an error prefixed by `Feature::TURN_SETTINGS.error_prefix` so the
/// frontend can fall back to "applies from the next turn".
#[tauri::command]
#[specta::specta]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn update_turn_settings(
    thread_id: String,
    turn_id: String,
    model: Option<String>,
    effort: Option<String>,
    speed_tier: Option<String>,
    app: AppHandle,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<Json, String> {
    let ctx = state.ctx(&window);
    if let Some((_, driver)) = ctx.harness_thread(&thread_id).await? {
        return Err(unsupported(driver.kind(), "changing a running turn"));
    }
    if let Some(tier) = speed_tier {
        let request = requests::turn_speed_update(
            &thread_id,
            &turn_id,
            &tier,
            model.as_deref(),
            effort.as_deref(),
        );
        return ctx
            .session
            .send_gated(
                &app,
                Feature::LIVE_SPEED,
                request,
                crate::codex::compat::speed_tier_unsupported,
            )
            .await
            .map(Json);
    }
    let request =
        requests::turn_settings_update(&thread_id, &turn_id, model.as_deref(), effort.as_deref());
    ctx.session
        .send_gated(&app, Feature::TURN_SETTINGS, request, |_| None)
        .await
        .map(Json)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn respond_approval(
    request_id: i64,
    decision: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let ctx = state.ctx(&window);
    if let Some(driver) = ctx.drivers.owner_of(request_id) {
        return driver
            .respond(request_id, RequestAnswer::Option(decision))
            .await;
    }
    ctx.session
        .respond(request_id, requests::approval_result(&decision))
        .await
}

/// Answer a server request whose response is not a bare `{decision}` — a
/// permission grant, an MCP elicitation. The frontend builds the whole result
/// object because each of these has its own shape, and Codex keeps adding more.
#[tauri::command]
#[specta::specta]
pub(crate) async fn respond_server_request(
    request_id: i64,
    result: Json,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let ctx = state.ctx(&window);
    if let Some(driver) = ctx.drivers.owner_of(request_id) {
        // A permission-profile answer: a non-empty grant is an allow.
        let granted = result
            .0
            .get("permissions")
            .and_then(serde_json::Value::as_object)
            .is_some_and(|profile| !profile.is_empty());
        let decision = if granted { "accept" } else { "decline" };
        return driver
            .respond(request_id, RequestAnswer::Option(decision.into()))
            .await;
    }
    ctx.session.respond(request_id, result.0).await
}

/// Record a question the moment Codex asks it, so it is still readable if the
/// app-server (and with it the request) dies before the user answers.
#[tauri::command]
#[specta::specta]
pub(crate) async fn record_user_input_request(
    thread_id: String,
    turn_id: String,
    item_id: String,
    item: Json,
    after_item_id: Option<String>,
    window: crate::HomeWindow,
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
#[specta::specta]
pub(crate) async fn threads_with_unanswered_questions(
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    let ctx = state.ctx(&window);
    storage::list_threads_with_unanswered_user_inputs(&ctx.database()).await
}

/// Threads with a turn currently running on this home's Codex child.
///
/// The webview only learns about turns from `turn/started`, so a reload
/// mid-turn (a file link, a dev refresh) comes back with no idea anything is
/// working: the sidebar drops the indicator and `ThreadView` demotes the turn
/// to `interrupted`. The child and its journal outlive the webview, so this
/// hands the set back to reseed it.
#[tauri::command]
#[specta::specta]
pub(crate) async fn threads_with_active_turns(
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    let ctx = state.ctx(&window);
    let mut threads = ctx.session.active_threads().await;
    threads.extend(ctx.drivers.active_threads());
    Ok(threads)
}

/// `request_id` is `None` when answering a question whose request died with an
/// earlier session: there is nothing left to respond to, so the answer is only
/// persisted (the caller sends it on as a fresh turn).
#[tauri::command]
#[specta::specta]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn respond_user_input(
    request_id: Option<i64>,
    answers: Json,
    thread_id: Option<String>,
    turn_id: Option<String>,
    item_id: Option<String>,
    item: Option<Json>,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let ctx = state.ctx(&window);
    if let Some(request_id) = request_id {
        if let Some(driver) = ctx.drivers.owner_of(request_id) {
            driver
                .respond(request_id, RequestAnswer::UserInput(answers.0.clone()))
                .await?;
        } else {
            ctx.session
                .respond(request_id, requests::user_input_result(answers.0))
                .await?;
        }
    }
    // Codex's thread/read projection has no item for request_user_input, so the
    // answered question (a client-built item, secrets already masked) is
    // persisted here and merged back in at read time.
    if let (Some(thread_id), Some(turn_id), Some(item_id), Some(item)) =
        (thread_id, turn_id, item_id, item.map(|item| item.0))
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

    fn workspace(cwd: &str, context: &str) -> WorkspaceRuntime {
        WorkspaceRuntime {
            workspace_id: "ws-1".into(),
            cwd: cwd.into(),
            roots: vec!["/a".into(), "/b".into()],
            context: context.into(),
        }
    }

    fn inputs() -> StartInputs {
        StartInputs {
            cwd: "/proj".into(),
            workspace: None,
            project_instructions: String::new(),
            project_id: None,
        }
    }

    #[test]
    fn workspace_cwd_wins_over_caller_cwd() {
        let ws = workspace("/hub", "");
        assert_eq!(
            resolve_start_cwd(Some(&ws), Some("/elsewhere".into())).unwrap(),
            "/hub"
        );
    }

    #[test]
    fn caller_cwd_used_without_workspace() {
        assert_eq!(
            resolve_start_cwd(None, Some("/proj".into())).unwrap(),
            "/proj"
        );
    }

    #[test]
    fn blank_cwd_is_rejected() {
        assert!(resolve_start_cwd(None, None).is_err());
        assert!(resolve_start_cwd(None, Some("   ".into())).is_err());
    }

    #[test]
    fn builds_a_thread_start_for_the_cwd() {
        let request = build_start_request(&inputs());
        assert_eq!(request.method, "thread/start");
        assert_eq!(request.params, json!({"cwd": "/proj"}));
    }

    #[test]
    fn instructions_order_is_project_then_workspace() {
        let request = build_start_request(&StartInputs {
            workspace: Some(workspace("/hub", "ctx")),
            project_instructions: "proj".into(),
            ..inputs()
        });
        assert_eq!(request.params["developerInstructions"], "proj\n\nctx");
    }

    #[test]
    fn no_separator_when_project_instructions_empty() {
        let request = build_start_request(&StartInputs {
            workspace: Some(workspace("/hub", "ctx")),
            ..inputs()
        });
        assert_eq!(request.params["developerInstructions"], "ctx");
    }

    #[test]
    fn no_instructions_field_when_everything_empty() {
        let request = build_start_request(&inputs());
        assert!(request.params.get("developerInstructions").is_none());
    }

    #[test]
    fn workspace_roots_are_sent_only_with_a_workspace() {
        assert!(build_start_request(&inputs())
            .params
            .get("runtimeWorkspaceRoots")
            .is_none());
        let request = build_start_request(&StartInputs {
            workspace: Some(workspace("/hub", "")),
            ..inputs()
        });
        assert_eq!(request.params["cwd"], "/proj");
        assert_eq!(request.params["runtimeWorkspaceRoots"], json!(["/a", "/b"]));
    }

    #[test]
    fn project_id_is_attached_only_when_known() {
        assert!(build_start_request(&inputs())
            .params
            .get("projectId")
            .is_none());
        let request = build_start_request(&StartInputs {
            project_id: Some("p1".into()),
            ..inputs()
        });
        assert_eq!(request.params["projectId"], "p1");
    }

    #[test]
    fn maps_supported_turn_options() {
        let mut params = json!({});
        apply_turn_options(
            &mut params,
            TurnOptions {
                mode: None,
                model: Some("gpt-5.6".into()),
                effort: Some("high".into()),
                speed_tier: Some("default".into()),
                approval_policy: Some("on-request".into()),
                sandbox_mode: Some("workspace-write".into()),
                approvals_reviewer: Some("auto".into()),
                collaboration_mode: Some(Json(json!({"mode": "plan"}))),
                subagent_model_policy: Some(Json(json!({"mode": "allow"}))),
                subagent_reasoning_effort_policy: Some(Json(json!({"mode": "inherit"}))),
                resolved_model: Some("gpt-5.6".into()),
                resolved_effort: Some("high".into()),
            },
        );
        assert_eq!(params["model"], "gpt-5.6");
        assert_eq!(params["effort"], "high");
        assert_eq!(params["serviceTier"], "default");
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
    fn absent_options_add_nothing() {
        let mut params = json!({"threadId": "t1"});
        apply_turn_options(&mut params, TurnOptions::default());
        assert_eq!(params, json!({"threadId": "t1"}));
    }
}
