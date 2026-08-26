//! The tool surface the parent agent sees.
//!
//! These are declared to the app-server as `thread/start.dynamicTools`, so a
//! call arrives as an `item/tool/call` server request and we answer it with
//! `{contentItems, success}`.
//!
//! Every name carries a `pingex_` prefix. Codex ships its own `spawn_agent`,
//! and a same-named dynamic tool is silently shadowed by it — the model calls
//! the built-in, we never see an `item/tool/call`, and the failure looks like
//! "dynamic tools don't work" rather than "you picked a taken name".

use serde::Deserialize;
use serde_json::{json, Value};
use std::path::{Component, Path, PathBuf};

pub(crate) const SPAWN: &str = "pingex_spawn_agent";
pub(crate) const WAIT: &str = "pingex_wait_agents";
pub(crate) const SEND_INPUT: &str = "pingex_send_input";
pub(crate) const KILL: &str = "pingex_kill_agent";

/// Every tool this module answers.
pub(crate) const TOOL_NAMES: [&str; 4] = [SPAWN, WAIT, SEND_INPUT, KILL];

/// Upper bounds on what one call may carry. The prompt cap is generous enough
/// for a full briefing but stops a runaway parent from writing a novel into
/// the database on every call.
const MAX_PROMPT_BYTES: usize = 32 * 1024;
const MAX_NAME_CHARS: usize = 60;
const MAX_FILES: usize = 20;
const MAX_ATTACHED_BYTES: usize = 32 * 1024;
/// Ceiling on `wait_agents` so a parent turn can never block indefinitely.
pub(crate) const MAX_WAIT_SECONDS: u64 = 1800;
const DEFAULT_WAIT_SECONDS: u64 = 300;
/// How much of an agent's final message is handed back to the parent.
const MAX_RESULT_BYTES: usize = 24 * 1024;

/// Efforts the tool may ask for. Anything else is dropped rather than guessed
/// at, matching how `apply_turn_options` treats an unknown sandbox.
const EFFORTS: [&str; 4] = ["low", "medium", "high", "xhigh"];

/// Does this `item/tool/call` belong to us? Anything else falls through to the
/// frontend untouched, so a future Codex-side dynamic tool still works.
pub(crate) fn owns(params: &Value) -> bool {
    params
        .get("tool")
        .and_then(Value::as_str)
        .is_some_and(|tool| TOOL_NAMES.contains(&tool))
}

/// The array passed as `thread/start.dynamicTools`.
///
/// `model_ids` are the slugs this account can actually run. When they are known
/// the `model` field becomes an enum, which is the only reliable way to stop a
/// prompt like "use the luna subagents" turning into `model: "luna"` — a value
/// `turn/start` accepts and the provider then rejects mid-stream, killing the
/// agent. An empty list leaves the field free-form rather than blocking every
/// model, since not knowing the list is not the same as there being none.
pub fn specs(model_ids: &[String]) -> Value {
    let mut model_property = json!({
        "type": "string",
        "description": "An exact model slug. Omit unless you know it: a nickname, \
    an agent name, or a description is not a model.",
    });
    if !model_ids.is_empty() {
        model_property["enum"] = json!(model_ids);
        // Users name models loosely ("the luna agents"), and the matching slug
        // is usually a longer form of that word — so say to map it rather than
        // to ignore it.
        model_property["description"] = json!(
            "Which model the agent runs on. Must be exactly one of the listed slugs. If the \
user names a model informally, pick the slug that matches it; if none does, omit this and \
the agent uses the default."
        );
    }
    let mut specs = json!([
        {
            "type": "function",
            "name": SPAWN,
            "description": "Spawn a background agent in its own separate Codex process to work \
    on a task independently. Returns immediately with an agentId so several agents can run at once — \
    spawn all of them first, then call pingex_wait_agents once with every id. The agent starts with a \
    completely fresh context and can see nothing from this conversation, so `prompt` must be \
    self-contained.",
            "deferLoading": false,
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Short label shown to the user, e.g. 'auth audit'."
                    },
                    "prompt": {
                        "type": "string",
                        "description": "The complete, self-contained task for the agent."
                    },
                    "cwd": {
                        "type": "string",
                        "description": "Working directory, absolute or relative to the parent's \
    working directory. Must stay inside it. Defaults to the parent's."
                    },
                    "model": null,
                    "effort": {"type": "string", "enum": EFFORTS, "description": "Reasoning effort."},
                    "sandbox": {
                        "type": "string",
                        "enum": ["read-only", "workspace-write"],
                        "description": "Narrower than the user's configured ceiling only; a wider \
    request is clamped down to it."
                    },
                    "files": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Up to 20 files, relative to cwd, appended to the prompt."
                    }
                },
                "required": ["name", "prompt"],
                "additionalProperties": false
            }
        },
        {
            "type": "function",
            "name": WAIT,
            "description": "Wait for agents spawned with pingex_spawn_agent and collect their \
    results. Pass every id you are waiting on in one call. If it returns with an agent still running, \
    that is a timeout, not a failure — call again with the ids that have not finished.",
            "deferLoading": false,
            "inputSchema": {
                "type": "object",
                "properties": {
                    "agentIds": {"type": "array", "items": {"type": "string"}},
                    "timeoutSeconds": {
                        "type": "number",
                        "description": "How long to wait. Defaults to 300, capped at 1800."
                    }
                },
                "required": ["agentIds"],
                "additionalProperties": false
            }
        },
        {
            "type": "function",
            "name": SEND_INPUT,
            "description": "Send a follow-up message to an agent that has finished its current \
    turn, continuing its existing conversation. Use pingex_wait_agents afterwards to collect the reply.",
            "deferLoading": false,
            "inputSchema": {
                "type": "object",
                "properties": {
                    "agentId": {"type": "string"},
                    "text": {"type": "string"}
                },
                "required": ["agentId", "text"],
                "additionalProperties": false
            }
        },
        {
            "type": "function",
            "name": KILL,
            "description": "Stop an agent and kill its process. Use this for an agent that is \
    stuck or no longer needed; its partial output is kept.",
            "deferLoading": false,
            "inputSchema": {
                "type": "object",
                "properties": {
                    "agentId": {"type": "string"},
                    "reason": {"type": "string"}
                },
                "required": ["agentId"],
                "additionalProperties": false
            }
        }
    ]);
    specs[0]["inputSchema"]["properties"]["model"] = model_property;
    specs
}

