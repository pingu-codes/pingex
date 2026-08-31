//! Claude stream-json frames into neutral events. One translator per process,
//! since block indices and message ids are per session.
//!
//! Shapes follow `docs/research/claude-code-stdio.md`. Everything is read
//! field by field: a frame we do not know is ignored, never an error.

use serde_json::Value;
use std::collections::{HashMap, HashSet};

use super::tools;
use crate::harness::{
    HarnessEvent, PlanEntry, StopReason, ToolCallContent, ToolCallStatus, TurnUsage,
};
use crate::util::json::{arr_or_empty, str_at};

#[derive(Clone)]
enum BlockKind {
    Text,
    Thinking,
    ToolUse { id: String, name: String },
}

#[derive(Clone)]
struct Block {
    item_id: String,
    kind: BlockKind,
    json: String,
}

#[derive(Default)]
pub(crate) struct Translator {
    pub(crate) cwd: String,
    /// The turn the driver opened by sending a prompt; `None` between turns.
    pub(crate) turn_id: Option<String>,
    message_seq: u64,
    blocks: HashMap<u64, Block>,
    /// Message ids whose blocks arrived as stream events, so the matching
    /// `assistant` frame is not rendered a second time.
    streamed: HashSet<String>,
    current_message: Option<String>,
    /// The `CLAUDE_CONFIG_DIR` the process runs under, for login errors.
    config_dir: String,
    /// Tool calls announced and not yet resolved: name and input by id.
    open_tools: HashMap<String, (String, Value)>,
}

impl Translator {
    pub(crate) fn new(cwd: String, config_dir: String) -> Self {
        Self {
            cwd,
            config_dir,
            ..Default::default()
        }
    }

    fn next_item_id(&mut self, suffix: &str) -> String {
        let turn = self.turn_id.clone().unwrap_or_else(|| "turn".into());
        format!("{turn}-m{}-{suffix}", self.message_seq)
    }

    /// Translate one stdout frame. Control requests never come here; the
    /// driver answers those itself.
    pub(crate) fn frame(&mut self, frame: &Value) -> Vec<HarnessEvent> {
        match str_at(frame, "type") {
            Some("stream_event") => self.stream_event(frame.get("event").unwrap_or(&Value::Null)),
            Some("assistant") => self.assistant(frame),
            Some("user") => self.user(frame),
            Some("result") => self.result(frame),
            Some("system") => self.system(frame),
            _ => Vec::new(),
        }
    }

    fn system(&mut self, frame: &Value) -> Vec<HarnessEvent> {
        match str_at(frame, "subtype") {
            // The CLI credential in use. An environment key would bill (and
            // authenticate) differently than the login, so it is worth a
            // visible warning; the spawn strips the usual variables, but a
            // helper or settings file can still inject one.
            Some("init") => match str_at(frame, "apiKeySource") {
                Some(source @ ("ANTHROPIC_API_KEY" | "apiKeyHelper")) => {
                    vec![HarnessEvent::Notice {
                        level: "warning".into(),
                        text: format!("Claude is authenticating with {source}, not your login"),
                    }]
                }
                _ => Vec::new(),
            },
            Some("compact_boundary") => vec![HarnessEvent::Compaction {
                item_id: format!("compact-{}", str_at(frame, "uuid").unwrap_or("boundary")),
                trigger: frame
                    .get("compact_metadata")
                    .and_then(|meta| str_at(meta, "trigger"))
                    .unwrap_or("auto")
                    .to_string(),
            }],
            Some("api_retry") => vec![HarnessEvent::Notice {
                level: "warning".into(),
                text: format!(
                    "API error ({}), retrying (attempt {} of {})",
                    str_at(frame, "error").unwrap_or("unknown"),
                    frame.get("attempt").and_then(Value::as_u64).unwrap_or(0),
                    frame
                        .get("max_retries")
                        .and_then(Value::as_u64)
                        .unwrap_or(0),
                ),
            }],
            Some("permission_denied") => vec![HarnessEvent::Notice {
                level: "warning".into(),
                text: format!(
                    "{} was denied: {}",
                    str_at(frame, "tool_name").unwrap_or("A tool"),
                    str_at(frame, "message").unwrap_or("")
                ),
            }],
            Some("informational") if str_at(frame, "level") == Some("warning") => {
                vec![HarnessEvent::Notice {
                    level: "warning".into(),
                    text: str_at(frame, "content").unwrap_or("").to_string(),
                }]
            }
            _ => Vec::new(),
        }
    }

