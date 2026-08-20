//! Persisting the parts of a turn that `thread/read` will not hand back.
//!
//! Codex's re-read projection returns the conversation but drops the work — a
//! thread that ran twenty commands re-reads as if it had run none — and it
//! renumbers what it does return (`item-1`, `item-2`, …) rather than reusing
//! the ids it streamed, so its copies cannot even be matched up with ours.
//!
//! So every completed item is written to the local journal as it streams past,
//! anchored to the item it followed. A turn the app watched from `turn/started`
//! to `turn/completed` is journaled in full, and `threads::read` then replays it
//! verbatim instead of splicing into the projection.

use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::AppHandle;

/// How much aggregated command output to keep. A single build can emit
/// megabytes, and the transcript only ever shows the tail of it.
pub(crate) const MAX_OUTPUT_BYTES: usize = 16 * 1024;

/// How much of a single file's diff to keep. The transcript folds anything
/// this large anyway, and a generated file can carry a diff far bigger.
pub(crate) const MAX_DIFF_BYTES: usize = 64 * 1024;

/// What an `item/completed` notification contributes to the journal.
pub(crate) struct JournalTarget {
    pub(crate) thread_id: String,
    pub(crate) turn_id: String,
    pub(crate) item_id: String,
    pub(crate) payload: Value,
}

/// The `(turn_id, item_id)` an `item/completed` notification carries, if any
/// — extracted independently of whether the item carries the ids a journal row
/// needs, since an item that cannot be journaled still occupies a slot in the
/// turn's real stream order and later items must be able to anchor after it.
pub(crate) fn stream_item_id(params: &Value) -> Option<(String, String)> {
    let turn_id = params.get("turnId").and_then(Value::as_str)?;
    let item_id = params.get("item")?.get("id").and_then(Value::as_str)?;
    Some((turn_id.to_string(), item_id.to_string()))
}

/// Core of the per-turn anchor bookkeeping, split out so it is testable
/// without spinning up a whole session.
pub(crate) fn advance_anchor(
    last_item_id: &mut HashMap<String, String>,
    turn_id: &str,
    item_id: &str,
) -> Option<String> {
    last_item_id.insert(turn_id.to_string(), item_id.to_string())
}

/// Key a turn's buffered reasoning summary by both ids, so the whole turn's
/// leftovers can be dropped when it ends without touching another turn's.
pub(crate) fn summary_key(turn_id: &str, item_id: &str) -> String {
    format!("{turn_id}\u{1f}{item_id}")
}

/// Accumulate one `item/reasoning/summaryPartAdded` or
/// `item/reasoning/summaryTextDelta` notification.
///
/// Reasoning text reaches the app only as these deltas: the `item/completed`
/// that follows carries an empty `summary`, so without this buffer the journal
/// — and therefore the thread after a restart — keeps no reasoning at all.
pub(crate) fn record_summary_delta(summaries: &mut HashMap<String, Vec<String>>, params: &Value) {
    let (Some(turn_id), Some(item_id)) = (
        params.get("turnId").and_then(Value::as_str),
        params.get("itemId").and_then(Value::as_str),
    ) else {
        return;
    };
    let index = params
        .get("summaryIndex")
        .and_then(Value::as_u64)
        .unwrap_or_default() as usize;
    let parts = summaries.entry(summary_key(turn_id, item_id)).or_default();
    if parts.len() <= index {
        parts.resize(index + 1, String::new());
    }
    let delta = params
        .get("delta")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if parts[index].len() < MAX_OUTPUT_BYTES {
        parts[index].push_str(delta);
    }
}

/// Put the streamed summary back onto a completed reasoning item. A summary
/// Codex did report is left alone, so this only ever fills the gap.
pub(crate) fn restore_summary(payload: &mut Value, parts: &[String]) {
    let Some(object) = payload.as_object_mut() else {
        return;
    };
    if object.get("type").and_then(Value::as_str) != Some("reasoning") {
        return;
    }
    let reported = object
        .get("summary")
        .and_then(Value::as_array)
        .is_some_and(|summary| {
            summary
                .iter()
                .any(|entry| entry.as_str().is_some_and(|text| !text.is_empty()))
        });
    if reported {
        return;
    }
    let streamed: Vec<&String> = parts.iter().filter(|part| !part.is_empty()).collect();
    if streamed.is_empty() {
        return;
    }
    object.insert("summary".into(), serde_json::json!(streamed));
}