/// The delegation policy handed to a thread that has these tools.
///
/// Codex's own `spawn_agent` cannot be turned off — no `multiAgentMode`,
/// `[agents]` or `[features]` setting suppresses it — so the only way to make
/// the app's agents the ones that actually run is to say so in the thread's
/// developer instructions. Verified to hold even for prompts that explicitly
/// ask for a subagent.
pub const DELEGATION_POLICY: &str = "\
# Delegation policy

This session runs inside Pingex, which manages background agents itself.

Codex's built-in subagent tools (spawn_agent, wait_agent, send_input, \
close_agent) must NOT be used here: work they do is invisible to the user and \
is not tracked by the app.

To delegate, use the Pingex tools instead:
- `pingex_spawn_agent` to start an agent (returns immediately)
- `pingex_wait_agents` to collect results
- `pingex_send_input` to follow up with one
- `pingex_kill_agent` to stop one that is stuck

Whenever you would otherwise spawn a subagent, delegate, parallelise or fan out \
work, use `pingex_spawn_agent`. Spawn every agent you need first, then wait for \
them in a single `pingex_wait_agents` call, so they run concurrently.

Each agent is a separate process with an empty context: it cannot see this \
conversation, so give it everything it needs in its prompt.";

/// A validated `pingex_spawn_agent` call.
#[derive(Debug, PartialEq)]
pub(crate) struct SpawnArgs {
    pub(crate) name: String,
    pub(crate) prompt: String,
    pub(crate) cwd: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) effort: Option<String>,
    pub(crate) sandbox: Option<String>,
    pub(crate) files: Vec<String>,
}

#[derive(Default, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
struct RawSpawnArgs {
    name: String,
    prompt: String,
    cwd: Option<String>,
    model: Option<String>,
    effort: Option<String>,
    sandbox: Option<String>,
    files: Vec<String>,
}

