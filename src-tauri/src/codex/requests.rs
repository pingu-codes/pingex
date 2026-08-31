//! Every JSON-RPC request this app sends to `codex app-server`, built in one
//! place as plain data.
//!
//! The builders are pure (`&str`/`Value` in, [`Request`] out) so the exact
//! payloads the app sends can be replayed against a real `codex` binary by the
//! live end-to-end suite (`tests/live_codex.rs`) without spinning up Tauri.
//! When adding a request elsewhere in the app, add its builder here and cover
//! it there — the protocol only tells you a field is missing at runtime.

use crate::util::json::Json;
use serde::Deserialize;
use serde_json::{json, Value};

/// One outbound JSON-RPC call: the method name and its `params`.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub method: &'static str,
    pub params: Value,
}

fn request(method: &'static str, params: Value) -> Request {
    Request { method, params }
}

/// The `initialize` handshake params sent right after spawning a child.
pub fn initialize(client_name: &str) -> Request {
    request(
        "initialize",
        json!({
            "clientInfo": {"name": client_name, "title": "Pingex", "version": env!("CARGO_PKG_VERSION")},
            // Deliberately no `extensions: {"openai/form": {}}`: declaring it
            // (0.149+) invites `mode: "openai/form"` elicitations, whose
            // schema `ElicitationCard.svelte` cannot draw. Plain `form` and
            // `url` elicitations arrive without it.
            "capabilities": {"experimentalApi": true},
        }),
    )
}

/// `thread/start`. `developer_instructions` and `dynamic_tools` are only sent
/// when present; `runtime_workspace_roots` when the thread belongs to a
/// workspace.
pub fn thread_start(
    cwd: &str,
    runtime_workspace_roots: Option<&[String]>,
    developer_instructions: Option<&str>,
    dynamic_tools: Option<Value>,
) -> Request {
    let mut params = json!({"cwd": cwd});
    if let Some(roots) = runtime_workspace_roots {
        params["runtimeWorkspaceRoots"] = json!(roots);
    }
    if let Some(tools) = dynamic_tools {
        params["dynamicTools"] = tools;
    }
    if let Some(instructions) = developer_instructions.filter(|text| !text.is_empty()) {
        params["developerInstructions"] = json!(instructions);
    }
    request("thread/start", params)
}

pub fn thread_resume(thread_id: &str) -> Request {
    request("thread/resume", json!({"threadId": thread_id}))
}

pub fn thread_read(thread_id: &str) -> Request {
    request(
        "thread/read",
        json!({"threadId": thread_id, "includeTurns": true}),
    )
}

/// `thread/goal/set`: only the fields given change; the app-server keeps the
/// rest of the goal.
pub fn thread_goal_set(thread_id: &str, objective: Option<&str>, status: Option<&str>) -> Request {
    let mut params = json!({"threadId": thread_id});
    if let Some(objective) = objective {
        params["objective"] = json!(objective);
    }
    if let Some(status) = status {
        params["status"] = json!(status);
    }
    request("thread/goal/set", params)
}

pub fn thread_goal_get(thread_id: &str) -> Request {
    request("thread/goal/get", json!({"threadId": thread_id}))
}

pub fn thread_goal_clear(thread_id: &str) -> Request {
    request("thread/goal/clear", json!({"threadId": thread_id}))
}

pub fn thread_delete(thread_id: &str) -> Request {
    request("thread/delete", json!({"threadId": thread_id}))
}

pub fn thread_archive(thread_id: &str) -> Request {
    request("thread/archive", json!({"threadId": thread_id}))
}

pub fn thread_unarchive(thread_id: &str) -> Request {
    request("thread/unarchive", json!({"threadId": thread_id}))
}

pub fn thread_compact(thread_id: &str) -> Request {
    request("thread/compact/start", json!({"threadId": thread_id}))
}

/// The refusal Codex 0.149 gives `thread/revert` on a thread whose history
/// is not in paginated mode (the default): the method exists, the thread
/// just cannot use it, so the caller falls back to `thread/rollback`.
pub const REVERT_NEEDS_PAGINATED: &str = "only supports paginated threads";

/// `thread/revert`, the successor to `thread/rollback`. Absent from Codex
/// 0.146.0 and earlier; callers go through `Feature::REVERT`.
pub fn thread_revert(thread_id: &str, before_turn_id: &str) -> Request {
    request(
        "thread/revert",
        json!({"threadId": thread_id, "beforeTurnId": before_turn_id}),
    )
}

/// `thread/fork` with history strictly before `before_turn_id`: the fork
/// excludes that turn and everything after it. Turn and item ids carry over
/// verbatim, which the journal copy and message versions both rely on.
pub fn thread_fork_before(thread_id: &str, before_turn_id: &str) -> Request {
    request(
        "thread/fork",
        json!({"threadId": thread_id, "beforeTurnId": before_turn_id}),
    )
}

