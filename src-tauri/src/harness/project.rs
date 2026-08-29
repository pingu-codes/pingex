//! Project neutral events onto the Codex notification vocabulary the journal
//! and the transcript still speak. Transitional: goes away with the reducer
//! migration described in `features/13-harnesses.md`.

use serde_json::{json, Value};

use super::{HarnessEvent, StopReason, ToolCallContent, ToolCallStatus, ToolKind};

/// One Codex-shaped notification: `(method, params)`.
pub(crate) type Notification = (&'static str, Value);

fn item_status(status: ToolCallStatus) -> &'static str {
    match status {
        ToolCallStatus::Pending | ToolCallStatus::InProgress => "inProgress",
        ToolCallStatus::Completed => "completed",
        ToolCallStatus::Failed => "failed",
        ToolCallStatus::Cancelled => "declined",
    }
}

fn turn_status(stop: StopReason) -> &'static str {
    match stop {
        StopReason::EndTurn | StopReason::MaxTokens | StopReason::MaxTurnRequests => "completed",
        StopReason::Cancelled => "interrupted",
        StopReason::Refusal | StopReason::Error => "failed",
    }
}

/// A unified diff for one file, good enough for the diff card. Line numbers
/// are nominal: the card renders hunks, it does not apply them.
pub(crate) fn unified_diff(path: &str, old_text: Option<&str>, new_text: &str) -> String {
    let old_lines: Vec<&str> = old_text
        .map(|text| text.lines().collect())
        .unwrap_or_default();
    let new_lines: Vec<&str> = new_text.lines().collect();
    let mut out = format!("--- a/{path}\n+++ b/{path}\n");
    if old_text.is_none() {
        out.push_str(&format!("@@ -0,0 +1,{} @@\n", new_lines.len()));
        for line in &new_lines {
            out.push('+');
            out.push_str(line);
            out.push('\n');
        }
        return out;
    }
    // Trim the common prefix and suffix so a one-line edit reads as one line.
    let prefix = old_lines
        .iter()
        .zip(new_lines.iter())
        .take_while(|(a, b)| a == b)
        .count();
    let max_suffix = old_lines.len().min(new_lines.len()) - prefix;
    let suffix = old_lines
        .iter()
        .rev()
        .zip(new_lines.iter().rev())
        .take(max_suffix)
        .take_while(|(a, b)| a == b)
        .count();
    let old_mid = &old_lines[prefix..old_lines.len() - suffix];
    let new_mid = &new_lines[prefix..new_lines.len() - suffix];
    out.push_str(&format!(
        "@@ -{},{} +{},{} @@\n",
        prefix + 1,
        old_mid.len(),
        prefix + 1,
        new_mid.len()
    ));
    for line in old_mid {
        out.push('-');
        out.push_str(line);
        out.push('\n');
    }
    for line in new_mid {
        out.push('+');
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// The `changes` array of a Codex `fileChange` item from diff content.
pub(crate) fn file_changes(content: &[ToolCallContent]) -> Vec<Value> {
    content
        .iter()
        .filter_map(|entry| match entry {
            ToolCallContent::Diff {
                path,
                old_text,
                new_text,
            } => {
                let kind = if old_text.is_none() {
                    "add"
                } else if new_text.is_empty() {
                    "delete"
                } else {
                    "update"
                };
                Some(json!({
                    "path": path,
                    "kind": {"type": kind},
                    "diff": unified_diff(path, old_text.as_deref(), new_text),
                }))
            }
            _ => None,
        })
        .collect()
}

fn terminal(content: &[ToolCallContent]) -> Option<(&str, Option<i64>, Option<&str>)> {
    content.iter().find_map(|entry| match entry {
        ToolCallContent::Terminal {
            text,
            exit_code,
            cwd,
        } => Some((text.as_str(), *exit_code, cwd.as_deref())),
        _ => None,
    })
}

fn text_content(content: &[ToolCallContent]) -> String {
    content
        .iter()
        .filter_map(|entry| match entry {
            ToolCallContent::Content { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The Codex item a tool call renders as. `execute` with terminal content is
/// a `commandExecution`; `edit` with diffs is a `fileChange`; everything else
/// is a `dynamicToolCall`, which the transcript draws as a generic tool card.
fn tool_item(
    item_id: &str,
    title: &str,
    kind: ToolKind,
    status: ToolCallStatus,
    name: &str,
    content: &[ToolCallContent],
    raw_input: &Value,
) -> Value {
    match kind {
        ToolKind::Execute if terminal(content).is_some() => {
            let (text, exit_code, cwd) = terminal(content).unwrap_or_default();
            json!({
                "type": "commandExecution",
                "id": item_id,
                "command": title,
                "cwd": cwd,
                "status": item_status(status),
                "aggregatedOutput": text,
                "exitCode": exit_code,
            })
        }
        ToolKind::Edit if !file_changes(content).is_empty() => json!({
            "type": "fileChange",
            "id": item_id,
            "status": item_status(status),
            "changes": file_changes(content),
        }),
        _ => {
            let output = text_content(content);
            json!({
                "type": "dynamicToolCall",
                "id": item_id,
                "tool": name,
                "title": title,
                "arguments": raw_input,
                "status": item_status(status),
                "output": if output.is_empty() { Value::Null } else { json!(output) },
            })
        }
    }
}

/// State the projection needs across events: what each open tool call was
/// announced as, so an update can re-emit the whole item.
#[derive(Default)]
pub(crate) struct Projector {
    open_tools: std::collections::HashMap<String, OpenTool>,
    message_text: std::collections::HashMap<String, String>,
}

struct OpenTool {
    title: String,
    kind: ToolKind,
    name: String,
    content: Vec<ToolCallContent>,
    raw_input: Value,
    output: String,
}

impl Projector {
    /// Codex-shaped notifications for one neutral event.
    pub(crate) fn project(
        &mut self,
        thread_id: &str,
        turn_id: &str,
        event: &HarnessEvent,
    ) -> Vec<Notification> {
        let base = |extra: Value| {
            let mut params = json!({"threadId": thread_id, "turnId": turn_id});
            if let (Some(target), Some(source)) = (params.as_object_mut(), extra.as_object()) {
                for (key, value) in source {
                    target.insert(key.clone(), value.clone());
                }
            }
            params
        };
        match event {
            HarnessEvent::TurnStarted { turn_id, .. } => vec![(
                "turn/started",
                json!({"threadId": thread_id, "turn": {"id": turn_id}}),
            )],
            HarnessEvent::TurnEnded {
                turn_id,
                stop_reason,
                error,
                duration_ms,
                usage,
            } => {
                let mut out = Vec::new();
                if let Some(usage) = usage {
                    let total = usage.input_tokens + usage.output_tokens;
                    let breakdown = json!({
                        "totalTokens": total,
                        "inputTokens": usage.input_tokens,
                        "cachedInputTokens": usage.cached_input_tokens,
                        "outputTokens": usage.output_tokens,
                        "reasoningOutputTokens": 0,
                    });
                    out.push((
                        "thread/tokenUsage/updated",
                        json!({
                            "threadId": thread_id,
                            "turnId": turn_id,
                            "tokenUsage": {
                                "total": breakdown,
                                "last": breakdown,
                                "modelContextWindow": usage.context_window,
                            },
                        }),
                    ));
                }
                let mut turn = json!({
                    "id": turn_id,
                    "status": turn_status(*stop_reason),
                    "durationMs": duration_ms,
                });
                if let Some(message) = error {
                    turn["error"] = json!({"message": message});
                }
                out.push((
                    "turn/completed",
                    json!({"threadId": thread_id, "turn": turn}),
                ));
                out
            }
            HarnessEvent::UserMessage { item_id, text } => {
                let item = json!({
                    "type": "userMessage",
                    "id": item_id,
                    "content": [{"type": "text", "text": text}],
                });
                vec![
                    ("item/started", base(json!({"item": item}))),
                    ("item/completed", base(json!({"item": item}))),
                ]
            }
            HarnessEvent::AgentMessageChunk {
                item_id,
                text,
                done,
            } => {
                let mut out = Vec::new();
                let entry = self.message_text.entry(item_id.clone());
                let first = matches!(entry, std::collections::hash_map::Entry::Vacant(_));
                let buffer = entry.or_default();
                if first {
                    out.push((
                        "item/started",
                        base(json!({"item": {"type": "agentMessage", "id": item_id}})),
                    ));
                }
                if !text.is_empty() {
                    buffer.push_str(text);
                    out.push((
                        "item/agentMessage/delta",
                        base(json!({"itemId": item_id, "delta": text})),
                    ));
                }
                if *done {
                    let full = self.message_text.remove(item_id).unwrap_or_default();
                    out.push((
                        "item/completed",
                        base(
                            json!({"item": {"type": "agentMessage", "id": item_id, "text": full}}),
                        ),
                    ));
                }
                out
            }
            HarnessEvent::AgentThoughtChunk {
                item_id,
                text,
                done,
            } => {
                let mut out = Vec::new();
                let key = format!("thought:{item_id}");
                let first = !self.message_text.contains_key(&key);
                if first {
                    self.message_text.insert(key.clone(), String::new());
                    out.push((
                        "item/started",
                        base(json!({"item": {"type": "reasoning", "id": item_id}})),
                    ));
                    out.push((
                        "item/reasoning/summaryPartAdded",
                        base(json!({"itemId": item_id, "summaryIndex": 0})),
                    ));
                }
                if !text.is_empty() {
                    if let Some(buffer) = self.message_text.get_mut(&key) {
                        buffer.push_str(text);
                    }
                    out.push((
                        "item/reasoning/summaryTextDelta",
                        base(json!({"itemId": item_id, "summaryIndex": 0, "delta": text})),
                    ));
                }
                if *done {
                    let full = self.message_text.remove(&key).unwrap_or_default();
                    out.push((
                        "item/completed",
                        base(json!({"item": {"type": "reasoning", "id": item_id, "summary": [full]}})),
                    ));
                }
                out
            }
            HarnessEvent::ToolCall {
                item_id,
                title,
                kind,
                status,
                name,
                content,
                raw_input,
            } => {
                let item = tool_item(item_id, title, *kind, *status, name, content, &raw_input.0);
                self.open_tools.insert(
                    item_id.clone(),
                    OpenTool {
                        title: title.clone(),
                        kind: *kind,
                        name: name.clone(),
                        content: content.clone(),
                        raw_input: raw_input.0.clone(),
                        output: String::new(),
                    },
                );
                let method = if matches!(status, ToolCallStatus::Completed | ToolCallStatus::Failed)
                {
                    self.open_tools.remove(item_id);
                    "item/completed"
                } else {
                    "item/started"
                };
                vec![(method, base(json!({"item": item})))]
            }
            HarnessEvent::ToolCallUpdate {
                item_id,
                status,
                content,
                output_delta,
            } => {
                let Some(open) = self.open_tools.get_mut(item_id) else {
                    return Vec::new();
                };
                let mut out = Vec::new();
                if let Some(delta) = output_delta {
                    open.output.push_str(delta);
                    if open.kind == ToolKind::Execute {
                        out.push((
                            "item/commandExecution/outputDelta",
                            base(json!({"itemId": item_id, "delta": delta})),
                        ));
                    }
                }
                if let Some(content) = content {
                    open.content = content.clone();
                }
                if let Some(status) = status {
                    // Fold streamed output back into the terminal content so
                    // the completed item carries the whole text.
                    if !open.output.is_empty() {
                        let mut folded = false;
                        for entry in &mut open.content {
                            if let ToolCallContent::Terminal { text, .. } = entry {
                                if text.is_empty() {
                                    *text = open.output.clone();
                                }
                                folded = true;
                            }
                        }
                        if !folded && open.kind == ToolKind::Execute {
                            open.content.push(ToolCallContent::Terminal {
                                text: open.output.clone(),
                                exit_code: None,
                                cwd: None,
                            });
                        } else if !folded {
                            open.content.push(ToolCallContent::Content {
                                text: open.output.clone(),
                            });
                        }
                    }
                    let item = tool_item(
                        item_id,
                        &open.title,
                        open.kind,
                        *status,
                        &open.name,
                        &open.content,
                        &open.raw_input,
                    );
                    if matches!(
                        status,
                        ToolCallStatus::Completed
                            | ToolCallStatus::Failed
                            | ToolCallStatus::Cancelled
                    ) {
                        self.open_tools.remove(item_id);
                        out.push(("item/completed", base(json!({"item": item}))));
                    } else {
                        out.push(("item/started", base(json!({"item": item}))));
                    }
                }
                out
            }
            HarnessEvent::Plan { entries } => vec![(
                "turn/plan/updated",
                json!({
                    "threadId": thread_id,
                    "turnId": turn_id,
                    "explanation": null,
                    "plan": entries.iter().map(|entry| json!({
                        "step": entry.content,
                        "status": match entry.status.as_str() {
                            "in_progress" => "inProgress",
                            other => other,
                        },
                    })).collect::<Vec<_>>(),
                }),
            )],
            HarnessEvent::Compaction { item_id, .. } => {
                let item = json!({"type": "contextCompaction", "id": item_id});
                vec![
                    ("item/started", base(json!({"item": item}))),
                    ("item/completed", base(json!({"item": item}))),
                ]
            }
            HarnessEvent::Notice { level, text } => {
                if level == "error" {
                    vec![(
                        "error",
                        json!({"threadId": thread_id, "turnId": turn_id, "error": {"message": text}, "willRetry": false}),
                    )]
                } else {
                    vec![("warning", json!({"threadId": thread_id, "message": text}))]
                }
            }
            HarnessEvent::RequestCancelled { request_id } => vec![(
                "serverRequest/resolved",
                json!({"threadId": thread_id, "requestId": request_id}),
            )],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unified_diff_trims_common_lines() {
        let diff = unified_diff("a.rs", Some("a\nb\nc\n"), "a\nB\nc\n");
        assert!(diff.contains("@@ -2,1 +2,1 @@"));
        assert!(diff.contains("-b\n+B\n"));
        assert!(!diff.contains("-a"));
    }

    #[test]
    fn a_new_file_is_all_additions() {
        let diff = unified_diff("new.txt", None, "x\ny\n");
        assert!(diff.contains("@@ -0,0 +1,2 @@"));
        assert!(diff.contains("+x\n+y\n"));
    }

    #[test]
    fn a_message_projects_to_started_deltas_and_completed() {
        let mut projector = Projector::default();
        let first = projector.project(
            "t",
            "turn",
            &HarnessEvent::AgentMessageChunk {
                item_id: "m".into(),
                text: "Hel".into(),
                done: false,
            },
        );
        assert_eq!(first[0].0, "item/started");
        assert_eq!(first[1].0, "item/agentMessage/delta");
        let last = projector.project(
            "t",
            "turn",
            &HarnessEvent::AgentMessageChunk {
                item_id: "m".into(),
                text: "lo".into(),
                done: true,
            },
        );
        let completed = last
            .iter()
            .find(|(method, _)| *method == "item/completed")
            .unwrap();
        assert_eq!(completed.1["item"]["text"], "Hello");
    }

    #[test]
    fn a_command_keeps_its_streamed_output_when_it_completes() {
        let mut projector = Projector::default();
        projector.project(
            "t",
            "turn",
            &HarnessEvent::ToolCall {
                item_id: "c".into(),
                title: "ls".into(),
                kind: ToolKind::Execute,
                status: ToolCallStatus::InProgress,
                name: "Bash".into(),
                content: vec![ToolCallContent::Terminal {
                    text: String::new(),
                    exit_code: None,
                    cwd: Some("/repo".into()),
                }],
                raw_input: Json(json!({})),
            },
        );
        projector.project(
            "t",
            "turn",
            &HarnessEvent::ToolCallUpdate {
                item_id: "c".into(),
                status: None,
                content: None,
                output_delta: Some("a.txt\n".into()),
            },
        );
        let done = projector.project(
            "t",
            "turn",
            &HarnessEvent::ToolCallUpdate {
                item_id: "c".into(),
                status: Some(ToolCallStatus::Completed),
                content: None,
                output_delta: None,
            },
        );
        let item = &done[0].1["item"];
        assert_eq!(item["type"], "commandExecution");
        assert_eq!(item["aggregatedOutput"], "a.txt\n");
        assert_eq!(item["status"], "completed");
    }

    #[test]
    fn edits_project_to_a_file_change() {
        let mut projector = Projector::default();
        let out = projector.project(
            "t",
            "turn",
            &HarnessEvent::ToolCall {
                item_id: "e".into(),
                title: "Edit a.rs".into(),
                kind: ToolKind::Edit,
                status: ToolCallStatus::Completed,
                name: "Edit".into(),
                content: vec![ToolCallContent::Diff {
                    path: "a.rs".into(),
                    old_text: Some("x".into()),
                    new_text: "y".into(),
                }],
                raw_input: Json(json!({})),
            },
        );
        assert_eq!(out[0].0, "item/completed");
        assert_eq!(out[0].1["item"]["type"], "fileChange");
        assert_eq!(out[0].1["item"]["changes"][0]["kind"]["type"], "update");
    }

    use crate::util::json::Json;
    use serde_json::json;
}
