//! Reading one thread for the transcript view.
//!
//! Served from the local cache when it matches the app-server's `updated_at`,
//! otherwise fetched and cached. Either way two sets of locally persisted items
//! are merged back in, because Codex's projection returns neither: the
//! `request_user_input` questions (it has no item type for them) and the
//! journaled stream items (it drops command executions entirely, and has not
//! persisted anything from a turn that is still running).

use serde_json::{json, Value};
use tauri::{AppHandle, State};

use crate::codex::requests;
use crate::storage::{self, JournaledItem, TurnSettings, UserInputAnswer};
use crate::util::json::str_at;
use crate::util::json::Json;
use crate::AppState;

/// Thread settings that are resolved at resume time rather than stored on the
/// thread, so they must be overlaid onto a cached read.
const RESOLVED_SETTINGS: [&str; 2] = ["subagentModelPolicy", "subagentReasoningEffortPolicy"];

#[tauri::command]
#[specta::specta]
pub(crate) async fn read_thread(
    thread_id: String,
    app: AppHandle,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Json, String> {
    let ctx = state.ctx(&window);
    if let Some(thread) = storage::thread_harness(&ctx.database(), &thread_id).await? {
        return read_harness_thread(&ctx, &thread).await.map(Json);
    }
    // Resuming subscribes the app to live updates. Keep this best-effort so a
    // cached thread remains readable while Codex is unavailable.
    let resume = ctx.session.ensure_resumed(&app, &thread_id).await.ok();
    let source_updated_at = storage::thread_updated_at(&ctx.database(), &thread_id)
        .await?
        .unwrap_or_default();
    if let Some(mut detail) =
        storage::read_thread_detail(&ctx.database(), &thread_id, source_updated_at).await?
    {
        merge_local_items(&ctx, &thread_id, &mut detail).await?;
        return Ok(Json(with_thread_settings(detail, resume.as_ref())));
    }
    let response = read_when_rollout_settles(&ctx, &app, &thread_id).await?;
    let mut detail = response
        .get("thread")
        .cloned()
        .ok_or_else(|| "Codex returned no thread data".to_string())?;
    // Cached before the merge: the row holds Codex's own payload, and the local
    // items are layered on at read time so they cannot go stale inside it.
    storage::write_thread_detail(&ctx.database(), &thread_id, source_updated_at, &detail).await?;
    merge_local_items(&ctx, &thread_id, &mut detail).await?;
    Ok(Json(with_thread_settings(detail, resume.as_ref())))
}

/// A thread on another harness has no projection to fetch: the journal is
/// its transcript. Turns come back in the order their items were recorded,
/// with any turn still open on the live process marked as running.
async fn read_harness_thread(
    ctx: &crate::HomeContext,
    thread: &storage::HarnessThread,
) -> Result<Value, String> {
    let database = ctx.database();
    let items = storage::read_thread_items(&database, &thread.thread_id).await?;
    let complete = storage::read_complete_turns(&database, &thread.thread_id).await?;
    let running = storage::read_running_turns(&database, &thread.thread_id).await?;
    let mut order: Vec<String> = Vec::new();
    for item in &items {
        if !order.contains(&item.turn_id) {
            order.push(item.turn_id.clone());
        }
    }
    for turn_id in complete.iter().chain(running.iter()) {
        if !order.contains(turn_id) {
            order.push(turn_id.clone());
        }
    }
    let turns: Vec<Value> = order
        .iter()
        .map(|turn_id| {
            let status = if running.contains(turn_id) {
                "inProgress"
            } else {
                "completed"
            };
            json!({
                "id": turn_id,
                "status": status,
                "items": items
                    .iter()
                    .filter(|item| &item.turn_id == turn_id)
                    .map(|item| item.payload.clone())
                    .collect::<Vec<_>>(),
            })
        })
        .collect();
    let mut detail = json!({
        "id": thread.thread_id,
        "preview": thread.title,
        "name": thread.title,
        "cwd": thread.cwd,
        "harness": thread.harness,
        "turns": turns,
    });
    let answers = storage::read_user_input_answers(&database, &thread.thread_id).await?;
    merge_user_input_answers(&mut detail, &answers);
    let settings = storage::read_turn_settings(&database, &thread.thread_id).await?;
    merge_turn_settings(&mut detail, &settings);
    Ok(detail)
}

/// Codex creates a thread's rollout file lazily, and only writes its meta line
/// after collecting git info, so a read racing the first turn of a brand-new
/// thread can find the file empty and fail with "rollout at … is empty". That
/// state clears itself within moments — retry it instead of surfacing it.
async fn read_when_rollout_settles(
    ctx: &crate::HomeContext,
    app: &AppHandle,
    thread_id: &str,
) -> Result<Value, String> {
    const ATTEMPTS: u32 = 6;
    const BACKOFF: std::time::Duration = std::time::Duration::from_millis(250);
    let mut attempt = 1;
    loop {
        match ctx
            .session
            .send(app, requests::thread_read(thread_id))
            .await
        {
            Err(error) if is_empty_rollout(&error) && attempt < ATTEMPTS => {
                attempt += 1;
                tokio::time::sleep(BACKOFF).await;
            }
            result => return result,
        }
    }
}

/// The thread-store's wording for a rollout file that exists but has no
/// session meta line yet.
fn is_empty_rollout(error: &str) -> bool {
    error.contains("is empty")
}

/// Layer everything Pingex persisted itself onto Codex's payload.
async fn merge_local_items(
    ctx: &crate::HomeContext,
    thread_id: &str,
    detail: &mut Value,
) -> Result<(), String> {
    let items = storage::read_thread_items(&ctx.database(), thread_id).await?;
    // Before the merge: a turn that is still running may not be in Codex's
    // payload at all, and journaled items whose turn is missing are dropped.
    let running = storage::read_running_turns(&ctx.database(), thread_id).await?;
    mark_running_turns(detail, &running);
    let complete = storage::read_complete_turns(&ctx.database(), thread_id).await?;
    merge_journaled_items(detail, &items, &complete);
    let answers = storage::read_user_input_answers(&ctx.database(), thread_id).await?;
    merge_user_input_answers(detail, &answers);
    let settings = storage::read_turn_settings(&ctx.database(), thread_id).await?;
    merge_turn_settings(detail, &settings);
    Ok(())
}

/// Stamp each turn with what it ran on. Codex's payload wins if it ever starts
/// reporting this itself; turns from before it was recorded stay unlabelled.
fn merge_turn_settings(detail: &mut Value, settings: &[TurnSettings]) {
    let Some(turns) = detail.get_mut("turns").and_then(Value::as_array_mut) else {
        return;
    };
    for entry in settings {
        let Some(turn) = turns
            .iter_mut()
            .find(|turn| str_at(turn, "id") == Some(&entry.turn_id))
            .and_then(Value::as_object_mut)
        else {
            continue;
        };
        if let Some(model) = &entry.model {
            turn.entry("model").or_insert_with(|| json!(model));
        }
        if let Some(effort) = &entry.reasoning_effort {
            turn.entry("reasoningEffort")
                .or_insert_with(|| json!(effort));
        }
    }
}

/// Say which turns are still running.
///
/// Codex's projection never reports a turn as in progress — it describes what
/// has been persisted, not what is happening — so a thread opened for the first
/// time while it works renders as finished: no typing indicator, and the work
/// that is streaming right now folded away behind "Worked". That is most
/// visible on a subagent's thread, which is almost always opened mid-run.
///
/// A turn this process watched start and never saw finish is exactly the set to
/// stamp. A turn that is not in the payload yet is added rather than skipped:
/// nothing of a running turn need have been persisted, and the journal's own
/// items for it are dropped by `insert_item` if there is no turn to hold them.
///
/// A stale row left by a process that died still reads as running here. The
/// transcript resolves that on its side, demoting an `inProgress` turn on a
/// thread with no live stream to `interrupted`.
fn mark_running_turns(detail: &mut Value, running: &[String]) {
    let Some(turns) = detail.get_mut("turns").and_then(Value::as_array_mut) else {
        return;
    };
    for turn_id in running {
        match turns
            .iter_mut()
            .find(|turn| str_at(turn, "id") == Some(turn_id))
            .and_then(Value::as_object_mut)
        {
            Some(turn) => {
                turn.insert("status".into(), json!("inProgress"));
            }
            // Appended, not inserted: a turn Codex has not persisted has by
            // definition only just started, so it belongs last.
            None => turns.push(json!({
                "id": turn_id,
                "status": "inProgress",
                "items": [],
            })),
        }
    }
}

/// Reconcile the journal with Codex's projection.
///
/// A turn the app watched from `turn/started` to `turn/completed` is journaled
/// in full, and the journal is the only record that is in true stream order, so
/// it replaces the projection's items outright. That matters because the
/// projection both drops work (command executions never come back) and hands
/// items back under fresh ids (`item-1`, `item-2`, …) — an agent message merged
/// into it would otherwise appear twice, and a plan would sit wherever the
/// rollout happened to materialise it rather than where it streamed.
///
/// Every other turn — older threads, or one whose start this process missed —
/// keeps the anchor-based merge, where an item Codex returned wins and the
/// journal only fills gaps.
fn merge_journaled_items(detail: &mut Value, items: &[JournaledItem], complete_turns: &[String]) {
    let replayed = replay_complete_turns(detail, items, complete_turns);
    for item in items {
        if replayed.iter().any(|turn_id| turn_id == &item.turn_id) {
            continue;
        }
        insert_item(
            detail,
            &item.turn_id,
            &item.item_id,
            item.after_item_id.as_deref(),
            item.payload.clone(),
        );
    }
}

/// Swap each fully journaled turn's items for the journal's own, in the order
/// they streamed. Returns the turns that were replaced. A turn with no
/// journaled rows is left alone rather than emptied.
fn replay_complete_turns(
    detail: &mut Value,
    items: &[JournaledItem],
    complete_turns: &[String],
) -> Vec<String> {
    let Some(turns) = detail.get_mut("turns").and_then(Value::as_array_mut) else {
        return Vec::new();
    };
    let mut replayed = Vec::new();
    for turn_id in complete_turns {
        let journaled: Vec<Value> = items
            .iter()
            .filter(|item| &item.turn_id == turn_id)
            .map(|item| item.payload.clone())
            .collect();
        if journaled.is_empty() {
            continue;
        }
        let Some(turn) = turns
            .iter_mut()
            .find(|turn| str_at(turn, "id") == Some(turn_id))
            .and_then(Value::as_object_mut)
        else {
            continue;
        };
        turn.insert("items".into(), Value::Array(journaled));
        replayed.push(turn_id.clone());
    }
    replayed
}

/// Inserts persisted questions into their turns. Questions the user never
/// answered are flagged so the transcript can offer to re-ask them.
fn merge_user_input_answers(detail: &mut Value, answers: &[UserInputAnswer]) {
    for answer in answers {
        let mut payload = answer.payload.clone();
        if !answer.answered {
            if let Some(object) = payload.as_object_mut() {
                object.insert("unanswered".into(), Value::Bool(true));
            }
        }
        insert_item(
            detail,
            &answer.turn_id,
            &answer.item_id,
            answer.after_item_id.as_deref(),
            payload,
        );
    }
}

/// Place one locally held item in its turn, right after the sibling it was
/// captured next to in the real stream order (`after_item_id`). Item ids are
/// opaque tool-call ids, not a sortable sequence, so there is no way to derive
/// position from the id itself — when no anchor is known (or its sibling is
/// not in this turn), the item falls back to the end of the turn. An item id
/// Codex already returned is left alone, and an item whose turn is not in the
/// payload is dropped rather than invented.
fn insert_item(
    detail: &mut Value,
    turn_id: &str,
    item_id: &str,
    after_item_id: Option<&str>,
    payload: Value,
) {
    let Some(turns) = detail.get_mut("turns").and_then(Value::as_array_mut) else {
        return;
    };
    let Some(items) = turns
        .iter_mut()
        .find(|turn| str_at(turn, "id") == Some(turn_id))
        .and_then(|turn| turn.get_mut("items"))
        .and_then(Value::as_array_mut)
    else {
        return;
    };
    if items.iter().any(|item| str_at(item, "id") == Some(item_id)) {
        return;
    }
    let position = after_item_id
        .and_then(|anchor| {
            items
                .iter()
                .position(|item| str_at(item, "id") == Some(anchor))
        })
        .map(|index| index + 1)
        .unwrap_or(items.len());
    items.insert(position, payload);
}

/// Overlay the settings the resume response resolved onto the thread payload,
/// so a cached read does not report stale subagent policies.
fn with_thread_settings(mut detail: Value, resume: Option<&Value>) -> Value {
    let Some(object) = detail.as_object_mut() else {
        return detail;
    };
    for key in RESOLVED_SETTINGS {
        if let Some(value) = resume.and_then(|response| response.get(key)) {
            object.insert(key.to_string(), value.clone());
        }
    }
    detail
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answer(turn_id: &str, item_id: &str, after_item_id: Option<&str>) -> UserInputAnswer {
        UserInputAnswer {
            turn_id: turn_id.into(),
            item_id: item_id.into(),
            payload: json!({"type": "userInputAnswered", "id": item_id}),
            answered: true,
            after_item_id: after_item_id.map(Into::into),
        }
    }

    fn unanswered(turn_id: &str, item_id: &str, after_item_id: Option<&str>) -> UserInputAnswer {
        UserInputAnswer {
            answered: false,
            ..answer(turn_id, item_id, after_item_id)
        }
    }

    #[test]
    fn merges_answers_after_their_anchor_without_duplicates() {
        let mut detail = json!({
            "turns": [{
                "id": "turn-1",
                "items": [{"id": "item_1"}, {"id": "item_4"}]
            }]
        });
        merge_user_input_answers(
            &mut detail,
            &[
                answer("turn-1", "item_2", Some("item_1")),
                answer("turn-1", "item_1", None),
                answer("turn-2", "item_9", None),
            ],
        );
        let ids: Vec<&str> = detail["turns"][0]["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["id"].as_str().unwrap())
            .collect();
        // item_1 already present (skipped), item_2 anchored right after it,
        // turn-2 answer dropped (no such turn).
        assert_eq!(ids, vec!["item_1", "item_2", "item_4"]);
    }

    #[test]
    fn flags_questions_the_user_never_answered() {
        let mut detail = json!({"turns": [{"id": "turn-1", "items": [{"id": "item_1"}]}]});
        merge_user_input_answers(
            &mut detail,
            &[
                unanswered("turn-1", "item_2", Some("item_1")),
                answer("turn-1", "item_3", Some("item_2")),
            ],
        );
        let items = detail["turns"][0]["items"].as_array().unwrap();
        assert_eq!(items[1]["unanswered"], json!(true));
        assert_eq!(items[2].get("unanswered"), None);
    }

    #[test]
    fn appends_answers_with_no_anchor_or_an_anchor_not_in_the_turn() {
        let mut detail = json!({"turns": [{"id": "turn-1", "items": [{"id": "item_1"}]}]});
        merge_user_input_answers(
            &mut detail,
            &[
                answer("turn-1", "call_missing", None),
                answer("turn-1", "call_stale", Some("call_gone")),
            ],
        );
        let ids: Vec<&str> = detail["turns"][0]["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, vec!["item_1", "call_missing", "call_stale"]);
    }

    #[test]
    fn a_turn_still_running_is_reported_as_in_progress() {
        // Codex describes what it persisted, so it reports the turn it is
        // partway through as if it had ended.
        let mut detail = json!({
            "turns": [
                {"id": "turn-1", "status": "completed", "items": []},
                {"id": "turn-2", "status": "completed", "items": []},
            ]
        });
        mark_running_turns(&mut detail, &["turn-2".into()]);
        assert_eq!(detail["turns"][0]["status"], json!("completed"));
        assert_eq!(detail["turns"][1]["status"], json!("inProgress"));
    }

    #[test]
    fn a_running_turn_missing_from_the_projection_is_added_with_its_work() {
        // A subagent's thread is opened seconds after it starts, when Codex has
        // persisted nothing of the turn. Without the turn there is nowhere for
        // the journal's items to go, and the transcript sits empty and idle.
        let mut detail = json!({"turns": [{"id": "turn-1", "status": "completed", "items": []}]});
        mark_running_turns(&mut detail, &["turn-2".into()]);
        merge_journaled_items(&mut detail, &[journaled("turn-2", "item_1", None)], &[]);

        let turns = detail["turns"].as_array().unwrap();
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[1]["id"], json!("turn-2"));
        assert_eq!(turns[1]["status"], json!("inProgress"));
        assert_eq!(turns[1]["items"][0]["id"], json!("item_1"));
    }

    fn journaled(turn_id: &str, item_id: &str, after_item_id: Option<&str>) -> JournaledItem {
        JournaledItem {
            turn_id: turn_id.into(),
            item_id: item_id.into(),
            payload: json!({"type": "commandExecution", "id": item_id, "command": "cargo test"}),
            after_item_id: after_item_id.map(Into::into),
        }
    }

    #[test]
    fn fills_in_the_command_executions_the_projection_drops() {
        // Codex returns the conversation; the shell work only exists locally.
        let mut detail = json!({
            "turns": [{
                "id": "turn-1",
                "items": [{"id": "item_1", "type": "userMessage"}, {"id": "item_5", "type": "agentMessage"}]
            }]
        });
        // Passed in true stream order, as storage::read_thread_items returns
        // them (sorted by recorded_at) — item_3's anchor must already be in
        // the list by the time it is processed.
        merge_journaled_items(
            &mut detail,
            &[
                journaled("turn-1", "item_2", Some("item_1")),
                journaled("turn-1", "item_3", Some("item_2")),
            ],
            &[],
        );
        let ids: Vec<&str> = detail["turns"][0]["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["id"].as_str().unwrap())
            .collect();
        // item_2 anchors after item_1, item_3 then chains off item_2 even
        // though item_2 was only just inserted this same pass.
        assert_eq!(ids, vec!["item_1", "item_2", "item_3", "item_5"]);
    }

    #[test]
    fn codex_keeps_its_own_copy_of_an_item_the_journal_also_has() {
        let mut detail = json!({
            "turns": [{
                "id": "turn-1",
                "items": [{"id": "item_2", "type": "commandExecution", "exitCode": 0}]
            }]
        });
        merge_journaled_items(
            &mut detail,
            &[journaled("turn-1", "item_2", Some("item_1"))],
            &[],
        );
        let items = detail["turns"][0]["items"].as_array().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["exitCode"], json!(0));
    }

    #[test]
    fn journaled_items_for_a_turn_the_thread_no_longer_has_are_dropped() {
        let mut detail = json!({"turns": [{"id": "turn-1", "items": []}]});
        merge_journaled_items(&mut detail, &[journaled("turn-9", "item_1", None)], &[]);
        assert!(detail["turns"][0]["items"].as_array().unwrap().is_empty());
    }

    fn journaled_payload(turn_id: &str, item_id: &str, payload: Value) -> JournaledItem {
        JournaledItem {
            turn_id: turn_id.into(),
            item_id: item_id.into(),
            payload,
            after_item_id: None,
        }
    }

    #[test]
    fn a_fully_journaled_turn_replays_in_the_order_it_streamed() {
        // Codex hands the same agent message back under a fresh id and puts
        // the plan last; the journal knows where both really belong.
        let mut detail = json!({
            "turns": [{
                "id": "turn-1",
                "items": [
                    {"id": "item-1", "type": "userMessage"},
                    {"id": "item-2", "type": "agentMessage", "text": "On it"},
                    {"id": "turn-1-plan", "type": "plan", "text": "1. Look"}
                ]
            }]
        });
        merge_journaled_items(
            &mut detail,
            &[
                journaled_payload(
                    "turn-1",
                    "msg_a",
                    json!({"id": "msg_a", "type": "userMessage"}),
                ),
                journaled_payload(
                    "turn-1",
                    "turn-1-plan",
                    json!({"id": "turn-1-plan", "type": "plan", "text": "1. Look"}),
                ),
                journaled("turn-1", "exec_1", None),
                journaled_payload(
                    "turn-1",
                    "msg_b",
                    json!({"id": "msg_b", "type": "agentMessage", "text": "On it"}),
                ),
            ],
            &["turn-1".to_string()],
        );
        let ids: Vec<&str> = detail["turns"][0]["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["id"].as_str().unwrap())
            .collect();
        // The plan sits where it streamed, and the message appears once.
        assert_eq!(ids, vec!["msg_a", "turn-1-plan", "exec_1", "msg_b"]);
    }

    #[test]
    fn a_turn_marked_complete_with_nothing_journaled_keeps_codexs_items() {
        let mut detail = json!({
            "turns": [{"id": "turn-1", "items": [{"id": "item-1", "type": "userMessage"}]}]
        });
        merge_journaled_items(&mut detail, &[], &["turn-1".to_string()]);
        assert_eq!(detail["turns"][0]["items"][0]["id"], json!("item-1"));
    }

    #[test]
    fn turns_the_app_did_not_watch_from_the_start_still_take_the_anchor_merge() {
        let mut detail = json!({
            "turns": [
                {"id": "turn-1", "items": [{"id": "item_1", "type": "userMessage"}]},
                {"id": "turn-2", "items": [{"id": "item_9", "type": "userMessage"}]}
            ]
        });
        merge_journaled_items(
            &mut detail,
            &[
                journaled_payload("turn-1", "exec_1", json!({"id": "exec_1"})),
                journaled("turn-2", "exec_2", Some("item_9")),
            ],
            &["turn-1".to_string()],
        );
        assert_eq!(detail["turns"][0]["items"][0]["id"], json!("exec_1"));
        let second: Vec<&str> = detail["turns"][1]["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["id"].as_str().unwrap())
            .collect();
        assert_eq!(second, vec!["item_9", "exec_2"]);
    }

    #[test]
    fn a_thread_with_no_turns_is_left_alone() {
        let mut detail = json!({"id": "thread-1"});
        merge_user_input_answers(&mut detail, &[answer("turn-1", "item_1", None)]);
        assert_eq!(detail, json!({"id": "thread-1"}));
    }

    fn ran_with(turn_id: &str, model: Option<&str>, effort: Option<&str>) -> TurnSettings {
        TurnSettings {
            turn_id: turn_id.into(),
            model: model.map(Into::into),
            reasoning_effort: effort.map(Into::into),
        }
    }

    #[test]
    fn stamps_turns_with_what_they_ran_on() {
        let mut detail = json!({"turns": [{"id": "turn-1"}, {"id": "turn-2"}]});
        merge_turn_settings(
            &mut detail,
            &[
                ran_with("turn-1", Some("gpt-5.2"), Some("high")),
                ran_with("turn-2", Some("gpt-5.6-terra"), None),
                ran_with("turn-9", Some("gpt-5.2"), None),
            ],
        );
        assert_eq!(detail["turns"][0]["model"], "gpt-5.2");
        assert_eq!(detail["turns"][0]["reasoningEffort"], "high");
        assert_eq!(detail["turns"][1]["model"], "gpt-5.6-terra");
        assert_eq!(detail["turns"][1].get("reasoningEffort"), None);
    }

    #[test]
    fn codex_wins_when_it_reports_a_turns_model_itself() {
        let mut detail = json!({"turns": [{"id": "turn-1", "model": "from-codex"}]});
        merge_turn_settings(&mut detail, &[ran_with("turn-1", Some("gpt-5.2"), None)]);
        assert_eq!(detail["turns"][0]["model"], "from-codex");
    }

    #[test]
    fn overlays_only_resolved_thread_settings() {
        let detail = json!({"id": "thread-1", "subagentModelPolicy": {"mode": "old"}});
        let resume = json!({
            "subagentModelPolicy": {"mode": "new"},
            "unrelated": true
        });
        assert_eq!(
            with_thread_settings(detail, Some(&resume)),
            json!({"id": "thread-1", "subagentModelPolicy": {"mode": "new"}})
        );
    }

    #[test]
    fn without_a_resume_response_settings_are_untouched() {
        let detail = json!({"id": "thread-1", "subagentModelPolicy": {"mode": "old"}});
        assert_eq!(with_thread_settings(detail.clone(), None), detail);
    }
}