pub fn thread_rollback(thread_id: &str, num_turns: u32) -> Request {
    request(
        "thread/rollback",
        json!({"threadId": thread_id, "numTurns": num_turns}),
    )
}

/// `thread/list`, newest first. `cwd` narrows to one project.
pub fn thread_list(limit: u32, cursor: Option<&str>, cwd: Option<&str>, archived: bool) -> Request {
    let mut params = json!({
        "limit": limit,
        "sortKey": "updated_at",
        "sortDirection": "desc",
        "archived": archived,
    });
    if let Some(cursor) = cursor {
        params["cursor"] = json!(cursor);
    }
    if let Some(cwd) = cwd {
        params["cwd"] = json!(cwd);
    }
    request("thread/list", params)
}

pub fn model_list(limit: u32, include_hidden: bool) -> Request {
    let mut params = json!({"limit": limit});
    if include_hidden {
        params["includeHidden"] = json!(true);
    }
    request("model/list", params)
}

/// Per-turn overrides the composer can set. All optional: an absent field means
/// "keep whatever the thread already resolved to".
#[derive(Default, Deserialize, Debug, Clone, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct TurnOptions {
    pub model: Option<String>,
    pub effort: Option<String>,
    pub approval_policy: Option<String>,
    pub sandbox_mode: Option<String>,
    pub collaboration_mode: Option<Json>,
    pub subagent_model_policy: Option<Json>,
    pub subagent_reasoning_effort_policy: Option<Json>,
    /// What the composer resolved this turn to run on, including the defaults
    /// it did not have to override. Recorded locally so the transcript can say
    /// what produced each reply; never sent to Codex.
    pub resolved_model: Option<String>,
    pub resolved_effort: Option<String>,
}

/// The UI uses kebab-case sandbox names; the protocol expects camelCase tags.
/// An unrecognised mode yields `None` and is dropped rather than guessed at.
pub fn sandbox_policy_type(mode: &str) -> Option<&'static str> {
    match mode {
        "read-only" => Some("readOnly"),
        "workspace-write" => Some("workspaceWrite"),
        "danger-full-access" => Some("dangerFullAccess"),
        _ => None,
    }
}

pub fn apply_turn_options(params: &mut Value, options: TurnOptions) {
    if let Some(model) = options.model {
        params["model"] = json!(model);
    }
    if let Some(effort) = options.effort {
        params["effort"] = json!(effort);
    }
    if let Some(policy) = options.approval_policy {
        params["approvalPolicy"] = json!(policy);
    }
    if let Some(sandbox_type) = options
        .sandbox_mode
        .as_deref()
        .and_then(sandbox_policy_type)
    {
        params["sandboxPolicy"] = json!({"type": sandbox_type});
    }
    if let Some(mode) = options.collaboration_mode {
        params["collaborationMode"] = mode.0;
    }
    if let Some(policy) = options.subagent_model_policy {
        params["subagentModelPolicy"] = policy.0;
    }
    if let Some(policy) = options.subagent_reasoning_effort_policy {
        params["subagentReasoningEffortPolicy"] = policy.0;
    }
}

/// `turn/start` for the composer: `input` is the frontend's `UserInput[]`,
/// forwarded untouched.
pub fn turn_start(thread_id: &str, input: Vec<Value>, options: Option<TurnOptions>) -> Request {
    let mut params = json!({"threadId": thread_id, "input": input});
    if let Some(options) = options {
        apply_turn_options(&mut params, options);
    }
    request("turn/start", params)
}

/// The workspace fields a turn carries when its thread belongs to a workspace.
/// Membership is authoritative: the frontend cannot silently widen or retain
/// stale roots after a workspace was edited.
pub fn apply_workspace_params(params: &mut Value, cwd: &str, roots: &[String], context: &str) {
    params["cwd"] = json!(cwd);
    params["runtimeWorkspaceRoots"] = json!(roots);
    params["additionalContext"] = json!({
        "pingex_workspace": {"kind": "application", "value": context}
    });
}

/// `turn/settings/update`: change the model and/or effort of a running turn
/// (Codex ≥0.151; gated by `Feature::TURN_SETTINGS`). The params are
/// `deny_unknown_fields` upstream, so only the keys given are sent.
pub fn turn_settings_update(
    thread_id: &str,
    turn_id: &str,
    model: Option<&str>,
    effort: Option<&str>,
) -> Request {
    let mut params = json!({"threadId": thread_id, "turnId": turn_id});
    if let Some(model) = model {
        params["model"] = json!(model);
    }
    if let Some(effort) = effort {
        params["effort"] = json!(effort);
    }
    request("turn/settings/update", params)
}