/// Accumulate one `item/fileChange/patchUpdated` notification.
///
/// Codex reports a patch as it applies, and a later report need not repeat
/// every file an earlier one named, so the reports are unioned by path rather
/// than overwritten — otherwise a file edited early in a multi-file patch is
/// missing from the journal, and therefore from the thread after a restart.
pub(crate) fn record_patch_update(patches: &mut HashMap<String, Vec<Value>>, params: &Value) {
    let (Some(turn_id), Some(item_id)) = (
        params.get("turnId").and_then(Value::as_str),
        params.get("itemId").and_then(Value::as_str),
    ) else {
        return;
    };
    let Some(incoming) = params.get("changes").and_then(Value::as_array) else {
        return;
    };
    let held = patches.entry(summary_key(turn_id, item_id)).or_default();
    for change in incoming {
        let path = change.get("path").and_then(Value::as_str);
        match held
            .iter()
            .position(|existing| existing.get("path").and_then(Value::as_str) == path)
        {
            // The newer report carries the newer diff, in the slot the file was
            // first seen in — the list stays in first-touched order.
            Some(index) => held[index] = change.clone(),
            None => held.push(change.clone()),
        }
    }
}

/// Put the streamed patch back onto a completed file change. Changes Codex did
/// report are left alone, so this only ever fills the gap.
pub(crate) fn restore_changes(payload: &mut Value, streamed: &[Value]) {
    let Some(object) = payload.as_object_mut() else {
        return;
    };
    if object.get("type").and_then(Value::as_str) != Some("fileChange") {
        return;
    }
    let reported = object
        .get("changes")
        .and_then(Value::as_array)
        .is_some_and(|changes| !changes.is_empty());
    if reported || streamed.is_empty() {
        return;
    }
    object.insert("changes".into(), Value::Array(streamed.to_vec()));
}

/// Pick the journalable part out of an `item/completed` notification, or
/// nothing when the item is one Codex will hand back on its own.
pub(crate) fn journal_target(params: &Value) -> Option<JournalTarget> {
    let (thread_id, turn_id, item) = (
        params.get("threadId").and_then(Value::as_str)?,
        params.get("turnId").and_then(Value::as_str)?,
        params.get("item")?,
    );
    let item_id = item.get("id").and_then(Value::as_str)?;
    Some(JournalTarget {
        thread_id: thread_id.to_string(),
        turn_id: turn_id.to_string(),
        item_id: item_id.to_string(),
        payload: trim_payload(item.clone()),
    })
}

/// Persist a completed item so it survives the stream that carried it. Best
/// effort throughout: a journal that cannot be written must not disturb the
/// session it is watching.
pub(crate) fn journal_item(
    app: &AppHandle,
    home_key: &str,
    params: &Value,
    after_item_id: Option<String>,
    streamed_summary: Vec<String>,
    streamed_changes: Vec<Value>,
) {
    let Some(JournalTarget {
        thread_id,
        turn_id,
        item_id,
        mut payload,
    }) = journal_target(params)
    else {
        return;
    };
    restore_summary(&mut payload, &streamed_summary);
    // Restored diffs never passed through `journal_target`'s trim, and a
    // streamed patch is as capable of carrying a megabyte as a reported one.
    restore_changes(&mut payload, &streamed_changes);
    payload = trim_payload(payload);
    let (app, home_key) = (app.clone(), home_key.to_string());
    tauri::async_runtime::spawn(async move {
        let Some(database) = crate::database_for(&app, &home_key) else {
            return;
        };
        let _ = crate::storage::record_thread_item(
            &database,
            &thread_id,
            &turn_id,
            &item_id,
            &payload,
            after_item_id.as_deref(),
        )
        .await;
    });
}

/// The `turn.id` a `turn/started` or `turn/completed` notification carries.
fn turn_id(params: &Value) -> Option<&str> {
    params
        .get("turn")
        .and_then(|turn| turn.get("id"))
        .and_then(Value::as_str)
}

