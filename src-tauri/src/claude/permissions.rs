//! `can_use_tool` in, a permission request out; an option id in, a
//! `PermissionResult` out. The frontend never sees Claude's own shapes.

use serde_json::{json, Value};

use super::tools;
use crate::harness::{HarnessRequest, PermissionOption};
use crate::util::json::{arr_or_empty, str_at, Json};

pub(crate) const ALLOW: &str = "allow";
pub(crate) const ALLOW_ALWAYS: &str = "allow_always";
pub(crate) const REJECT: &str = "reject";
pub(crate) const PLAN_IMPLEMENT: &str = "plan_implement";
pub(crate) const PLAN_IMPLEMENT_AUTO: &str = "plan_implement_auto";
pub(crate) const PLAN_REVISE: &str = "plan_revise";

/// Codex's decision words, accepted too so one approval card serves both.
fn normalise(option_id: &str) -> &str {
    match option_id {
        "accept" => ALLOW,
        "acceptForSession" => ALLOW_ALWAYS,
        "decline" => REJECT,
        other => other,
    }
}

fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for next in chars.by_ref() {
                    if next.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(ch);
    }
    out
}

fn describe_suggestion(suggestions: &[Value]) -> String {
    let first = suggestions.first();
    match first.and_then(|s| str_at(s, "type")) {
        Some("setMode") => format!(
            "Switch to {}",
            first.and_then(|s| str_at(s, "mode")).unwrap_or("mode")
        ),
        Some("addRules") | Some("replaceRules") => {
            let rules = first.map(|s| arr_or_empty(s, "rules")).unwrap_or_default();
            match rules.first().and_then(|rule| str_at(rule, "ruleContent")) {
                Some(rule) if rules.len() == 1 => format!("Always allow `{rule}`"),
                _ => "Always allow".to_string(),
            }
        }
        _ => "Always allow".to_string(),
    }
}

/// The neutral request for one `can_use_tool`.
pub(crate) fn request_for(can_use_tool: &Value, cwd: &str) -> HarnessRequest {
    let name = str_at(can_use_tool, "tool_name").unwrap_or("tool");
    let input = can_use_tool.get("input").cloned().unwrap_or(Value::Null);
    if name == "AskUserQuestion" {
        return HarnessRequest::UserInput {
            questions: Json(questions_for(&input)),
        };
    }
    let suggestions = arr_or_empty(can_use_tool, "permission_suggestions");
    let suppress_always = can_use_tool
        .get("suppress_always_allow_rule")
        .and_then(Value::as_bool)
        == Some(true);
    let options = if name == "ExitPlanMode" {
        vec![
            PermissionOption {
                option_id: PLAN_IMPLEMENT.into(),
                name: "Implement".into(),
                kind: "allow_once".into(),
            },
            PermissionOption {
                option_id: PLAN_IMPLEMENT_AUTO.into(),
                name: "Implement, auto-accept edits".into(),
                kind: "allow_once".into(),
            },
            PermissionOption {
                option_id: PLAN_REVISE.into(),
                name: "Keep planning".into(),
                kind: "reject_once".into(),
            },
        ]
    } else {
        let mut options = vec![PermissionOption {
            option_id: ALLOW.into(),
            name: "Allow".into(),
            kind: "allow_once".into(),
        }];
        if !suggestions.is_empty() && !suppress_always {
            options.push(PermissionOption {
                option_id: ALLOW_ALWAYS.into(),
                name: describe_suggestion(suggestions),
                kind: "allow_always".into(),
            });
        }
        options.push(PermissionOption {
            option_id: REJECT.into(),
            name: "Decline".into(),
            kind: "reject_once".into(),
        });
        options
    };
    let mut reason = str_at(can_use_tool, "decision_reason").map(strip_ansi);
    if let Some(blocked) = str_at(can_use_tool, "blocked_path") {
        let line = format!("Outside the allowed directories: {blocked}");
        reason = Some(match reason {
            Some(existing) => format!("{existing}\n{line}"),
            None => line,
        });
    }
    let content = tools::initial_content(name, &input, cwd);
    let changes = Json(Value::Array(crate::harness::project::file_changes(
        &content,
    )));
    HarnessRequest::Permission {
        command: (name == "Bash").then(|| str_at(&input, "command").unwrap_or("").to_string()),
        cwd: (name == "Bash").then(|| cwd.to_string()),
        changes,
        title: str_at(can_use_tool, "title")
            .filter(|title| !title.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| tools::title_for(name, &input)),
        description: str_at(can_use_tool, "description").map(str::to_string),
        kind: tools::kind_for(name),
        name: name.to_string(),
        content,
        options,
        reason,
        default_to_reject: can_use_tool.get("default_to_no").and_then(Value::as_bool) == Some(true),
    }
}