pub fn turn_interrupt(thread_id: &str, turn_id: &str) -> Request {
    request(
        "turn/interrupt",
        json!({"threadId": thread_id, "turnId": turn_id}),
    )
}

/// The result object answering an approval server request.
pub fn approval_result(decision: &str) -> Value {
    json!({"decision": decision})
}

/// The result object answering an `mcpServer/elicitation/request` — the shape
/// `ElicitationCard.svelte` sends through `respond_server_request`. `content`
/// is the form's values on accept and `null` otherwise.
pub fn elicitation_result(action: &str, content: Option<Value>) -> Value {
    json!({"action": action, "content": content, "_meta": null})
}

/// The result object answering an `item/tool/requestUserInput` request.
pub fn user_input_result(answers: Value) -> Value {
    json!({"answers": answers})
}

pub fn skills_list(cwds: &[String]) -> Request {
    request("skills/list", json!({"cwds": cwds}))
}

/// `skills/list` with `forceReload`, for right after we created or deleted a
/// skill directory ourselves — Codex caches the scan otherwise.
pub fn skills_list_force(cwds: &[String]) -> Request {
    request("skills/list", json!({"cwds": cwds, "forceReload": true}))
}

/// `skills/config/write` requires exactly one of `name` or `path`; we always
/// key by name.
pub fn skill_config_write(name: &str, enabled: bool) -> Request {
    request(
        "skills/config/write",
        json!({"name": name, "enabled": enabled}),
    )
}

pub fn mcp_server_status_list() -> Request {
    request("mcpServerStatus/list", json!({}))
}

pub fn mcp_oauth_login(name: &str) -> Request {
    request("mcpServer/oauth/login", json!({"name": name}))
}

pub fn mcp_config_reload() -> Request {
    request("config/mcpServer/reload", json!({}))
}

/// The throwaway thread the auto-namer uses; started in the temp dir so it
/// inherits neither the project's instructions nor its workspace roots.
pub fn namer_thread_start(instructions: &str) -> Request {
    thread_start(
        &std::env::temp_dir().display().to_string(),
        None,
        Some(instructions),
        None,
    )
}

/// The single cheap turn that names a conversation.
pub fn naming_turn(namer_id: &str, seed: &str, model: Option<&str>) -> Request {
    let mut params = json!({
        "threadId": namer_id,
        "input": [{"type": "text", "text": format!("Name this conversation:\n\n{seed}")}],
        "effort": "low",
        "approvalPolicy": "never",
        "sandboxPolicy": {"type": "readOnly"},
    });
    if let Some(model) = model {
        params["model"] = json!(model);
    }
    request("turn/start", params)
}

/// The thread an app-owned subagent runs in. No `dynamicTools`: an agent that
/// could spawn agents is a fork bomb one prompt away.
pub fn agent_thread_start(cwd: &str, preamble: &str) -> Request {
    thread_start(cwd, None, Some(preamble), None)
}

/// A subagent's first turn. `sandbox_type` is already a protocol tag.
pub fn agent_turn(
    thread_id: &str,
    prompt: &str,
    sandbox_type: &str,
    model: Option<&str>,
    effort: Option<&str>,
) -> Request {
    let mut params = json!({
        "threadId": thread_id,
        "input": [{"type": "text", "text": prompt}],
        "approvalPolicy": "never",
        "sandboxPolicy": {"type": sandbox_type},
    });
    if let Some(model) = model {
        params["model"] = json!(model);
    }
    if let Some(effort) = effort {
        params["effort"] = json!(effort);
    }
    request("turn/start", params)
}

/// A follow-up message to a subagent that has finished its previous turn.
pub fn agent_followup(thread_id: &str, text: &str) -> Request {
    request(
        "turn/start",
        json!({
            "threadId": thread_id,
            "input": [{"type": "text", "text": text}],
            "approvalPolicy": "never",
        }),
    )
}

/// Put a `thread/start` under an app-server project (`projectId`, experimental
/// in Codex ≥0.149). A no-op on an already-built request when `None`.
pub fn apply_project(params: &mut Value, project_id: Option<&str>) {
    if let Some(project_id) = project_id {
        params["projectId"] = json!(project_id);
    }
}

/// `project/list`, one page. Experimental; absent before Codex 0.149.
/// `sort` is `(sortKey, sortDirection)` — `("recencyAt", "desc")` orders by
/// the newest thread; releases without the keys ignore them (the params are
/// not `deny_unknown_fields`), so it is safe to always send.
pub fn project_list(cursor: Option<&str>, sort: Option<(&str, &str)>) -> Request {
    let mut params = json!({"limit": 100});
    if let Some(cursor) = cursor {
        params["cursor"] = json!(cursor);
    }
    if let Some((key, direction)) = sort {
        params["sortKey"] = json!(key);
        params["sortDirection"] = json!(direction);
    }
    request("project/list", params)
}