    fn stream_event(&mut self, event: &Value) -> Vec<HarnessEvent> {
        // Subagent streams carry their own parent id; only the main thread
        // renders here.
        match str_at(event, "type") {
            Some("message_start") => {
                self.message_seq += 1;
                self.blocks.clear();
                if let Some(id) = event.get("message").and_then(|m| str_at(m, "id")) {
                    self.streamed.insert(id.to_string());
                    self.current_message = Some(id.to_string());
                }
                Vec::new()
            }
            Some("content_block_start") => {
                let index = event.get("index").and_then(Value::as_u64).unwrap_or(0);
                let block = event.get("content_block").unwrap_or(&Value::Null);
                match str_at(block, "type") {
                    Some("text") => {
                        let item_id = self.next_item_id(&format!("b{index}"));
                        self.blocks.insert(
                            index,
                            Block {
                                item_id: item_id.clone(),
                                kind: BlockKind::Text,
                                json: String::new(),
                            },
                        );
                        vec![HarnessEvent::AgentMessageChunk {
                            item_id,
                            text: str_at(block, "text").unwrap_or("").to_string(),
                            done: false,
                        }]
                    }
                    Some("thinking") => {
                        let item_id = self.next_item_id(&format!("b{index}"));
                        self.blocks.insert(
                            index,
                            Block {
                                item_id: item_id.clone(),
                                kind: BlockKind::Thinking,
                                json: String::new(),
                            },
                        );
                        vec![HarnessEvent::AgentThoughtChunk {
                            item_id,
                            text: str_at(block, "thinking").unwrap_or("").to_string(),
                            done: false,
                        }]
                    }
                    Some("tool_use") => {
                        let id = str_at(block, "id").unwrap_or("tool").to_string();
                        let name = str_at(block, "name").unwrap_or("tool").to_string();
                        self.blocks.insert(
                            index,
                            Block {
                                item_id: id.clone(),
                                kind: BlockKind::ToolUse { id, name },
                                json: String::new(),
                            },
                        );
                        Vec::new()
                    }
                    _ => Vec::new(),
                }
            }
            Some("content_block_delta") => {
                let index = event.get("index").and_then(Value::as_u64).unwrap_or(0);
                let Some(block) = self.blocks.get_mut(&index) else {
                    return Vec::new();
                };
                let delta = event.get("delta").unwrap_or(&Value::Null);
                match (str_at(delta, "type"), &block.kind) {
                    (Some("text_delta"), BlockKind::Text) => {
                        vec![HarnessEvent::AgentMessageChunk {
                            item_id: block.item_id.clone(),
                            text: str_at(delta, "text").unwrap_or("").to_string(),
                            done: false,
                        }]
                    }
                    (Some("thinking_delta"), BlockKind::Thinking) => {
                        vec![HarnessEvent::AgentThoughtChunk {
                            item_id: block.item_id.clone(),
                            text: str_at(delta, "thinking").unwrap_or("").to_string(),
                            done: false,
                        }]
                    }
                    (Some("input_json_delta"), BlockKind::ToolUse { .. }) => {
                        block
                            .json
                            .push_str(str_at(delta, "partial_json").unwrap_or(""));
                        Vec::new()
                    }
                    _ => Vec::new(),
                }
            }
            Some("content_block_stop") => {
                let index = event.get("index").and_then(Value::as_u64).unwrap_or(0);
                let Some(block) = self.blocks.remove(&index) else {
                    return Vec::new();
                };
                match block.kind {
                    BlockKind::Text => vec![HarnessEvent::AgentMessageChunk {
                        item_id: block.item_id,
                        text: String::new(),
                        done: true,
                    }],
                    BlockKind::Thinking => vec![HarnessEvent::AgentThoughtChunk {
                        item_id: block.item_id,
                        text: String::new(),
                        done: true,
                    }],
                    BlockKind::ToolUse { id, name } => {
                        let input = if block.json.trim().is_empty() {
                            Value::Object(Default::default())
                        } else {
                            serde_json::from_str(&block.json).unwrap_or(Value::Null)
                        };
                        self.tool_use(&id, &name, &input)
                    }
                }
            }
            _ => Vec::new(),
        }
    }