/// The per-child bookkeeping a journal needs, and the one place that decides
/// what a notification contributes to it.
///
/// Every app-server child the app runs — the main session and each subagent —
/// needs exactly this: buffered reasoning deltas, an anchor chain per turn, and
/// a record of which turns were watched from `turn/started` to `turn/completed`.
/// A child that keeps only some of it is not a lighter version of the same
/// thing, it is broken in a specific way: without the turn records
/// `threads::read` cannot replay the journal, so it falls back to splicing into
/// Codex's renumbered projection and every agent message it already returned is
/// merged in a second time under its streamed id.
pub(crate) struct TurnJournal {
    /// The most recent item id seen on the stream for each turn, in true
    /// arrival order. Item ids are opaque tool-call ids rather than a sortable
    /// sequence, so this is the only reliable way to know what a locally
    /// persisted item should be spliced in after when the thread is re-read.
    last_item_id: Mutex<HashMap<String, String>>,
    /// Reasoning summary text as it streams, keyed by `summary_key`. The
    /// `item/completed` that ends a reasoning item reports an empty summary, so
    /// these deltas are the only copy of the text there is to journal.
    summaries: Mutex<HashMap<String, Vec<String>>>,
    /// File changes as their patch applies, keyed by `summary_key`. A completed
    /// `fileChange` can report fewer files than it touched, and what is dropped
    /// here is missing from the thread's file list for good.
    patches: Mutex<HashMap<String, Vec<Value>>>,
    /// The turn currently open on each thread. An `error` notification ends a
    /// turn without a `turn/completed` and names no turn of its own, so this is
    /// the only way to know which one it just closed — and a turn left open
    /// reads as still running for as long as the row survives.
    open_turn: Mutex<HashMap<String, String>>,
    app: AppHandle,
    /// Canonical home key this journal writes under, resolving the right
    /// per-home database from a background task.
    home_key: String,
}

impl TurnJournal {
    pub(crate) fn new(app: AppHandle, home_key: String) -> Self {
        Self {
            last_item_id: Mutex::new(HashMap::new()),
            summaries: Mutex::new(HashMap::new()),
            patches: Mutex::new(HashMap::new()),
            open_turn: Mutex::new(HashMap::new()),
            app,
            home_key,
        }
    }

    /// Fold one notification into the journal: buffer reasoning, persist
    /// completed items in stream order, and record the turn boundaries that
    /// make a turn replayable.
    pub(crate) fn observe(&self, method: &str, params: &Value) {
        if method.starts_with("item/reasoning/summary") {
            if let Ok(mut summaries) = self.summaries.lock() {
                record_summary_delta(&mut summaries, params);
            }
        }
        if method == "item/fileChange/patchUpdated" {
            if let Ok(mut patches) = self.patches.lock() {
                record_patch_update(&mut patches, params);
            }
        }
        if method == "item/completed" {
            let ids = stream_item_id(params);
            let anchor = ids
                .as_ref()
                .and_then(|(turn_id, item_id)| self.anchor_and_advance(turn_id, item_id));
            let (summary, changes) = ids
                .map(|(turn_id, item_id)| {
                    (
                        self.take_summary(&turn_id, &item_id),
                        self.take_patch(&turn_id, &item_id),
                    )
                })
                .unwrap_or_default();
            journal_item(&self.app, &self.home_key, params, anchor, summary, changes);
        }
        if method == "turn/started" {
            self.track_turn(params, false);
            self.set_open_turn(params);
        }
        if method == "turn/completed" {
            self.track_turn(params, true);
            if let Some(turn_id) = turn_id(params) {
                self.forget_turn(turn_id);
            }
            self.clear_open_turn(params);
        }
        if method == "error" {
            self.close_open_turn(params);
        }
    }

    fn set_open_turn(&self, params: &Value) {
        let (Some(thread_id), Some(turn_id)) = (
            params.get("threadId").and_then(Value::as_str),
            turn_id(params),
        ) else {
            return;
        };
        if let Ok(mut open) = self.open_turn.lock() {
            open.insert(thread_id.to_string(), turn_id.to_string());
        }
    }

    fn clear_open_turn(&self, params: &Value) {
        let Some(thread_id) = params.get("threadId").and_then(Value::as_str) else {
            return;
        };
        if let Ok(mut open) = self.open_turn.lock() {
            open.remove(thread_id);
        }
    }

    /// Close the turn an `error` just ended, since the error names no turn and
    /// no `turn/completed` follows it.
    fn close_open_turn(&self, params: &Value) {
        let Some(thread_id) = params.get("threadId").and_then(Value::as_str) else {
            return;
        };
        let Some(turn_id) = self
            .open_turn
            .lock()
            .ok()
            .and_then(|mut open| open.remove(thread_id))
        else {
            return;
        };
        self.forget_turn(&turn_id);
        self.mark_complete(thread_id.to_string(), turn_id);
    }