fn clean(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub(crate) fn parse_spawn_args(arguments: &Value) -> Result<SpawnArgs, String> {
    let raw: RawSpawnArgs = serde_json::from_value(arguments.clone())
        .map_err(|error| format!("Could not read the arguments: {error}"))?;

    let prompt = raw.prompt.trim().to_string();
    if prompt.is_empty() {
        return Err("`prompt` is required and must not be empty.".into());
    }
    if prompt.len() > MAX_PROMPT_BYTES {
        return Err(format!(
            "`prompt` is {} bytes; the limit is {MAX_PROMPT_BYTES}.",
            prompt.len()
        ));
    }
    let name = raw.name.trim();
    let name = if name.is_empty() { "agent" } else { name };
    let name: String = name.chars().take(MAX_NAME_CHARS).collect();

    if raw.files.len() > MAX_FILES {
        return Err(format!(
            "`files` has {} entries; the limit is {MAX_FILES}.",
            raw.files.len()
        ));
    }
    let effort = clean(raw.effort).filter(|value| EFFORTS.contains(&value.as_str()));

    Ok(SpawnArgs {
        name,
        prompt,
        cwd: clean(raw.cwd),
        model: clean(raw.model),
        effort,
        sandbox: clean(raw.sandbox),
        files: raw
            .files
            .into_iter()
            .filter_map(|file| clean(Some(file)))
            .collect(),
    })
}

/// Resolve the working directory a spawn asked for, refusing anything outside
/// the parent's own. The model picks this string, so it is untrusted: `..`
/// segments are rejected outright rather than normalised away, and an absolute
/// path is only allowed if it is already inside the parent directory.
pub(crate) fn resolve_cwd(parent_cwd: &Path, requested: Option<&str>) -> Result<PathBuf, String> {
    let Some(requested) = requested else {
        return Ok(parent_cwd.to_path_buf());
    };
    let candidate = Path::new(requested);
    let joined = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        parent_cwd.join(candidate)
    };
    if joined
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(format!(
            "`cwd` must stay inside {}: {requested}",
            parent_cwd.display()
        ));
    }
    if !joined.starts_with(parent_cwd) {
        return Err(format!(
            "`cwd` must stay inside {}: {requested}",
            parent_cwd.display()
        ));
    }
    Ok(joined)
}

/// Narrow the requested sandbox to the user's configured ceiling. A tool call
/// can ask for less than the ceiling but never more, and `danger-full-access`
/// is unreachable either way — nobody is watching to approve what an agent does.
pub(crate) fn clamp_sandbox(requested: Option<&str>, ceiling: &str) -> String {
    let rank = |value: &str| match value {
        "read-only" => 0,
        "workspace-write" => 1,
        // Anything unrecognised (including danger-full-access) is treated as
        // the widest possible request, so the ceiling always wins.
        _ => 2,
    };
    let ceiling = if rank(ceiling) <= 1 {
        ceiling
    } else {
        crate::settings::prefs::DEFAULT_AGENT_SANDBOX
    };
    match requested {
        Some(requested) if rank(requested) < rank(ceiling) => requested.to_string(),
        _ => ceiling.to_string(),
    }
}

/// Append the requested files to the prompt. A fresh process shares nothing
/// with its parent, and a bounded file list is the one piece of context the
/// parent cannot simply write out in prose.
pub(crate) fn attach_files(prompt: &str, cwd: &Path, files: &[String]) -> String {
    if files.is_empty() {
        return prompt.to_string();
    }
    let mut body = String::new();
    let mut budget = MAX_ATTACHED_BYTES;
    for file in files {
        // Reuse the cwd guard: an attachment path is as untrusted as a cwd.
        let Ok(path) = resolve_cwd(cwd, Some(file)) else {
            continue;
        };
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let content = if content.len() > budget {
            &content[..floor_char_boundary(&content, budget)]
        } else {
            content.as_str()
        };
        if content.is_empty() {
            continue;
        }
        budget = budget.saturating_sub(content.len());
        body.push_str(&format!("\n### {file}\n\n```\n{content}\n```\n"));
        if budget == 0 {
            break;
        }
    }
    if body.is_empty() {
        return prompt.to_string();
    }
    format!("{prompt}\n\n## Attached files\n{body}")
}

fn floor_char_boundary(value: &str, index: usize) -> usize {
    let mut index = index.min(value.len());
    while index > 0 && !value.is_char_boundary(index) {
        index -= 1;
    }
    index
}

/// Keep the tail of an oversized agent result, on a char boundary — the same
/// shape as the journal's command-output trimming, and for the same reason:
/// the conclusion is at the end.
pub(crate) fn trim_result(value: &str) -> String {
    if value.len() <= MAX_RESULT_BYTES {
        return value.to_string();
    }
    let start = floor_char_boundary(value, value.len() - MAX_RESULT_BYTES);
    format!("[earlier output truncated]\n{}", &value[start..])
}

pub(crate) fn wait_timeout_seconds(requested: Option<f64>) -> u64 {
    match requested {
        Some(seconds) if seconds.is_finite() && seconds > 0.0 => {
            (seconds as u64).min(MAX_WAIT_SECONDS)
        }
        _ => DEFAULT_WAIT_SECONDS,
    }
}

