//! ACP `session/update` notifications in, [`HarnessEvent`]s out.
//!
//! Content only counts while a prompt is in flight. `session/load` replays a
//! whole conversation as updates before it answers, and the journal already
//! holds that history, so a translator with no turn drops it: the turn gate
//! is the replay suppression.

use serde_json::Value;
use std::collections::HashMap;
use std::time::Instant;

use super::spec::AcpSpec;
use crate::harness::{
    HarnessEvent, PlanEntry, StopReason, ToolCallContent, ToolCallStatus, ToolKind, TurnUsage,
};
use crate::util::json::{str_at, Json};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Run {
    Message,
    Thought,
}

struct Tool {
    item_id: String,
    /// The title the call opened with. ACP agents tend to put the tool's own
    /// name there before they know what it is doing.
    name: String,
    title: Option<String>,
    kind: ToolKind,
    raw_input: Value,
    /// What the agent itself attached: diffs, or text.
    content: Vec<ToolCallContent>,
    announced: bool,
    finished: bool,
    /// The output already streamed, so accumulated output becomes a delta.
    output: String,
}

/// The latest `usage_update`: context used and size, and the cost so far.
#[derive(Clone, Copy, Default)]
struct Usage {
    used: u64,
    size: Option<u64>,
    cost: Option<f64>,
}

pub(crate) struct Translator {
    pub(crate) turn_id: Option<String>,
    cwd: String,
    spec: &'static dyn AcpSpec,
    run: Option<(Run, String)>,
    runs: u32,
    tools: HashMap<String, Tool>,
    started: Option<Instant>,
    model: Option<String>,
    usage: Usage,
    /// A `usage_update` arrived during this turn.
    usage_fresh: bool,
    cost_at_start: f64,
}

fn text_of(block: &Value) -> Option<&str> {
    (str_at(block, "type") == Some("text"))
        .then(|| str_at(block, "text"))
        .flatten()
}

/// Output agents fence as Markdown, unfenced.
fn unfence(text: &str) -> &str {
    let trimmed = text.trim_end();
    if let Some(body) = trimmed
        .strip_prefix("```")
        .and_then(|rest| rest.split_once('\n'))
        .map(|(_, body)| body)
        .and_then(|body| body.strip_suffix("```"))
    {
        return body.strip_suffix('\n').unwrap_or(body);
    }
    text
}

/// ACP tool-call content: text and diffs (terminals are client-hosted, and
/// this client hosts none).
fn content_of(update: &Value) -> Option<(String, Vec<ToolCallContent>)> {
    let entries = update.get("content")?.as_array()?;
    let mut texts = Vec::new();
    let mut diffs = Vec::new();
    for entry in entries {
        match str_at(entry, "type") {
            Some("content") => {
                if let Some(text) = entry.get("content").and_then(text_of) {
                    texts.push(unfence(text).to_string());
                }
            }
            Some("diff") => {
                if let Some(path) = str_at(entry, "path") {
                    diffs.push(ToolCallContent::Diff {
                        path: path.to_string(),
                        old_text: str_at(entry, "oldText").map(str::to_string),
                        new_text: str_at(entry, "newText").unwrap_or_default().to_string(),
                    });
                }
            }
            _ => {}
        }
    }
    Some((texts.join("\n"), diffs))
}

fn parse<T: serde::de::DeserializeOwned>(value: Option<&Value>) -> Option<T> {
    value.and_then(|value| serde_json::from_value(value.clone()).ok())
}

impl Translator {
    pub(crate) fn new(cwd: String, spec: &'static dyn AcpSpec) -> Self {
        Self {
            turn_id: None,
            cwd,
            spec,
            run: None,
            runs: 0,
            tools: HashMap::new(),
            started: None,
            model: None,
            usage: Usage::default(),
            usage_fresh: false,
            cost_at_start: 0.0,
        }
    }

    pub(crate) fn begin_turn(&mut self, turn_id: &str, model: Option<String>) {
        self.turn_id = Some(turn_id.to_string());
        self.model = model;
        self.started = Some(Instant::now());
        self.run = None;
        self.runs = 0;
        self.tools.clear();
        self.usage_fresh = false;
        self.cost_at_start = self.usage.cost.unwrap_or(0.0);
    }