    /// Record `item_id` as the latest item seen for `turn_id`, returning
    /// whatever preceded it (`None` if this is the turn's first). Must be
    /// called in true stream order.
    pub(crate) fn anchor_and_advance(&self, turn_id: &str, item_id: &str) -> Option<String> {
        let mut last_item_id = self.last_item_id.lock().ok()?;
        advance_anchor(&mut last_item_id, turn_id, item_id)
    }

    /// Hand over whatever text a completed item streamed, clearing the buffer.
    fn take_summary(&self, turn_id: &str, item_id: &str) -> Vec<String> {
        self.summaries
            .lock()
            .ok()
            .and_then(|mut summaries| summaries.remove(&summary_key(turn_id, item_id)))
            .unwrap_or_default()
    }

    /// Hand over whatever patch a completed item streamed, clearing the buffer.
    fn take_patch(&self, turn_id: &str, item_id: &str) -> Vec<Value> {
        self.patches
            .lock()
            .ok()
            .and_then(|mut patches| patches.remove(&summary_key(turn_id, item_id)))
            .unwrap_or_default()
    }

    /// Forget a finished turn's bookkeeping. An item that never completed would
    /// otherwise keep its half-streamed summary for the session's life.
    fn forget_turn(&self, turn_id: &str) {
        if let Ok(mut last_item_id) = self.last_item_id.lock() {
            last_item_id.remove(turn_id);
        }
        let prefix = summary_key(turn_id, "");
        if let Ok(mut summaries) = self.summaries.lock() {
            summaries.retain(|key, _| !key.starts_with(&prefix));
        }
        if let Ok(mut patches) = self.patches.lock() {
            patches.retain(|key, _| !key.starts_with(&prefix));
        }
    }

    /// Mark a turn as watched from its first item, so `threads::read` can later
    /// replay the journal for it instead of trusting Codex's projection.
    fn track_turn(&self, params: &Value, complete: bool) {
        let (Some(thread_id), Some(turn_id)) = (
            params.get("threadId").and_then(Value::as_str),
            turn_id(params),
        ) else {
            return;
        };
        let (thread_id, turn_id) = (thread_id.to_string(), turn_id.to_string());
        if complete {
            self.mark_complete(thread_id, turn_id);
        } else {
            self.write(|database| async move {
                let _ = crate::storage::record_turn_start(&database, &thread_id, &turn_id).await;
            });
        }
    }

    fn mark_complete(&self, thread_id: String, turn_id: String) {
        self.write(|database| async move {
            let _ = crate::storage::mark_turn_complete(&database, &thread_id, &turn_id).await;
        });
    }

    /// Run a journal write off the stream thread. The reader loop is
    /// synchronous, and a journal that cannot be written must not disturb the
    /// session it is watching.
    fn write<F, Fut>(&self, body: F)
    where
        F: FnOnce(turso::Database) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send,
    {
        let (app, home_key) = (self.app.clone(), self.home_key.clone());
        tauri::async_runtime::spawn(async move {
            let Some(database) = crate::database_for(&app, &home_key) else {
                return;
            };
            body(database).await;
        });
    }
}

/// Cut an item down to what is worth keeping on disk: the tail of an oversized
/// command output, and the head of an oversized diff (a diff reads top-down,
/// where output only ever shows its tail).
pub(crate) fn trim_payload(mut item: Value) -> Value {
    let Some(object) = item.as_object_mut() else {
        return item;
    };
    if let Some(output) = object.get("aggregatedOutput").and_then(Value::as_str) {
        if output.len() > MAX_OUTPUT_BYTES {
            let tail = from_char_boundary(output, output.len() - MAX_OUTPUT_BYTES);
            let trimmed = format!("[earlier output truncated]\n{tail}");
            object.insert("aggregatedOutput".into(), Value::String(trimmed));
        }
    }
    if let Some(changes) = object.get_mut("changes").and_then(Value::as_array_mut) {
        for change in changes {
            let Some(change) = change.as_object_mut() else {
                continue;
            };
            let Some(diff) = change.get("diff").and_then(Value::as_str) else {
                continue;
            };
            if diff.len() <= MAX_DIFF_BYTES {
                continue;
            }
            let head = &diff[..to_char_boundary(diff, MAX_DIFF_BYTES)];
            let trimmed = format!("{head}\n[rest of diff truncated]");
            change.insert("diff".into(), Value::String(trimmed));
        }
    }
    item
}

/// The slice from `start`, moved forward to the next char boundary.
fn from_char_boundary(text: &str, start: usize) -> &str {
    text.char_indices()
        .find(|(index, _)| *index >= start)
        .map(|(index, _)| &text[index..])
        .unwrap_or_default()
}