    /// A tool call the model decided on. Plan tools drive the plan instead
    /// of a card; everything else is announced as a running tool call.
    fn tool_use(&mut self, id: &str, name: &str, input: &Value) -> Vec<HarnessEvent> {
        self.open_tools
            .insert(id.to_string(), (name.to_string(), input.clone()));
        if name == "TodoWrite" {
            return vec![HarnessEvent::Plan {
                entries: arr_or_empty(input, "todos")
                    .iter()
                    .map(|todo| PlanEntry {
                        content: str_at(todo, "content").unwrap_or("").to_string(),
                        priority: "medium".into(),
                        status: str_at(todo, "status").unwrap_or("pending").to_string(),
                    })
                    .collect(),
            }];
        }
        if tools::drives_plan(name) || name == "AskUserQuestion" {
            // Task* calls and questions have no transcript card of their own;
            // the question is answered through a request, not an item.
            return Vec::new();
        }
        vec![tools::tool_call(
            id,
            name,
            input,
            &self.cwd,
            ToolCallStatus::InProgress,
        )]
    }

    /// A completed `assistant` frame. Blocks that streamed are already in
    /// the transcript; this only renders blocks the stream never announced
    /// (a CLI without partial messages).
    fn assistant(&mut self, frame: &Value) -> Vec<HarnessEvent> {
        let message = frame.get("message").unwrap_or(&Value::Null);
        if frame
            .get("parent_tool_use_id")
            .is_some_and(|id| !id.is_null())
        {
            return Vec::new();
        }
        if let Some(id) = str_at(message, "id") {
            if self.streamed.contains(id) {
                return Vec::new();
            }
        }
        self.message_seq += 1;
        let mut out = Vec::new();
        for (index, block) in arr_or_empty(message, "content").iter().enumerate() {
            match str_at(block, "type") {
                Some("text") => {
                    let item_id = self.next_item_id(&format!("b{index}"));
                    out.push(HarnessEvent::AgentMessageChunk {
                        item_id,
                        text: str_at(block, "text").unwrap_or("").to_string(),
                        done: true,
                    });
                }
                Some("thinking") => {
                    let item_id = self.next_item_id(&format!("b{index}"));
                    out.push(HarnessEvent::AgentThoughtChunk {
                        item_id,
                        text: str_at(block, "thinking").unwrap_or("").to_string(),
                        done: true,
                    });
                }
                Some("tool_use") => {
                    let id = str_at(block, "id").unwrap_or("tool").to_string();
                    let name = str_at(block, "name").unwrap_or("tool").to_string();
                    let input = block.get("input").cloned().unwrap_or(Value::Null);
                    out.extend(self.tool_use(&id, &name, &input));
                }
                _ => {}
            }
        }
        out
    }

    /// A CLI-emitted `user` frame: a tool result (or a replay, which the
    /// driver never asks for).
    fn user(&mut self, frame: &Value) -> Vec<HarnessEvent> {
        if frame.get("isReplay").and_then(Value::as_bool) == Some(true) {
            return Vec::new();
        }
        if frame
            .get("parent_tool_use_id")
            .is_some_and(|id| !id.is_null())
        {
            return Vec::new();
        }
        let message = frame.get("message").unwrap_or(&Value::Null);
        let structured = frame.get("tool_use_result");
        let mut out = Vec::new();
        for block in arr_or_empty(message, "content") {
            if str_at(block, "type") != Some("tool_result") {
                continue;
            }
            let Some(tool_use_id) = str_at(block, "tool_use_id") else {
                continue;
            };
            let Some((name, _input)) = self.open_tools.remove(tool_use_id) else {
                continue;
            };
            if tools::drives_plan(&name) || name == "AskUserQuestion" {
                continue;
            }
            let failed = block.get("is_error").and_then(Value::as_bool) == Some(true);
            let status = if failed {
                ToolCallStatus::Failed
            } else {
                ToolCallStatus::Completed
            };
            let text = tools::result_text(block);
            let update = match name.as_str() {
                "Bash" => {
                    let output = structured
                        .filter(|value| value.get("stdout").is_some())
                        .map(|value| {
                            let stdout = str_at(value, "stdout").unwrap_or("");
                            let stderr = str_at(value, "stderr").unwrap_or("");
                            if stderr.is_empty() {
                                stdout.to_string()
                            } else if stdout.is_empty() {
                                stderr.to_string()
                            } else {
                                format!("{stdout}\n{stderr}")
                            }
                        })
                        .unwrap_or(text);
                    HarnessEvent::ToolCallUpdate {
                        item_id: tool_use_id.to_string(),
                        status: Some(status),
                        content: None,
                        output_delta: Some(output),
                    }
                }
                "Edit" | "MultiEdit" | "Write" | "NotebookEdit" => HarnessEvent::ToolCallUpdate {
                    item_id: tool_use_id.to_string(),
                    status: Some(status),
                    content: None,
                    output_delta: None,
                },
                _ => HarnessEvent::ToolCallUpdate {
                    item_id: tool_use_id.to_string(),
                    status: Some(status),
                    content: if text.is_empty() {
                        None
                    } else {
                        Some(vec![ToolCallContent::Content { text }])
                    },
                    output_delta: None,
                },
            };
            out.push(update);
        }
        out
    }