/// `project/import`: create a project over `roots` and file `threads` under it
/// in one call. `metadata` is the app's own tag so the project can be matched
/// back to its sidebar entry; `idempotency_key` makes a retried import a
/// no-op rather than a duplicate.
pub fn project_import(
    name: &str,
    roots: &[String],
    metadata: Value,
    threads: &[String],
    idempotency_key: &str,
) -> Request {
    request(
        "project/import",
        json!({
            "name": name,
            "roots": roots.iter().map(|path| json!({"path": path})).collect::<Vec<_>>(),
            "metadata": metadata,
            "threads": threads,
            "idempotencyKey": idempotency_key,
        }),
    )
}

/// `project/update`: only the given fields change.
pub fn project_update(project_id: &str, name: Option<&str>, roots: Option<&[String]>) -> Request {
    let mut params = json!({"projectId": project_id});
    if let Some(name) = name {
        params["name"] = json!(name);
    }
    if let Some(roots) = roots {
        params["roots"] = json!(roots
            .iter()
            .map(|path| json!({"path": path}))
            .collect::<Vec<_>>());
    }
    request("project/update", params)
}

pub fn project_delete(project_id: &str) -> Request {
    request("project/delete", json!({"projectId": project_id}))
}

/// `thread/metadata/update` restricted to the project assignment. `None`
/// files the thread under no project — the protocol spells that as an empty
/// string, since omitting the field means "leave it alone".
pub fn thread_set_project(thread_id: &str, project_id: Option<&str>) -> Request {
    request(
        "thread/metadata/update",
        json!({"threadId": thread_id, "projectId": project_id.unwrap_or("")}),
    )
}

/// `threadSection/list`, one page. Stable in Codex 0.149; absent before.
pub fn thread_section_list(cursor: Option<&str>) -> Request {
    let mut params = json!({"limit": 100});
    if let Some(cursor) = cursor {
        params["cursor"] = json!(cursor);
    }
    request("threadSection/list", params)
}

/// `appearance` is `{icon, color}`; only `color` is set from this app.
fn section_appearance(color: Option<&str>) -> Value {
    json!({"icon": null, "color": color})
}

pub fn thread_section_create(name: &str, color: Option<&str>) -> Request {
    request(
        "threadSection/create",
        json!({"name": name, "appearance": section_appearance(color)}),
    )
}

/// `threadSection/update` always carries the name; `appearance` is sent so a
/// cleared colour actually clears (omitting it would keep the old one).
pub fn thread_section_update(section_id: &str, name: &str, color: Option<&str>) -> Request {
    request(
        "threadSection/update",
        json!({"sectionId": section_id, "name": name, "appearance": section_appearance(color)}),
    )
}

pub fn thread_section_delete(section_id: &str) -> Request {
    request("threadSection/delete", json!({"sectionId": section_id}))
}

/// `thread/section/move`: `None` takes the thread out of its section. The
/// thread is appended to the section (no `beforeThreadId`).
pub fn thread_section_move(thread_id: &str, section_id: Option<&str>) -> Request {
    request(
        "thread/section/move",
        json!({"threadId": thread_id, "sectionId": section_id}),
    )
}

pub fn queue_add(thread_id: &str, input: Value, client_user_message_id: &str) -> Request {
    request(
        "thread/queue/add",
        json!({
            "threadId": thread_id,
            "input": input,
            "clientUserMessageId": client_user_message_id,
        }),
    )
}

pub fn queue_list(thread_id: &str, cursor: Option<&str>) -> Request {
    request(
        "thread/queue/list",
        json!({"threadId": thread_id, "cursor": cursor}),
    )
}

pub fn queue_update(thread_id: &str, queued_submission_id: &str, input: Value) -> Request {
    request(
        "thread/queue/update",
        json!({
            "threadId": thread_id,
            "queuedSubmissionId": queued_submission_id,
            "input": input,
        }),
    )
}

pub fn queue_delete(thread_id: &str, queued_submission_id: &str) -> Request {
    request(
        "thread/queue/delete",
        json!({"threadId": thread_id, "queuedSubmissionId": queued_submission_id}),
    )
}

pub fn queue_reorder(thread_id: &str, queued_submission_ids: &[String]) -> Request {
    request(
        "thread/queue/reorder",
        json!({"threadId": thread_id, "queuedSubmissionIds": queued_submission_ids}),
    )
}

pub fn queue_start(thread_id: &str, queued_submission_id: Option<&str>) -> Request {
    request(
        "thread/queue/start",
        json!({"threadId": thread_id, "queuedSubmissionId": queued_submission_id}),
    )
}