/// Build the `DynamicToolCallResponse` the app-server expects. The payload is
/// JSON inside a text content item: the model reads it, so it has to be
/// self-describing, and the server rejects any other shape.
pub(crate) fn render_result(payload: Value, success: bool) -> Value {
    let text = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string());
    json!({
        "contentItems": [{"type": "inputText", "text": text}],
        "success": success,
    })
}

/// A failure the model should read and act on, rather than a protocol error.
pub(crate) fn render_error(message: impl Into<String>) -> Value {
    render_result(json!({"error": message.into()}), false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offers_only_real_model_slugs_when_they_are_known() {
        let specs = specs(&["gpt-5.2".to_string(), "gpt-5.6-terra".to_string()]);
        let model = &specs[0]["inputSchema"]["properties"]["model"];
        // The enum is what stops "use the luna subagents" becoming
        // `model: "luna"` — a value turn/start accepts and inference rejects.
        assert_eq!(model["enum"], json!(["gpt-5.2", "gpt-5.6-terra"]));
        assert_eq!(model["type"], json!("string"));
    }

    #[test]
    fn leaves_the_model_free_form_when_the_list_is_unknown() {
        // Not knowing the models is not the same as there being none, so the
        // field stays usable rather than rejecting every value.
        let specs = specs(&[]);
        let model = &specs[0]["inputSchema"]["properties"]["model"];
        assert!(model.get("enum").is_none());
        assert_eq!(model["type"], json!("string"));
    }

    #[test]
    fn declares_every_tool_with_a_pingex_prefix() {
        let specs = specs(&[]);
        let names: Vec<&str> = specs
            .as_array()
            .unwrap()
            .iter()
            .map(|spec| spec["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, TOOL_NAMES);
        // The prefix is load-bearing: a bare `spawn_agent` is shadowed by
        // Codex's own tool and the call never reaches us.
        assert!(names.iter().all(|name| name.starts_with("pingex_")));
        for spec in specs.as_array().unwrap() {
            assert_eq!(spec["type"], json!("function"));
            assert_eq!(spec["inputSchema"]["type"], json!("object"));
            assert!(spec["description"].as_str().unwrap().len() > 20);
        }
    }

    #[test]
    fn owns_only_our_own_tool_calls() {
        assert!(owns(&json!({"tool": SPAWN})));
        assert!(owns(&json!({"tool": KILL})));
        assert!(!owns(&json!({"tool": "spawn_agent"})));
        assert!(!owns(&json!({"tool": "some_mcp_tool"})));
        assert!(!owns(&json!({})));
    }

    #[test]
    fn parses_a_spawn_call_and_defaults_the_name() {
        let args = parse_spawn_args(&json!({
            "name": "  audit  ", "prompt": " look at auth ", "effort": "high"
        }))
        .unwrap();
        assert_eq!(args.name, "audit");
        assert_eq!(args.prompt, "look at auth");
        assert_eq!(args.effort.as_deref(), Some("high"));

        let unnamed = parse_spawn_args(&json!({"name": "  ", "prompt": "go"})).unwrap();
        assert_eq!(unnamed.name, "agent");
    }

    #[test]
    fn rejects_a_spawn_with_nothing_to_do() {
        assert!(parse_spawn_args(&json!({"name": "a", "prompt": "   "})).is_err());
        assert!(parse_spawn_args(&json!({"name": "a"})).is_err());
    }

    #[test]
    fn rejects_an_oversized_prompt_or_file_list() {
        let long = "x".repeat(MAX_PROMPT_BYTES + 1);
        assert!(parse_spawn_args(&json!({"name": "a", "prompt": long})).is_err());

        let files: Vec<String> = (0..MAX_FILES + 1).map(|i| format!("f{i}")).collect();
        assert!(parse_spawn_args(&json!({"name": "a", "prompt": "go", "files": files})).is_err());
    }

    #[test]
    fn drops_an_effort_it_does_not_recognise() {
        let args =
            parse_spawn_args(&json!({"name": "a", "prompt": "go", "effort": "ultra"})).unwrap();
        assert_eq!(args.effort, None);
    }

    #[test]
    fn a_long_name_is_truncated_rather_than_rejected() {
        let args =
            parse_spawn_args(&json!({"name": "n".repeat(MAX_NAME_CHARS + 40), "prompt": "go"}))
                .unwrap();
        assert_eq!(args.name.chars().count(), MAX_NAME_CHARS);
    }

    #[test]
    fn resolves_a_cwd_inside_the_parent() {
        let parent = Path::new("/repo");
        assert_eq!(resolve_cwd(parent, None).unwrap(), PathBuf::from("/repo"));
        assert_eq!(
            resolve_cwd(parent, Some("crates/core")).unwrap(),
            PathBuf::from("/repo/crates/core")
        );
        assert_eq!(
            resolve_cwd(parent, Some("/repo/crates")).unwrap(),
            PathBuf::from("/repo/crates")
        );
    }

    #[test]
    fn refuses_a_cwd_that_escapes_the_parent() {
        let parent = Path::new("/repo");
        assert!(resolve_cwd(parent, Some("../secrets")).is_err());
        assert!(resolve_cwd(parent, Some("crates/../../etc")).is_err());
        assert!(resolve_cwd(parent, Some("/etc")).is_err());
        // A sibling that merely shares a name prefix is still outside.
        assert!(resolve_cwd(parent, Some("/repo-other")).is_err());
    }

    #[test]
    fn the_sandbox_ceiling_can_be_narrowed_but_never_widened() {
        assert_eq!(clamp_sandbox(None, "workspace-write"), "workspace-write");
        assert_eq!(
            clamp_sandbox(Some("read-only"), "workspace-write"),
            "read-only"
        );
        // Asking for more than the ceiling gets the ceiling.
        assert_eq!(
            clamp_sandbox(Some("workspace-write"), "read-only"),
            "read-only"
        );
        assert_eq!(
            clamp_sandbox(Some("danger-full-access"), "workspace-write"),
            "workspace-write"
        );
    }

    #[test]
    fn full_access_is_unreachable_even_if_it_reaches_the_ceiling() {
        assert_eq!(
            clamp_sandbox(Some("danger-full-access"), "danger-full-access"),
            crate::settings::prefs::DEFAULT_AGENT_SANDBOX
        );
        assert_eq!(
            clamp_sandbox(None, "nonsense"),
            crate::settings::prefs::DEFAULT_AGENT_SANDBOX
        );
    }

    #[test]
    fn attaches_readable_files_and_skips_the_rest() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("a.txt"), "alpha").unwrap();

        let prompt = attach_files(
            "do it",
            directory.path(),
            &["a.txt".into(), "missing.txt".into(), "../escape".into()],
        );
        assert!(prompt.starts_with("do it"));
        assert!(prompt.contains("## Attached files"));
        assert!(prompt.contains("### a.txt"));
        assert!(prompt.contains("alpha"));
        assert!(!prompt.contains("missing.txt"));
        assert!(!prompt.contains("escape"));
    }

    #[test]
    fn a_prompt_with_no_readable_files_is_left_alone() {
        let directory = tempfile::tempdir().unwrap();
        assert_eq!(attach_files("do it", directory.path(), &[]), "do it");
        assert_eq!(
            attach_files("do it", directory.path(), &["nope.txt".into()]),
            "do it"
        );
    }

    #[test]
    fn keeps_the_tail_of_an_oversized_result() {
        let value = format!("{}\nthe conclusion", "x".repeat(MAX_RESULT_BYTES * 2));
        let trimmed = trim_result(&value);
        assert!(trimmed.len() < value.len());
        assert!(trimmed.starts_with("[earlier output truncated]"));
        assert!(trimmed.ends_with("\nthe conclusion"));
        assert_eq!(trim_result("short"), "short");
    }

    #[test]
    fn wait_timeouts_are_bounded_so_a_turn_always_makes_progress() {
        assert_eq!(wait_timeout_seconds(None), DEFAULT_WAIT_SECONDS);
        assert_eq!(wait_timeout_seconds(Some(0.0)), DEFAULT_WAIT_SECONDS);
        assert_eq!(wait_timeout_seconds(Some(-5.0)), DEFAULT_WAIT_SECONDS);
        assert_eq!(
            wait_timeout_seconds(Some(f64::INFINITY)),
            DEFAULT_WAIT_SECONDS
        );
        assert_eq!(wait_timeout_seconds(Some(30.0)), 30);
        assert_eq!(wait_timeout_seconds(Some(99_999.0)), MAX_WAIT_SECONDS);
    }

    #[test]
    fn renders_the_response_shape_the_app_server_validates() {
        let value = render_result(json!({"agentId": "agt_1"}), true);
        assert_eq!(value["success"], json!(true));
        assert_eq!(value["contentItems"][0]["type"], json!("inputText"));
        let text = value["contentItems"][0]["text"].as_str().unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(text).unwrap()["agentId"],
            json!("agt_1")
        );

        let error = render_error("nope");
        assert_eq!(error["success"], json!(false));
        assert!(error["contentItems"][0]["text"]
            .as_str()
            .unwrap()
            .contains("nope"));
    }
}