    fn result(&mut self, frame: &Value) -> Vec<HarnessEvent> {
        let Some(turn_id) = self.turn_id.take() else {
            return Vec::new();
        };
        let subtype = str_at(frame, "subtype").unwrap_or("success");
        let terminal = str_at(frame, "terminal_reason").unwrap_or("");
        let is_error = frame.get("is_error").and_then(Value::as_bool) == Some(true);
        let stop_reason = if matches!(terminal, "aborted_streaming" | "aborted_tools") {
            StopReason::Cancelled
        } else if subtype == "error_max_turns" {
            StopReason::MaxTurnRequests
        } else if subtype != "success" || is_error {
            StopReason::Error
        } else if str_at(frame, "stop_reason") == Some("max_tokens") {
            StopReason::MaxTokens
        } else {
            StopReason::EndTurn
        };
        let error = match stop_reason {
            StopReason::Error => {
                let errors: Vec<&str> = arr_or_empty(frame, "errors")
                    .iter()
                    .filter_map(Value::as_str)
                    .collect();
                let text = if errors.is_empty() {
                    str_at(frame, "result")
                        .filter(|text| !text.is_empty())
                        .unwrap_or("Claude reported an error")
                        .to_string()
                } else {
                    errors.join("; ")
                };
                let lower = text.to_lowercase();
                Some(
                    if lower.contains("/login") || lower.contains("not logged in") {
                        format!(
                            "{text}\nClaude has no login under {}. Point the app at your \
                             logged-in Claude config directory in Settings, or run \
                             `claude /login` with CLAUDE_CONFIG_DIR set to it.",
                            self.config_dir
                        )
                    } else {
                        text
                    },
                )
            }
            _ => None,
        };
        let usage = frame.get("usage").map(|usage| {
            let read = |key: &str| usage.get(key).and_then(Value::as_u64).unwrap_or(0);
            let context_window = frame
                .get("modelUsage")
                .and_then(Value::as_object)
                .and_then(|models| models.values().next())
                .and_then(|model| model.get("contextWindow"))
                .and_then(Value::as_u64);
            TurnUsage {
                input_tokens: read("input_tokens")
                    + read("cache_read_input_tokens")
                    + read("cache_creation_input_tokens"),
                cached_input_tokens: read("cache_read_input_tokens"),
                output_tokens: read("output_tokens"),
                context_window,
                cost_usd: frame.get("total_cost_usd").and_then(Value::as_f64),
            }
        });
        // Anything still open did not get a result: the turn was cut short.
        let mut out: Vec<HarnessEvent> = self
            .open_tools
            .drain()
            .map(|(id, _)| HarnessEvent::ToolCallUpdate {
                item_id: id,
                status: Some(ToolCallStatus::Cancelled),
                content: None,
                output_delta: None,
            })
            .collect();
        self.blocks.clear();
        out.push(HarnessEvent::TurnEnded {
            turn_id,
            stop_reason,
            error,
            duration_ms: frame.get("duration_ms").and_then(Value::as_u64),
            usage,
        });
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn run(frames: Vec<Value>) -> Vec<HarnessEvent> {
        let mut translator = Translator::new("/repo".into(), "/cfg".into());
        translator.turn_id = Some("turn-1".into());
        frames
            .iter()
            .flat_map(|frame| translator.frame(frame))
            .collect()
    }

    fn kinds(events: &[HarnessEvent]) -> Vec<String> {
        events
            .iter()
            .map(|event| match event {
                HarnessEvent::AgentMessageChunk { done, .. } => {
                    format!("msg{}", if *done { "!" } else { "" })
                }
                HarnessEvent::AgentThoughtChunk { done, .. } => {
                    format!("thought{}", if *done { "!" } else { "" })
                }
                HarnessEvent::ToolCall { name, .. } => format!("call:{name}"),
                HarnessEvent::ToolCallUpdate { status, .. } => format!("update:{:?}", status),
                HarnessEvent::Plan { .. } => "plan".into(),
                HarnessEvent::TurnEnded { stop_reason, .. } => format!("end:{:?}", stop_reason),
                other => format!("{other:?}"),
            })
            .collect()
    }

    #[test]
    fn a_streamed_reply_with_a_bash_call_reads_in_order() {
        let events = run(vec![
            json!({"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_1"}}}),
            json!({"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"thinking"}}}),
            json!({"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"hm"}}}),
            json!({"type":"stream_event","event":{"type":"content_block_stop","index":0}}),
            json!({"type":"stream_event","event":{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"toolu_1","name":"Bash"}}}),
            json!({"type":"stream_event","event":{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"command\":\"ls\"}"}}}),
            json!({"type":"stream_event","event":{"type":"content_block_stop","index":1}}),
            json!({"type":"assistant","message":{"id":"msg_1","content":[{"type":"tool_use","id":"toolu_1","name":"Bash","input":{"command":"ls"}}]}}),
            json!({"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"a.txt"}]},"tool_use_result":{"stdout":"a.txt\n","stderr":""}}),
            json!({"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_2"}}}),
            json!({"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}),
            json!({"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Done."}}}),
            json!({"type":"stream_event","event":{"type":"content_block_stop","index":0}}),
            json!({"type":"result","subtype":"success","is_error":false,"duration_ms":10,"usage":{"input_tokens":5,"output_tokens":2}}),
        ]);
        assert_eq!(
            kinds(&events),
            vec![
                "thought",
                "thought",
                "thought!",
                "call:Bash",
                "update:Some(Completed)",
                "msg",
                "msg",
                "msg!",
                "end:EndTurn"
            ]
        );
        let HarnessEvent::ToolCallUpdate { output_delta, .. } = &events[4] else {
            panic!()
        };
        assert_eq!(output_delta.as_deref(), Some("a.txt\n"));
    }

    #[test]
    fn todo_write_drives_the_plan_and_shows_no_card() {
        let events = run(vec![
            json!({"type":"assistant","message":{"id":"m","content":[{"type":"tool_use","id":"t","name":"TodoWrite","input":{"todos":[{"content":"a","status":"in_progress"}]}}]}}),
            json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t","content":"Todos have been modified successfully"}]}}),
        ]);
        assert_eq!(kinds(&events), vec!["plan"]);
    }

    #[test]
    fn an_env_api_key_in_init_raises_a_warning() {
        let events = run(vec![json!({
            "type":"system","subtype":"init","session_id":"s","model":"claude-haiku-4-5",
            "apiKeySource":"ANTHROPIC_API_KEY"
        })]);
        let [HarnessEvent::Notice { level, text }] = &events[..] else {
            panic!("expected one notice: {events:?}")
        };
        assert_eq!(level, "warning");
        assert!(text.contains("ANTHROPIC_API_KEY"), "{text}");
        // A normal login is silent.
        assert!(run(vec![json!({
            "type":"system","subtype":"init","apiKeySource":"none"
        })])
        .is_empty());
    }

    #[test]
    fn a_login_error_names_the_config_dir() {
        let events = run(vec![json!({
            "type":"result","subtype":"error_during_execution","is_error":true,
            "result":"Not logged in - please run /login"
        })]);
        let HarnessEvent::TurnEnded {
            error: Some(error), ..
        } = &events[0]
        else {
            panic!("expected an error end: {events:?}")
        };
        assert!(error.contains("/cfg"), "{error}");
        assert!(error.contains("Settings"), "{error}");
    }

    #[test]
    fn an_aborted_result_cancels_open_calls() {
        let events = run(vec![
            json!({"type":"assistant","message":{"id":"m","content":[{"type":"tool_use","id":"t","name":"Bash","input":{"command":"sleep 100"}}]}}),
            json!({"type":"result","subtype":"success","is_error":false,"terminal_reason":"aborted_streaming"}),
        ]);
        assert_eq!(
            kinds(&events),
            vec!["call:Bash", "update:Some(Cancelled)", "end:Cancelled"]
        );
    }

    #[test]
    fn a_failed_result_carries_its_message() {
        let events = run(vec![
            json!({"type":"result","subtype":"error_during_execution","is_error":true,"errors":["boom"]}),
        ]);
        let HarnessEvent::TurnEnded { error, .. } = &events[0] else {
            panic!()
        };
        assert_eq!(error.as_deref(), Some("boom"));
    }
}

#[cfg(test)]
mod fixtures {
    //! Golden streams recorded from a real `claude` (`tests/fixtures/protocol/claude/`).
    use super::*;