    /// One `session/update`'s `update` object.
    pub(crate) fn update(&mut self, update: &Value) -> Vec<HarnessEvent> {
        let kind = str_at(update, "sessionUpdate").unwrap_or_default();
        if kind == "usage_update" {
            self.usage = Usage {
                used: update.get("used").and_then(Value::as_u64).unwrap_or(0),
                size: update.get("size").and_then(Value::as_u64),
                cost: update
                    .get("cost")
                    .and_then(|cost| cost.get("amount"))
                    .and_then(Value::as_f64),
            };
            self.usage_fresh = self.turn_id.is_some();
            return Vec::new();
        }
        if self.turn_id.is_none() {
            return Vec::new();
        }
        match kind {
            "agent_message_chunk" => self.chunk(Run::Message, update),
            "agent_thought_chunk" => self.chunk(Run::Thought, update),
            "tool_call" => self.tool_call(update),
            "tool_call_update" => self.tool_call_update(update),
            "plan" => self.plan(update),
            _ => Vec::new(),
        }
    }

    fn close_run(&mut self) -> Vec<HarnessEvent> {
        let Some((run, item_id)) = self.run.take() else {
            return Vec::new();
        };
        vec![match run {
            Run::Message => HarnessEvent::AgentMessageChunk {
                item_id,
                text: String::new(),
                done: true,
            },
            Run::Thought => HarnessEvent::AgentThoughtChunk {
                item_id,
                text: String::new(),
                done: true,
            },
        }]
    }

    fn chunk(&mut self, run: Run, update: &Value) -> Vec<HarnessEvent> {
        let Some(text) = update.get("content").and_then(text_of) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        if self.run.as_ref().map(|(current, _)| *current) != Some(run) {
            out.extend(self.close_run());
            self.runs += 1;
            let turn = self.turn_id.clone().unwrap_or_default();
            let suffix = if run == Run::Message { "m" } else { "t" };
            self.run = Some((run, format!("{turn}-{suffix}{}", self.runs)));
        }
        let item_id = self
            .run
            .as_ref()
            .map(|(_, id)| id.clone())
            .unwrap_or_default();
        out.push(match run {
            Run::Message => HarnessEvent::AgentMessageChunk {
                item_id,
                text: text.to_string(),
                done: false,
            },
            Run::Thought => HarnessEvent::AgentThoughtChunk {
                item_id,
                text: text.to_string(),
                done: false,
            },
        });
        out
    }

