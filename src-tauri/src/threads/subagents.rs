//! The agents a thread has spawned.
//!
//! The descendant listing gives identity but not the model or effort each child
//! was spawned with — that only appears in the parent's `spawnAgent` tool call.
//! So the parents are read too, and the two views are joined per child.

use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use tauri::{AppHandle, State};

use crate::projects::thread_summary_from;
use crate::util::json::{arr_or_empty, str_at};
use crate::AppState;

/// Upper bound on descendants fetched for one thread.
const MAX_DESCENDANTS: usize = 1000;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SubagentDetail {
    id: String,
    parent_thread_id: String,
    title: String,
    cwd: String,
    status: String,
    agent_nickname: Option<String>,
    agent_role: Option<String>,
    model: Option<String>,
    reasoning_effort: Option<String>,
}

/// What a parent thread recorded when it spawned a child.
#[derive(Clone, Debug, PartialEq, Eq)]
struct SpawnDetail {
    model: Option<String>,
    reasoning_effort: Option<String>,
    status: String,
}

/// Scan a parent thread's turns for `spawnAgent` calls, recording the model,
/// effort and live state it holds for each child it spawned.
fn collect_spawn_details(thread: &Value, details: &mut HashMap<String, SpawnDetail>) {
    let Some(turns) = thread.get("turns").and_then(Value::as_array) else {
        return;
    };
    for item in turns
        .iter()
        .filter_map(|turn| turn.get("items").and_then(Value::as_array))
        .flatten()
    {
        if str_at(item, "type") != Some("collabAgentToolCall")
            || str_at(item, "tool") != Some("spawnAgent")
        {
            continue;
        }
        let model = str_at(item, "model").map(str::to_string);
        let reasoning_effort = str_at(item, "reasoningEffort").map(str::to_string);
        for child_id in arr_or_empty(item, "receiverThreadIds")
            .iter()
            .filter_map(Value::as_str)
        {
            let status = item
                .get("agentsStates")
                .and_then(|states| states.get(child_id))
                .and_then(|state| str_at(state, "status"))
                .unwrap_or_default()
                .to_string();
            details.insert(
                child_id.to_string(),
                SpawnDetail {
                    model: model.clone(),
                    reasoning_effort: reasoning_effort.clone(),
                    status,
                },
            );
        }
    }
}

#[tauri::command]
pub(crate) async fn list_subagents(
    thread_id: String,
    app: AppHandle,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Vec<SubagentDetail>, String> {
    let ctx = state.ctx(&window);
    let response = ctx
        .session
        .request(
            &app,
            "thread/list",
            json!({
                "limit": MAX_DESCENDANTS,
                "sortKey": "created_at",
                "sortDirection": "asc",
                "archived": false,
                "ancestorThreadId": thread_id,
            }),
        )
        .await?;
    let descendants = arr_or_empty(&response, "data").to_vec();
    // Resuming each descendant subscribes the app to its live updates.
    for descendant_id in descendants.iter().filter_map(|thread| str_at(thread, "id")) {
        let _ = ctx.session.ensure_resumed(&app, descendant_id).await;
    }

    // The spawn details live on the parents, so every distinct parent (plus the
    // thread itself) has to be read to resolve them.
    let mut parent_ids: HashSet<String> = descendants
        .iter()
        .filter_map(|thread| str_at(thread, "parentThreadId"))
        .map(str::to_string)
        .collect();
    parent_ids.insert(thread_id);

    let mut spawn_details = HashMap::new();
    for parent_id in parent_ids {
        let _ = ctx.session.ensure_resumed(&app, &parent_id).await;
        if let Ok(response) = ctx
            .session
            .request(
                &app,
                "thread/read",
                json!({"threadId": parent_id, "includeTurns": true}),
            )
            .await
        {
            if let Some(thread) = response.get("thread") {
                collect_spawn_details(thread, &mut spawn_details);
            }
        }
    }

    Ok(descendants
        .iter()
        .filter_map(|thread| {
            let summary = thread_summary_from(thread, &HashSet::new())?;
            let parent_thread_id = summary.parent_thread_id?;
            let spawn = spawn_details.remove(&summary.id).unwrap_or(SpawnDetail {
                model: None,
                reasoning_effort: None,
                status: String::new(),
            });
            Some(SubagentDetail {
                id: summary.id,
                parent_thread_id,
                title: summary.title,
                cwd: summary.cwd,
                // The parent's live view of the child wins over the child's own
                // stored status, which lags behind.
                status: if spawn.status.is_empty() {
                    summary.status
                } else {
                    spawn.status
                },
                agent_nickname: summary.agent_nickname,
                agent_role: summary.agent_role,
                model: spawn.model,
                reasoning_effort: spawn.reasoning_effort,
            })
        })
        .collect())
}

#[tauri::command]
pub(crate) async fn update_subagent_policy(
    thread_id: String,
    model_policy: Value,
    reasoning_effort_policy: Value,
    app: AppHandle,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let ctx = state.ctx(&window);
    ctx.session.ensure_resumed(&app, &thread_id).await?;
    ctx
        .session
        .request(
            &app,
            "thread/settings/update",
            json!({
                "threadId": thread_id,
                "subagentModelPolicy": model_policy,
                "subagentReasoningEffortPolicy": reasoning_effort_policy,
            }),
        )
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_resolved_spawn_model_effort_and_state() {
        let thread = json!({
            "turns": [{
                "items": [{
                    "type": "collabAgentToolCall",
                    "tool": "spawnAgent",
                    "receiverThreadIds": ["child"],
                    "model": "gpt-5.6-terra",
                    "reasoningEffort": "high",
                    "agentsStates": {"child": {"status": "running"}}
                }]
            }]
        });
        let mut details = HashMap::new();
        collect_spawn_details(&thread, &mut details);
        assert_eq!(
            details.get("child"),
            Some(&SpawnDetail {
                model: Some("gpt-5.6-terra".into()),
                reasoning_effort: Some("high".into()),
                status: "running".into(),
            })
        );
    }

    #[test]
    fn ignores_tool_calls_that_are_not_spawns() {
        let thread = json!({
            "turns": [{
                "items": [
                    {"type": "collabAgentToolCall", "tool": "sendMessage", "receiverThreadIds": ["a"]},
                    {"type": "shellToolCall", "tool": "spawnAgent", "receiverThreadIds": ["b"]},
                ]
            }]
        });
        let mut details = HashMap::new();
        collect_spawn_details(&thread, &mut details);
        assert!(details.is_empty());
    }

    #[test]
    fn a_thread_with_no_turns_yields_no_spawn_details() {
        let mut details = HashMap::new();
        collect_spawn_details(&json!({"id": "t1"}), &mut details);
        assert!(details.is_empty());
    }
}