    fn events_for(wire: &str) -> Vec<HarnessEvent> {
        let mut translator = Translator::new("/tmp/pingex-fixture".into(), "/cfg".into());
        translator.turn_id = Some("turn-1".into());
        wire.lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str::<Value>(line).expect("fixture line is JSON"))
            .flat_map(|frame| translator.frame(&frame))
            .collect()
    }

    #[test]
    fn bash_echo_recorded_from_claude_2_1_251() {
        let events = events_for(include_str!(
            "../../../tests/fixtures/protocol/claude/bash-echo/wire.ndjson"
        ));
        // One thought, the Bash call, its completion with the echoed output,
        // a second thought, the reply, and the turn ending normally.
        let call = events
            .iter()
            .find(|e| matches!(e, HarnessEvent::ToolCall { name, .. } if name == "Bash"))
            .expect("Bash call announced");
        let HarnessEvent::ToolCall { title, kind, .. } = call else {
            unreachable!()
        };
        assert_eq!(title, "echo pingex-live-ok");
        assert_eq!(*kind, crate::harness::ToolKind::Execute);
        let update = events
            .iter()
            .find(|e| {
                matches!(
                    e,
                    HarnessEvent::ToolCallUpdate {
                        status: Some(ToolCallStatus::Completed),
                        ..
                    }
                )
            })
            .expect("Bash completed");
        let HarnessEvent::ToolCallUpdate { output_delta, .. } = update else {
            unreachable!()
        };
        assert_eq!(output_delta.as_deref(), Some("pingex-live-ok"));
        let reply: String = events
            .iter()
            .filter_map(|e| match e {
                HarnessEvent::AgentMessageChunk { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(reply.trim(), "done");
        assert!(events
            .iter()
            .any(|e| matches!(e, HarnessEvent::AgentThoughtChunk { done: true, .. })));
        assert!(matches!(
            events.last(),
            Some(HarnessEvent::TurnEnded {
                stop_reason: StopReason::EndTurn,
                error: None,
                usage: Some(_),
                ..
            })
        ));
        // Streamed blocks are not rendered twice off the `assistant` frames.
        let messages = events
            .iter()
            .filter(|e| matches!(e, HarnessEvent::AgentMessageChunk { done: true, .. }))
            .count();
        assert_eq!(messages, 1);
    }
}

#[cfg(test)]
mod permission_fixtures {
    use super::*;
    use crate::harness::HarnessRequest;

    #[test]
    fn write_approval_recorded_from_claude_2_1_251() {
        let wire =
            include_str!("../../../tests/fixtures/protocol/claude/write-approval/wire.ndjson");
        let request = wire
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .find(|frame| str_at(frame, "type") == Some("control_request"))
            .expect("a control request in the recording");
        let can_use_tool = request.get("request").expect("request body");
        assert_eq!(str_at(can_use_tool, "subtype"), Some("can_use_tool"));
        let mapped = super::super::permissions::request_for(can_use_tool, "/tmp/pingex-fixture");
        let HarnessRequest::Permission {
            kind,
            options,
            changes,
            command,
            description,
            ..
        } = mapped
        else {
            panic!("a Write is a permission, not a question");
        };
        assert_eq!(kind, crate::harness::ToolKind::Edit);
        assert!(command.is_none());
        assert_eq!(description.as_deref(), Some("hello.txt"));
        let change = &changes.0[0];
        assert!(str_at(change, "path").unwrap().ends_with("hello.txt"));
        assert_eq!(change["kind"]["type"], "add");
        assert!(str_at(change, "diff").unwrap().contains("+hi"));
        // The CLI suggested switching to acceptEdits, so "always" is offered
        // and answers with that suggestion.
        assert_eq!(
            options
                .iter()
                .map(|o| o.option_id.as_str())
                .collect::<Vec<_>>(),
            vec!["allow", "allow_always", "reject"]
        );
        assert_eq!(options[1].name, "Switch to acceptEdits");
        let answer = super::super::permissions::permission_result("allow_always", can_use_tool);
        assert_eq!(answer["updatedPermissions"][0]["mode"], "acceptEdits");
        assert_eq!(answer["toolUseID"], can_use_tool["tool_use_id"]);
    }
}