/// `end`, moved back to a char boundary so slicing cannot panic.
fn to_char_boundary(text: &str, end: usize) -> usize {
    let mut end = end.min(text.len());
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    end
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn completed(item: Value) -> Value {
        json!({"threadId": "thread-1", "turnId": "turn-1", "item": item})
    }

    #[test]
    fn journals_the_work_items_a_re_read_would_not_return() {
        let target = journal_target(&completed(json!({
            "id": "item_3",
            "type": "commandExecution",
            "command": "cargo test",
            "aggregatedOutput": "ok"
        })))
        .expect("command executions are journaled");
        assert_eq!(target.thread_id, "thread-1");
        assert_eq!(target.turn_id, "turn-1");
        assert_eq!(target.item_id, "item_3");
        assert_eq!(target.payload["command"], json!("cargo test"));
    }

    #[test]
    fn journals_every_kind_so_a_turn_is_recorded_in_full() {
        // Codex renumbers the items it does return, so a partial journal
        // cannot be lined up with its projection — the whole turn or nothing.
        for kind in ["userMessage", "fileChange", "plan", "agentMessage"] {
            let params = completed(json!({"id": "item_1", "type": kind}));
            assert!(
                journal_target(&params).is_some(),
                "{kind} should be journaled"
            );
        }
    }

    #[test]
    fn keeps_the_head_of_an_oversized_diff() {
        let diff = format!("@@ start\n{}", "+x\n".repeat(MAX_DIFF_BYTES));
        let target = journal_target(&completed(json!({
            "id": "item_1",
            "type": "fileChange",
            "changes": [{"path": "a.rs", "diff": diff}, {"path": "b.rs", "diff": "+ok"}]
        })))
        .expect("journaled");
        let stored = target.payload["changes"][0]["diff"].as_str().unwrap();
        assert!(stored.len() < diff.len());
        assert!(stored.starts_with("@@ start"));
        assert!(stored.ends_with("[rest of diff truncated]"));
        assert_eq!(target.payload["changes"][1]["diff"], json!("+ok"));
    }

    #[test]
    fn restores_the_summary_a_completed_reasoning_item_drops() {
        let mut summaries = HashMap::new();
        for delta in ["Weighing ", "the options"] {
            record_summary_delta(
                &mut summaries,
                &json!({"turnId": "turn-1", "itemId": "rs_1", "summaryIndex": 0, "delta": delta}),
            );
        }
        record_summary_delta(
            &mut summaries,
            &json!({"turnId": "turn-1", "itemId": "rs_1", "summaryIndex": 1, "delta": "Then acting"}),
        );
        let parts = summaries.remove(&summary_key("turn-1", "rs_1")).unwrap();
        let mut payload = json!({"id": "rs_1", "type": "reasoning", "summary": []});
        restore_summary(&mut payload, &parts);
        assert_eq!(
            payload["summary"],
            json!(["Weighing the options", "Then acting"])
        );
    }

    #[test]
    fn a_reported_summary_is_left_alone() {
        let mut payload = json!({"id": "rs_1", "type": "reasoning", "summary": ["From Codex"]});
        restore_summary(&mut payload, &["Streamed".to_string()]);
        assert_eq!(payload["summary"], json!(["From Codex"]));
    }

    #[test]
    fn only_reasoning_items_take_a_streamed_summary() {
        let mut payload = json!({"id": "item_1", "type": "commandExecution"});
        restore_summary(&mut payload, &["Streamed".to_string()]);
        assert_eq!(payload.get("summary"), None);
    }

    #[test]
    fn unions_the_files_a_patch_reports_as_it_applies() {
        let mut patches = HashMap::new();
        record_patch_update(
            &mut patches,
            &json!({"turnId": "turn-1", "itemId": "fc_1", "changes": [
                {"path": "a.rs", "kind": {"type": "update"}, "diff": "+a"}
            ]}),
        );
        // The second report names a new file and re-reports the first with a
        // fuller diff; neither may cost us the other.
        record_patch_update(
            &mut patches,
            &json!({"turnId": "turn-1", "itemId": "fc_1", "changes": [
                {"path": "b.rs", "kind": {"type": "add"}, "diff": "+b"},
                {"path": "a.rs", "kind": {"type": "update"}, "diff": "+a\n+a2"}
            ]}),
        );
        let held = patches.remove(&summary_key("turn-1", "fc_1")).unwrap();
        assert_eq!(held.len(), 2);
        assert_eq!(held[0]["path"], json!("a.rs"));
        assert_eq!(held[0]["diff"], json!("+a\n+a2"));
        assert_eq!(held[1]["path"], json!("b.rs"));
    }

    #[test]
    fn restores_the_changes_a_completed_file_change_drops() {
        let streamed = vec![json!({"path": "a.rs", "kind": {"type": "update"}, "diff": "+a"})];
        let mut payload = json!({"id": "fc_1", "type": "fileChange", "changes": []});
        restore_changes(&mut payload, &streamed);
        assert_eq!(payload["changes"], json!(streamed));

        // Missing outright, not merely empty.
        let mut absent = json!({"id": "fc_1", "type": "fileChange"});
        restore_changes(&mut absent, &streamed);
        assert_eq!(absent["changes"], json!(streamed));
    }

    #[test]
    fn reported_changes_are_left_alone() {
        let mut payload =
            json!({"id": "fc_1", "type": "fileChange", "changes": [{"path": "from-codex.rs"}]});
        restore_changes(
            &mut payload,
            &[json!({"path": "streamed.rs", "diff": "+x"})],
        );
        assert_eq!(payload["changes"][0]["path"], json!("from-codex.rs"));
    }

    #[test]
    fn only_file_changes_take_a_streamed_patch() {
        let mut payload = json!({"id": "item_1", "type": "commandExecution"});
        restore_changes(&mut payload, &[json!({"path": "a.rs", "diff": "+x"})]);
        assert_eq!(payload.get("changes"), None);
    }

    #[test]
    fn a_restored_diff_is_still_trimmed() {
        let diff = format!("@@ start\n{}", "+x\n".repeat(MAX_DIFF_BYTES));
        let mut payload = json!({"id": "fc_1", "type": "fileChange", "changes": []});
        restore_changes(&mut payload, &[json!({"path": "a.rs", "diff": diff})]);
        let stored = trim_payload(payload);
        let kept = stored["changes"][0]["diff"].as_str().unwrap();
        assert!(kept.starts_with("@@ start"));
        assert!(kept.ends_with("[rest of diff truncated]"));
    }

    #[test]
    fn skips_notifications_missing_the_ids_a_row_needs() {
        assert!(journal_target(&json!({"turnId": "turn-1", "item": {"id": "item_1"}})).is_none());
        assert!(journal_target(&completed(json!({"type": "commandExecution"}))).is_none());
    }

    #[test]
    fn keeps_the_tail_of_an_oversized_command_output() {
        let output = format!("{}\nlast line", "x".repeat(MAX_OUTPUT_BYTES * 2));
        let target = journal_target(&completed(json!({
            "id": "item_1",
            "type": "commandExecution",
            "aggregatedOutput": output
        })))
        .expect("journaled");
        let stored = target.payload["aggregatedOutput"].as_str().unwrap();
        assert!(stored.len() < output.len());
        assert!(stored.starts_with("[earlier output truncated]"));
        assert!(stored.ends_with("\nlast line"));
    }

    #[test]
    fn short_output_is_stored_verbatim() {
        let target = journal_target(&completed(json!({
            "id": "item_1",
            "type": "commandExecution",
            "aggregatedOutput": "all good"
        })))
        .expect("journaled");
        assert_eq!(target.payload["aggregatedOutput"], json!("all good"));
    }

    #[test]
    fn stream_item_id_reads_turn_and_item_regardless_of_kind() {
        assert_eq!(
            stream_item_id(&completed(json!({"id": "item_1", "type": "userMessage"}))),
            Some(("turn-1".to_string(), "item_1".to_string()))
        );
        assert_eq!(stream_item_id(&json!({"turnId": "turn-1"})), None);
    }

    #[test]
    fn anchors_each_item_after_the_last_one_seen_in_its_turn() {
        let mut last_item_id = HashMap::new();
        assert_eq!(advance_anchor(&mut last_item_id, "turn-1", "call_1"), None);
        assert_eq!(
            advance_anchor(&mut last_item_id, "turn-1", "call_2"),
            Some("call_1".to_string())
        );
        // A different turn tracks its own chain.
        assert_eq!(advance_anchor(&mut last_item_id, "turn-2", "call_9"), None);
        assert_eq!(
            advance_anchor(&mut last_item_id, "turn-1", "call_3"),
            Some("call_2".to_string())
        );
    }
}