    fn plan(&mut self, update: &Value) -> Vec<HarnessEvent> {
        let entries = update
            .get("entries")
            .and_then(Value::as_array)
            .map(|entries| {
                entries
                    .iter()
                    .map(|entry| PlanEntry {
                        content: str_at(entry, "content").unwrap_or_default().to_string(),
                        priority: str_at(entry, "priority").unwrap_or("medium").to_string(),
                        status: str_at(entry, "status").unwrap_or("pending").to_string(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        vec![HarnessEvent::Plan { entries }]
    }

    fn is_notice(&self, id: &str) -> bool {
        self.spec
            .notice_prefix()
            .is_some_and(|prefix| id.starts_with(prefix))
    }

    fn tool_call(&mut self, update: &Value) -> Vec<HarnessEvent> {
        let Some(id) = str_at(update, "toolCallId") else {
            return Vec::new();
        };
        let mut out = self.close_run();
        if self.is_notice(id) {
            let title = str_at(update, "title").unwrap_or_default();
            let body = content_of(update).map(|(text, _)| text).unwrap_or_default();
            let text = [title, body.as_str()]
                .into_iter()
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            if !text.is_empty() {
                out.push(HarnessEvent::Notice {
                    level: "warning".into(),
                    text,
                });
            }
            return out;
        }
        let turn = self.turn_id.clone().unwrap_or_default();
        let name = str_at(update, "title").unwrap_or("tool").to_string();
        self.tools.insert(
            id.to_string(),
            Tool {
                item_id: format!("{turn}-{id}"),
                name,
                title: None,
                kind: parse(update.get("kind")).unwrap_or(ToolKind::Other),
                raw_input: Value::Null,
                content: Vec::new(),
                announced: false,
                finished: false,
                output: String::new(),
            },
        );
        out.extend(self.tool_call_update(update));
        out
    }

    /// Fold what an update (or a permission request's `toolCall`) says about
    /// a call into what we know of it.
    fn merge(&mut self, update: &Value) -> Option<String> {
        let id = str_at(update, "toolCallId")?.to_string();
        if !self.tools.contains_key(&id) {
            // An update for a call never announced: a subagent's, which maki
            // only surfaces through its permission requests.
            let turn = self.turn_id.clone().unwrap_or_default();
            self.tools.insert(
                id.clone(),
                Tool {
                    item_id: format!("{turn}-{id}"),
                    name: str_at(update, "title").unwrap_or("tool").to_string(),
                    title: None,
                    kind: ToolKind::Other,
                    raw_input: Value::Null,
                    content: Vec::new(),
                    announced: false,
                    finished: false,
                    output: String::new(),
                },
            );
        }
        let tool = self.tools.get_mut(&id)?;
        if let Some(kind) = parse(update.get("kind")) {
            tool.kind = kind;
        }
        if let Some(title) = str_at(update, "title") {
            tool.title = Some(title.to_string());
        }
        if let Some(input) = update.get("rawInput").filter(|input| !input.is_null()) {
            tool.raw_input = input.clone();
        }
        Some(id)
    }

    fn ready(&self, tool: &Tool) -> bool {
        match tool.kind {
            // Announcing an edit before its diff exists would draw it as a
            // generic tool and then flip it to a file change.
            ToolKind::Edit => !self.diffs(tool).is_empty(),
            _ => !tool.raw_input.is_null(),
        }
    }

    fn diffs(&self, tool: &Tool) -> Vec<ToolCallContent> {
        let own: Vec<ToolCallContent> = tool
            .content
            .iter()
            .filter(|entry| matches!(entry, ToolCallContent::Diff { .. }))
            .cloned()
            .collect();
        if own.is_empty() {
            self.spec.tool_diff(&tool.name, &tool.raw_input)
        } else {
            own
        }
    }

    fn title(&self, tool: &Tool) -> String {
        if tool.kind == ToolKind::Execute {
            if let Some(command) = str_at(&tool.raw_input, "command") {
                return command.to_string();
            }
        }
        tool.title.clone().unwrap_or_else(|| tool.name.clone())
    }

    /// The content a call shows, given its output so far.
    fn shown(&self, tool: &Tool, output: &str) -> Vec<ToolCallContent> {
        match tool.kind {
            ToolKind::Execute => vec![ToolCallContent::Terminal {
                text: output.to_string(),
                exit_code: None,
                cwd: Some(self.cwd.clone()),
            }],
            ToolKind::Edit => {
                let mut content = self.diffs(tool);
                if content.is_empty() && !output.is_empty() {
                    content.push(ToolCallContent::Content {
                        text: output.to_string(),
                    });
                }
                content
            }
            _ if output.is_empty() => Vec::new(),
            _ => vec![ToolCallContent::Content {
                text: output.to_string(),
            }],
        }
    }

    fn announce(&mut self, id: &str, status: ToolCallStatus, output: &str) -> Vec<HarnessEvent> {
        let Some(tool) = self.tools.get(id) else {
            return Vec::new();
        };
        let event = HarnessEvent::ToolCall {
            item_id: tool.item_id.clone(),
            title: self.title(tool),
            kind: tool.kind,
            status,
            name: tool.name.clone(),
            content: self.shown(tool, output),
            raw_input: Json(tool.raw_input.clone()),
        };
        if let Some(tool) = self.tools.get_mut(id) {
            tool.announced = true;
            tool.output = output.to_string();
            tool.finished = matches!(
                status,
                ToolCallStatus::Completed | ToolCallStatus::Failed | ToolCallStatus::Cancelled
            );
        }
        vec![event]
    }

    fn tool_call_update(&mut self, update: &Value) -> Vec<HarnessEvent> {
        let Some(id) = self.merge(update) else {
            return Vec::new();
        };
        let status: Option<ToolCallStatus> = parse(update.get("status"));
        let content = content_of(update);
        if let Some((_, diffs)) = &content {
            if let Some(tool) = self.tools.get_mut(&id) {
                if !diffs.is_empty() {
                    tool.content = diffs.clone();
                }
            }
        }
        // `rawOutput` is the plain text; the content is the same fenced.
        let output = update
            .get("rawOutput")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| content.map(|(text, _)| text));
        let terminal = matches!(
            status,
            Some(ToolCallStatus::Completed | ToolCallStatus::Failed | ToolCallStatus::Cancelled)
        );
        let Some(tool) = self.tools.get(&id) else {
            return Vec::new();
        };
        if tool.finished {
            return Vec::new();
        }
        if !tool.announced {
            if terminal {
                let status = status.unwrap_or(ToolCallStatus::Completed);
                return self.announce(&id, status, output.as_deref().unwrap_or_default());
            }
            if self.ready(tool) {
                return self.announce(&id, ToolCallStatus::InProgress, "");
            }
            return Vec::new();
        }
        let item_id = tool.item_id.clone();
        if terminal {
            let text = output.unwrap_or_else(|| tool.output.clone());
            let shown = self.shown(tool, &text);
            if let Some(tool) = self.tools.get_mut(&id) {
                tool.finished = true;
            }
            return vec![HarnessEvent::ToolCallUpdate {
                item_id,
                status,
                content: Some(shown),
                output_delta: None,
            }];
        }
        let Some(text) = output.filter(|text| !text.is_empty()) else {
            return Vec::new();
        };
        // Output is resent whole each time: stream the new suffix, or
        // replace what is shown when it is not an extension.
        let event = match text.strip_prefix(tool.output.as_str()) {
            Some(suffix) if tool.kind == ToolKind::Execute => {
                if suffix.is_empty() {
                    return Vec::new();
                }
                HarnessEvent::ToolCallUpdate {
                    item_id,
                    status: None,
                    content: None,
                    output_delta: Some(suffix.to_string()),
                }
            }
            _ => HarnessEvent::ToolCallUpdate {
                item_id,
                status: None,
                content: Some(self.shown(tool, &text)),
                output_delta: None,
            },
        };
        if let Some(tool) = self.tools.get_mut(&id) {
            tool.output = text;
        }
        vec![event]
    }

    /// A permission request names a call that may not be announced yet.
    /// Returns the item id its card attaches to, and the events that
    /// announce it.
    pub(crate) fn permission(&mut self, tool_call: &Value) -> (String, Vec<HarnessEvent>) {
        let Some(id) = self.merge(tool_call) else {
            return (String::new(), Vec::new());
        };
        if let Some((_, diffs)) = content_of(tool_call) {
            if let (Some(tool), false) = (self.tools.get_mut(&id), diffs.is_empty()) {
                tool.content = diffs;
            }
        }
        let mut events = self.close_run();
        let item_id = self
            .tools
            .get(&id)
            .map(|tool| tool.item_id.clone())
            .unwrap_or_default();
        if self.tools.get(&id).is_some_and(|tool| !tool.announced) {
            events.extend(self.announce(&id, ToolCallStatus::Pending, ""));
        }
        (item_id, events)
    }

    /// The tool's own name and what its permission card should show, for a
    /// call [`Self::permission`] has seen.
    pub(crate) fn describe(&self, tool_call_id: &str) -> Option<(String, Vec<ToolCallContent>)> {
        let tool = self.tools.get(tool_call_id)?;
        Some((tool.name.clone(), self.shown(tool, "")))
    }

    /// The prompt's response arrived (or the process died). Returns the turn
    /// that ended and its closing events.
    pub(crate) fn end_turn(
        &mut self,
        stop_reason: StopReason,
        error: Option<String>,
    ) -> Option<(String, Vec<HarnessEvent>)> {
        let turn_id = self.turn_id.clone()?;
        let mut out = self.close_run();
        let closing = if stop_reason == StopReason::Cancelled {
            ToolCallStatus::Cancelled
        } else if error.is_some() {
            ToolCallStatus::Failed
        } else {
            ToolCallStatus::Completed
        };
        let mut open: Vec<&mut Tool> = self
            .tools
            .values_mut()
            .filter(|tool| tool.announced && !tool.finished)
            .collect();
        open.sort_by(|a, b| a.item_id.cmp(&b.item_id));
        for tool in open {
            tool.finished = true;
            out.push(HarnessEvent::ToolCallUpdate {
                item_id: tool.item_id.clone(),
                status: Some(closing),
                content: None,
                output_delta: None,
            });
        }
        let usage = self.usage_fresh.then(|| {
            let cost = self.usage.cost.unwrap_or(0.0);
            TurnUsage {
                // ACP reports the context in use, not a per-call split: the
                // last request's prompt is the closest thing to Codex's
                // `last.inputTokens`, which is what the context meter reads.
                input_tokens: self.usage.used,
                cached_input_tokens: 0,
                cache_write_input_tokens: 0,
                output_tokens: 0,
                reasoning_tokens: 0,
                context_window: self.usage.size,
                cost_usd: self.usage.cost.map(|_| {
                    // Cumulative per process: a drop means a new process.
                    if cost >= self.cost_at_start {
                        cost - self.cost_at_start
                    } else {
                        cost
                    }
                }),
                model: self.model.clone(),
            }
        });
        out.push(HarnessEvent::TurnEnded {
            turn_id: turn_id.clone(),
            stop_reason,
            error,
            duration_ms: self
                .started
                .take()
                .map(|started| started.elapsed().as_millis() as u64),
            usage,
        });
        self.turn_id = None;
        self.tools.clear();
        Some((turn_id, out))
    }
}

/// ACP's `stopReason` as the neutral enum.
pub(crate) fn stop_reason(result: &Value) -> StopReason {
    match str_at(result, "stopReason") {
        Some("max_tokens") => StopReason::MaxTokens,
        Some("max_turn_requests") => StopReason::MaxTurnRequests,
        Some("refusal") => StopReason::Refusal,
        Some("cancelled") => StopReason::Cancelled,
        _ => StopReason::EndTurn,
    }
}

#[cfg(test)]
mod fixtures {
    //! Golden streams recorded from maki 0.6.5 (`tests/fixtures/protocol/maki`).
    use super::*;
    use crate::acp::maki::Maki;
    use crate::harness::project::Projector;
    use serde_json::json;

    /// Replay a recording: a `session/prompt` going in opens a turn, its
    /// response closes it. Returns each turn's events in order, plus any
    /// events produced outside a turn.
    fn replay(wire: &str) -> (Vec<Vec<HarnessEvent>>, Vec<HarnessEvent>) {
        let mut translator = Translator::new("/tmp/pingex-fixture".into(), &Maki);
        let mut prompt = None;
        let mut turns = Vec::new();
        let mut current = Vec::new();
        let mut outside = Vec::new();
        for line in wire.lines() {
            let record: Value = serde_json::from_str(line).unwrap();
            let message = &record["msg"];
            let into = |events: Vec<HarnessEvent>,
                        current: &mut Vec<_>,
                        outside: &mut Vec<_>,
                        live: bool| {
                if live {
                    current.extend(events);
                } else {
                    outside.extend(events);
                }
            };
            match (record["dir"].as_str(), message["method"].as_str()) {
                (Some("in"), Some("session/prompt")) => {
                    prompt = message["id"].as_i64();
                    translator.begin_turn(&format!("turn{}", turns.len() + 1), None);
                }
                (Some("out"), Some("session/update")) => {
                    let events = translator.update(&message["params"]["update"]);
                    into(events, &mut current, &mut outside, prompt.is_some());
                }
                (Some("out"), Some("session/request_permission")) => {
                    let (_, events) = translator.permission(&message["params"]["toolCall"]);
                    current.extend(events);
                }
                (Some("out"), None) if message["id"].as_i64() == prompt && prompt.is_some() => {
                    let (stop, error) = match message.get("error") {
                        Some(error) => (
                            StopReason::Error,
                            error["message"].as_str().map(str::to_string),
                        ),
                        None => (stop_reason(&message["result"]), None),
                    };
                    let (_, events) = translator.end_turn(stop, error).unwrap();
                    current.extend(events);
                    turns.push(std::mem::take(&mut current));
                    prompt = None;
                }
                _ => {}
            }
        }
        (turns, outside)
    }

    fn message_text(events: &[HarnessEvent]) -> String {
        events
            .iter()
            .filter_map(|event| match event {
                HarnessEvent::AgentMessageChunk { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect()
    }

    fn ended(events: &[HarnessEvent]) -> (StopReason, Option<TurnUsage>) {
        match events.last() {
            Some(HarnessEvent::TurnEnded {
                stop_reason, usage, ..
            }) => (*stop_reason, usage.clone()),
            other => panic!("turn did not end last: {other:?}"),
        }
    }

    const APPROVE_DENY_WRITE: &str =
        include_str!("../../../tests/fixtures/protocol/maki/approve-deny-write/wire.ndjson");

    #[test]
    fn a_reply_streams_and_reports_the_context_and_its_cost() {
        let (turns, outside) = replay(APPROVE_DENY_WRITE);
        assert!(outside.is_empty());
        assert_eq!(turns.len(), 4);
        assert_eq!(message_text(&turns[0]), "pong");
        let (stop, usage) = ended(&turns[0]);
        assert_eq!(stop, StopReason::EndTurn);
        let usage = usage.unwrap();
        assert_eq!(usage.input_tokens, 5117);
        assert_eq!(usage.context_window, Some(272000));
        assert!((usage.cost_usd.unwrap() - 0.0005142).abs() < 1e-9);
        // The second turn costs only its own share of the running total.
        let second = ended(&turns[1]).1.unwrap().cost_usd.unwrap();
        assert!((second - (0.001577 - 0.0005142)).abs() < 1e-9, "{second}");
    }

    #[test]
    fn an_approved_command_is_one_card_with_its_output() {
        let (turns, _) = replay(APPROVE_DENY_WRITE);
        let calls: Vec<_> = turns[1]
            .iter()
            .filter(|event| matches!(event, HarnessEvent::ToolCall { .. }))
            .collect();
        assert_eq!(calls.len(), 1, "announced once, permission included");
        let HarnessEvent::ToolCall {
            title,
            kind,
            status,
            content,
            ..
        } = calls[0]
        else {
            unreachable!()
        };
        assert_eq!(title, "for i in 1 2 3; do echo line$i; sleep 1; done");
        assert_eq!(*kind, ToolKind::Execute);
        assert_eq!(*status, ToolCallStatus::InProgress);
        assert!(
            matches!(&content[..], [ToolCallContent::Terminal { text, .. }] if text.is_empty())
        );
        let done = turns[1].iter().find_map(|event| match event {
            HarnessEvent::ToolCallUpdate {
                status: Some(ToolCallStatus::Completed),
                content: Some(content),
                ..
            } => Some(content.clone()),
            _ => None,
        });
        assert!(matches!(
            done.as_deref(),
            Some([ToolCallContent::Terminal { text, .. }]) if text == "line1\nline2\nline3"
        ));
        assert_eq!(message_text(&turns[1]), "done");
    }

    #[test]
    fn a_denied_command_fails_with_maki_s_reason() {
        let (turns, _) = replay(APPROVE_DENY_WRITE);
        let failed = turns[2].iter().find_map(|event| match event {
            HarnessEvent::ToolCallUpdate {
                status: Some(ToolCallStatus::Failed),
                content: Some(content),
                ..
            } => Some(content.clone()),
            _ => None,
        });
        assert!(matches!(
            failed.as_deref(),
            Some([ToolCallContent::Terminal { text, .. }]) if text.starts_with("Permission denied for `bash`")
        ));
    }

    #[test]
    fn a_write_inside_the_project_becomes_a_file_change() {
        let (turns, _) = replay(APPROVE_DENY_WRITE);
        let mut projector = Projector::default();
        let items: Vec<Value> = turns[3]
            .iter()
            .flat_map(|event| projector.project("thread", "turn4", event))
            .filter(|(method, _)| *method == "item/completed")
            .map(|(_, params)| params["item"].clone())
            .collect();
        let change = items
            .iter()
            .find(|item| item["type"] == "fileChange")
            .expect("a file change");
        assert_eq!(change["status"], "completed");
        assert_eq!(change["changes"][0]["path"], "/tmp/pingex-fixture/note.txt");
        assert_eq!(change["changes"][0]["kind"]["type"], "add");
        assert!(items.iter().any(|item| item["type"] == "commandExecution"));
    }

    #[test]
    fn a_loaded_session_replays_nothing_into_the_thread() {
        let (turns, outside) = replay(include_str!(
            "../../../tests/fixtures/protocol/maki/load-plan/wire.ndjson"
        ));
        assert!(outside.is_empty(), "{outside:?}");
        assert_eq!(turns.len(), 1);
        assert!(message_text(&turns[0]).contains("plan mode"));
        assert!(!turns[0].iter().any(|event| matches!(
            event,
            HarnessEvent::ToolCall { .. } | HarnessEvent::UserMessage { .. }
        )));
    }

    #[test]
    fn cancelling_with_a_card_open_fails_the_call_and_ends_cancelled() {
        let (turns, outside) = replay(include_str!(
            "../../../tests/fixtures/protocol/maki/cancel-with-permission/wire.ndjson"
        ));
        assert!(outside.is_empty());
        let turn = &turns[0];
        assert_eq!(ended(turn).0, StopReason::Cancelled);
        assert!(turn.iter().any(|event| matches!(
            event,
            HarnessEvent::ToolCallUpdate {
                status: Some(ToolCallStatus::Failed),
                ..
            }
        )));
        // Nothing left open for end_turn to close.
        assert!(!turn.iter().any(|event| matches!(
            event,
            HarnessEvent::ToolCallUpdate {
                status: Some(ToolCallStatus::Cancelled),
                ..
            }
        )));
    }

    fn update(kind: &str, extra: Value) -> Value {
        let mut update = json!({"sessionUpdate": kind});
        for (key, value) in extra.as_object().unwrap() {
            update[key] = value.clone();
        }
        update
    }

    #[test]
    fn accumulated_output_streams_as_suffixes_or_replaces() {
        let mut translator = Translator::new("/w".into(), &Maki);
        translator.begin_turn("t", None);
        translator.update(&update(
            "tool_call",
            json!({"toolCallId": "c", "title": "bash", "kind": "execute"}),
        ));
        let started = translator.update(&update(
            "tool_call_update",
            json!({"toolCallId": "c", "status": "in_progress", "rawInput": {"command": "make"}}),
        ));
        assert!(matches!(&started[..], [HarnessEvent::ToolCall { title, .. }] if title == "make"));
        let chunk = |translator: &mut Translator, text: &str| {
            translator.update(&update(
                "tool_call_update",
                json!({"toolCallId": "c", "content": [{"type": "content", "content": {"type": "text", "text": text}}]}),
            ))
        };
        assert!(matches!(
            &chunk(&mut translator, "a\n")[..],
            [HarnessEvent::ToolCallUpdate { output_delta: Some(delta), .. }] if delta == "a\n"
        ));
        assert!(matches!(
            &chunk(&mut translator, "a\nb\n")[..],
            [HarnessEvent::ToolCallUpdate { output_delta: Some(delta), .. }] if delta == "b\n"
        ));
        assert!(chunk(&mut translator, "a\nb\n").is_empty());
        assert!(matches!(
            &chunk(&mut translator, "trimmed\n")[..],
            [HarnessEvent::ToolCallUpdate {
                content: Some(_),
                output_delta: None,
                ..
            }]
        ));
    }

    #[test]
    fn notices_are_warnings_not_tools() {
        let mut translator = Translator::new("/w".into(), &Maki);
        translator.begin_turn("t", None);
        let events = translator.update(&update(
            "tool_call",
            json!({"toolCallId": "notice-1", "title": "Context compacted", "status": "completed"}),
        ));
        assert!(matches!(
            &events[..],
            [HarnessEvent::Notice { text, .. }] if text == "Context compacted"
        ));
    }

    #[test]
    fn a_new_process_restarts_the_cost_from_zero() {
        let mut translator = Translator::new("/w".into(), &Maki);
        let usage = |translator: &mut Translator, cost: f64| {
            translator.update(&update(
                "usage_update",
                json!({"used": 10, "size": 100, "cost": {"amount": cost, "currency": "USD"}}),
            ));
        };
        translator.begin_turn("a", None);
        usage(&mut translator, 0.5);
        let (_, events) = translator.end_turn(StopReason::EndTurn, None).unwrap();
        assert_eq!(ended(&events).1.unwrap().cost_usd, Some(0.5));
        translator.begin_turn("b", None);
        // A turn with no usage update reports none.
        let (_, events) = translator.end_turn(StopReason::EndTurn, None).unwrap();
        assert!(ended(&events).1.is_none());
    }
}
