//! `session/request_permission` in, a permission request out; an option id
//! (or a Codex decision word) in, an ACP outcome out.

use serde_json::{json, Value};

use crate::harness::{HarnessRequest, PermissionOption, ToolCallContent, ToolKind};
use crate::util::json::{str_at, Json};

pub(crate) fn options(params: &Value) -> Vec<PermissionOption> {
    params
        .get("options")
        .and_then(Value::as_array)
        .map(|options| {
            options
                .iter()
                .filter_map(|option| {
                    Some(PermissionOption {
                        option_id: str_at(option, "optionId")?.to_string(),
                        name: str_at(option, "name").unwrap_or_default().to_string(),
                        kind: str_at(option, "kind").unwrap_or("allow_once").to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The card for a permission request. `content` is what the translator knows
/// the call shows (diffs included); `name` is the tool's own name.
pub(crate) fn request_for(
    params: &Value,
    name: &str,
    content: Vec<ToolCallContent>,
    cwd: &str,
) -> HarnessRequest {
    let tool_call = params.get("toolCall").cloned().unwrap_or(Value::Null);
    let kind: ToolKind = tool_call
        .get("kind")
        .and_then(|kind| serde_json::from_value(kind.clone()).ok())
        .unwrap_or(ToolKind::Other);
    let input = tool_call.get("rawInput").cloned().unwrap_or(Value::Null);
    let command = (kind == ToolKind::Execute)
        .then(|| str_at(&input, "command").map(str::to_string))
        .flatten();
    let changes = Json(Value::Array(crate::harness::project::file_changes(
        &content,
    )));
    HarnessRequest::Permission {
        title: str_at(&tool_call, "title").unwrap_or(name).to_string(),
        description: str_at(&input, "description").map(str::to_string),
        kind,
        name: name.to_string(),
        content,
        options: options(params),
        reason: None,
        default_to_reject: false,
        cwd: command.is_some().then(|| cwd.to_string()),
        command,
        changes,
    }
}

/// The option an answer picks: an option id as offered, or a Codex decision
/// word mapped onto the first option of the matching kind.
pub(crate) fn choose<'a>(options: &'a [PermissionOption], answer: &str) -> Option<&'a str> {
    if let Some(option) = options.iter().find(|option| option.option_id == answer) {
        return Some(&option.option_id);
    }
    let kinds: &[&str] = match answer {
        "accept" | "approved" => &["allow_once", "allow_always"],
        "acceptForSession" | "approved_for_session" => &["allow_always", "allow_once"],
        "decline" | "denied" | "abort" => &["reject_once", "reject_always"],
        _ => return None,
    };
    kinds.iter().find_map(|kind| {
        options
            .iter()
            .find(|option| option.kind == *kind)
            .map(|option| option.option_id.as_str())
    })
}

pub fn selected(option_id: &str) -> Value {
    json!({"outcome": {"outcome": "selected", "optionId": option_id}})
}

/// What a prompt turn being cancelled answers every open request with.
pub fn cancelled() -> Value {
    json!({"outcome": {"outcome": "cancelled"}})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn maki_options() -> Vec<PermissionOption> {
        options(&json!({"options": [
            {"optionId": "allow_once", "name": "Allow once", "kind": "allow_once"},
            {"optionId": "allow_always", "name": "Allow always", "kind": "allow_always"},
            {"optionId": "reject_once", "name": "Reject once", "kind": "reject_once"},
            {"optionId": "reject_always", "name": "Reject for this session", "kind": "reject_always"},
        ]}))
    }

    #[test]
    fn codex_decisions_map_onto_option_kinds() {
        let options = maki_options();
        assert_eq!(choose(&options, "reject_always"), Some("reject_always"));
        assert_eq!(choose(&options, "accept"), Some("allow_once"));
        assert_eq!(choose(&options, "acceptForSession"), Some("allow_always"));
        assert_eq!(choose(&options, "decline"), Some("reject_once"));
        assert_eq!(choose(&options, "something else"), None);
    }

    #[test]
    fn a_command_request_carries_its_command_and_cwd() {
        let params = json!({
            "toolCall": {
                "toolCallId": "call_1",
                "kind": "execute",
                "title": "bash: echo second",
                "rawInput": {"command": "echo second", "description": "Run requested command"},
            },
            "options": [{"optionId": "allow_once", "name": "Allow once", "kind": "allow_once"}],
        });
        let HarnessRequest::Permission {
            title,
            command,
            cwd,
            kind,
            description,
            options,
            ..
        } = request_for(&params, "bash", Vec::new(), "/work")
        else {
            panic!("expected a permission request");
        };
        assert_eq!(title, "bash: echo second");
        assert_eq!(command.as_deref(), Some("echo second"));
        assert_eq!(cwd.as_deref(), Some("/work"));
        assert_eq!(kind, ToolKind::Execute);
        assert_eq!(description.as_deref(), Some("Run requested command"));
        assert_eq!(options.len(), 1);
    }
}
