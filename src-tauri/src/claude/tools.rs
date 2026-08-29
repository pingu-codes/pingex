//! What each Claude tool looks like as a neutral tool call: the card it gets,
//! its title, and the content the card draws before the tool has run.
//! `features/13-harnesses.md`, "Approvals and user input on Claude".

use serde_json::Value;

use crate::harness::{HarnessEvent, ToolCallContent, ToolCallStatus, ToolKind};
use crate::util::json::{str_at, Json};

/// Tools whose result is bookkeeping, not output: the plan event already
/// carried their content, so their `tool_result` text is dropped.
pub(crate) fn drives_plan(name: &str) -> bool {
    matches!(
        name,
        "TodoWrite" | "TaskCreate" | "TaskUpdate" | "TaskList" | "TaskGet"
    )
}

pub(crate) fn kind_for(name: &str) -> ToolKind {
    match name {
        "Bash" => ToolKind::Execute,
        "Edit" | "MultiEdit" | "Write" | "NotebookEdit" => ToolKind::Edit,
        "Read" => ToolKind::Read,
        "Glob" | "Grep" => ToolKind::Search,
        "WebFetch" | "WebSearch" => ToolKind::Fetch,
        "Agent" | "Task" | "TodoWrite" | "TaskCreate" | "TaskUpdate" | "TaskList" | "TaskGet" => {
            ToolKind::Think
        }
        "ExitPlanMode" => ToolKind::SwitchMode,
        _ => ToolKind::Other,
    }
}

pub(crate) fn title_for(name: &str, input: &Value) -> String {
    match name {
        "Bash" => str_at(input, "command")
            .unwrap_or("Run command")
            .to_string(),
        "Edit" | "MultiEdit" => format!("Edit {}", str_at(input, "file_path").unwrap_or("file")),
        "Write" => format!("Write {}", str_at(input, "file_path").unwrap_or("file")),
        "NotebookEdit" => format!(
            "Edit {}",
            str_at(input, "notebook_path").unwrap_or("notebook")
        ),
        "Read" => format!("Read {}", str_at(input, "file_path").unwrap_or("file")),
        "Glob" | "Grep" => format!("{name} {}", str_at(input, "pattern").unwrap_or("")),
        "WebFetch" => str_at(input, "url").unwrap_or("Fetch").to_string(),
        "WebSearch" => format!("Search: {}", str_at(input, "query").unwrap_or("")),
        "Agent" | "Task" => str_at(input, "description")
            .unwrap_or("Subagent")
            .to_string(),
        "Skill" => format!("Skill {}", str_at(input, "skill").unwrap_or("")),
        "ExitPlanMode" => "Plan".to_string(),
        "AskUserQuestion" => "Question".to_string(),
        _ => match name.strip_prefix("mcp__") {
            Some(rest) => rest.replacen("__", " / ", 1),
            None => name.to_string(),
        },
    }
}

fn diff(path: &str, old: Option<String>, new: String) -> ToolCallContent {
    ToolCallContent::Diff {
        path: path.to_string(),
        old_text: old,
        new_text: new,
    }
}

/// The content a call shows before it runs. Edits carry their diff up front
/// so the approval card and the transcript draw the same thing.
pub(crate) fn initial_content(name: &str, input: &Value, cwd: &str) -> Vec<ToolCallContent> {
    match name {
        "Bash" => vec![ToolCallContent::Terminal {
            text: String::new(),
            exit_code: None,
            cwd: Some(cwd.to_string()),
        }],
        "Edit" => {
            let path = str_at(input, "file_path").unwrap_or_default();
            vec![diff(
                path,
                Some(str_at(input, "old_string").unwrap_or_default().to_string()),
                str_at(input, "new_string").unwrap_or_default().to_string(),
            )]
        }
        "MultiEdit" => {
            let path = str_at(input, "file_path").unwrap_or_default();
            input
                .get("edits")
                .and_then(Value::as_array)
                .map(|edits| {
                    edits
                        .iter()
                        .map(|edit| {
                            diff(
                                path,
                                Some(str_at(edit, "old_string").unwrap_or_default().to_string()),
                                str_at(edit, "new_string").unwrap_or_default().to_string(),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default()
        }
        "Write" => {
            let path = str_at(input, "file_path").unwrap_or_default();
            let old = std::fs::read_to_string(path).ok();
            vec![diff(
                path,
                old,
                str_at(input, "content").unwrap_or_default().to_string(),
            )]
        }
        "NotebookEdit" => {
            let path = str_at(input, "notebook_path").unwrap_or_default();
            vec![diff(
                path,
                None,
                str_at(input, "new_source").unwrap_or_default().to_string(),
            )]
        }
        "ExitPlanMode" => vec![ToolCallContent::Content {
            text: str_at(input, "plan").unwrap_or_default().to_string(),
        }],
        "Agent" | "Task" => vec![ToolCallContent::Content {
            text: str_at(input, "prompt").unwrap_or_default().to_string(),
        }],
        _ => Vec::new(),
    }
}

pub(crate) fn tool_call(
    tool_use_id: &str,
    name: &str,
    input: &Value,
    cwd: &str,
    status: ToolCallStatus,
) -> HarnessEvent {
    HarnessEvent::ToolCall {
        item_id: tool_use_id.to_string(),
        title: title_for(name, input),
        kind: kind_for(name),
        status,
        name: name.to_string(),
        content: initial_content(name, input, cwd),
        raw_input: Json(input.clone()),
    }
}

/// The text of a `tool_result` block, whatever shape it came in.
pub(crate) fn result_text(block: &Value) -> String {
    match block.get("content") {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|part| str_at(part, "text"))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn bash_is_an_execute_call_with_a_terminal() {
        let event = tool_call(
            "t1",
            "Bash",
            &json!({"command": "ls"}),
            "/repo",
            ToolCallStatus::InProgress,
        );
        let HarnessEvent::ToolCall {
            title,
            kind,
            content,
            ..
        } = event
        else {
            panic!("not a tool call");
        };
        assert_eq!(title, "ls");
        assert_eq!(kind, ToolKind::Execute);
        assert!(matches!(content[0], ToolCallContent::Terminal { .. }));
    }

    #[test]
    fn edit_carries_its_diff_up_front() {
        let input = json!({"file_path": "a.rs", "old_string": "x", "new_string": "y"});
        let content = initial_content("Edit", &input, "/repo");
        assert!(
            matches!(&content[0], ToolCallContent::Diff { path, old_text: Some(o), new_text } if path == "a.rs" && o == "x" && new_text == "y")
        );
    }

    #[test]
    fn mcp_tools_get_a_readable_title() {
        assert_eq!(
            title_for("mcp__linear__save_issue", &json!({})),
            "linear / save_issue"
        );
        assert_eq!(kind_for("mcp__linear__save_issue"), ToolKind::Other);
    }
}