/// Claude's `AskUserQuestion` input as Codex-shaped questions, so the
/// existing question card draws them. Ids are the question index.
pub(crate) fn questions_for(input: &Value) -> Value {
    let questions: Vec<Value> = arr_or_empty(input, "questions")
        .iter()
        .enumerate()
        .map(|(index, question)| {
            json!({
                "id": index.to_string(),
                "header": str_at(question, "header"),
                "question": str_at(question, "question").unwrap_or(""),
                "isSecret": false,
                "options": arr_or_empty(question, "options").iter().map(|option| json!({
                    "label": str_at(option, "label").unwrap_or(""),
                    "description": str_at(option, "description"),
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    Value::Array(questions)
}

/// Codex-shaped answers (`{id: {answers: [..]}}`) back into what Claude's
/// AskUserQuestion expects: `updatedInput.answers` keyed by question text.
pub(crate) fn user_input_result(input: &Value, tool_use_id: &str, answers: &Value) -> Value {
    let mut by_text = serde_json::Map::new();
    for (index, question) in arr_or_empty(input, "questions").iter().enumerate() {
        let text = str_at(question, "question").unwrap_or("").to_string();
        let given = answers
            .get(index.to_string())
            .and_then(|entry| entry.get("answers"))
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        by_text.insert(text, json!(given));
    }
    let mut updated = input.clone();
    if let Some(object) = updated.as_object_mut() {
        object.insert("answers".into(), Value::Object(by_text));
    }
    json!({
        "behavior": "allow",
        "updatedInput": updated,
        "toolUseID": tool_use_id,
    })
}

/// The `PermissionResult` for a chosen option.
pub fn permission_result(option_id: &str, can_use_tool: &Value) -> Value {
    let tool_use_id = str_at(can_use_tool, "tool_use_id").unwrap_or("");
    let input = can_use_tool.get("input").cloned().unwrap_or(Value::Null);
    let deny = |message: &str, classification: &str| {
        json!({
            "behavior": "deny",
            "message": message,
            "interrupt": false,
            "toolUseID": tool_use_id,
            "decisionClassification": classification,
        })
    };
    let allow = |updated_permissions: Option<Value>, classification: &str| {
        let mut result = json!({
            "behavior": "allow",
            "updatedInput": input,
            "toolUseID": tool_use_id,
            "decisionClassification": classification,
        });
        if let Some(permissions) = updated_permissions {
            result["updatedPermissions"] = permissions;
        }
        result
    };
    match normalise(option_id) {
        ALLOW => allow(None, "user_temporary"),
        ALLOW_ALWAYS => allow(
            can_use_tool.get("permission_suggestions").cloned(),
            "user_permanent",
        ),
        PLAN_IMPLEMENT => allow(
            Some(json!([{"type": "setMode", "mode": "default", "destination": "session"}])),
            "user_temporary",
        ),
        PLAN_IMPLEMENT_AUTO => allow(
            Some(json!([{"type": "setMode", "mode": "acceptEdits", "destination": "session"}])),
            "user_temporary",
        ),
        PLAN_REVISE => deny("Revise the plan", "user_reject"),
        _ => deny("User declined", "user_reject"),
    }
}

/// The deny sent for every prompt still open when a turn is interrupted.
pub(crate) fn interrupted_result(can_use_tool: &Value) -> Value {
    json!({
        "behavior": "deny",
        "message": "Interrupted",
        "interrupt": true,
        "toolUseID": str_at(can_use_tool, "tool_use_id").unwrap_or(""),
        "decisionClassification": "user_reject",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn always_allow_appears_only_with_suggestions() {
        let bare = request_for(
            &json!({"tool_name": "Bash", "input": {"command": "ls"}, "tool_use_id": "t"}),
            "/r",
        );
        let HarnessRequest::Permission { options, .. } = bare else {
            panic!()
        };
        assert_eq!(
            options
                .iter()
                .map(|o| o.option_id.as_str())
                .collect::<Vec<_>>(),
            vec![ALLOW, REJECT]
        );

        let suggested = request_for(
            &json!({"tool_name": "Bash", "input": {"command": "ls"}, "tool_use_id": "t",
                "permission_suggestions": [{"type": "addRules", "rules": [{"toolName": "Bash", "ruleContent": "ls *"}], "behavior": "allow", "destination": "session"}]}),
            "/r",
        );
        let HarnessRequest::Permission { options, .. } = suggested else {
            panic!()
        };
        assert_eq!(options[1].name, "Always allow `ls *`");
    }

    #[test]
    fn always_allow_returns_the_suggestions_verbatim() {
        let request = json!({"tool_name": "Bash", "input": {"command": "ls"}, "tool_use_id": "t",
            "permission_suggestions": [{"type": "setMode", "mode": "acceptEdits", "destination": "session"}]});
        let result = permission_result(ALLOW_ALWAYS, &request);
        assert_eq!(result["behavior"], "allow");
        assert_eq!(
            result["updatedPermissions"],
            request["permission_suggestions"]
        );
        assert_eq!(result["toolUseID"], "t");
    }

    #[test]
    fn codex_decision_words_are_accepted() {
        let request = json!({"tool_name": "Bash", "input": {}, "tool_use_id": "t"});
        assert_eq!(permission_result("decline", &request)["behavior"], "deny");
        assert_eq!(permission_result("accept", &request)["behavior"], "allow");
    }

    #[test]
    fn questions_round_trip_keyed_by_text() {
        let input = json!({"questions": [{"question": "Which?", "header": "Pick", "options": [{"label": "A"}, {"label": "B"}]}]});
        let questions = questions_for(&input);
        assert_eq!(questions[0]["id"], "0");
        let result = user_input_result(&input, "t", &json!({"0": {"answers": ["A", "B"]}}));
        assert_eq!(result["updatedInput"]["answers"]["Which?"], "A, B");
    }

    #[test]
    fn decision_reasons_lose_their_ansi() {
        assert_eq!(strip_ansi("\u{1b}[31mno\u{1b}[0m"), "no");
    }
}
