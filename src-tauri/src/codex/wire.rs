//! An opt-in record of the JSON-RPC traffic between this app and the Codex
//! app-server, so the frontend can show what was actually said to and from the
//! agent. Recording is off until the frontend turns it on (the payloads are
//! large and nobody pays for them unless the log is open), and the buffer is
//! in-memory only — nothing is written to disk.

use serde::Serialize;
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, State};

use crate::util::time::unix_millis;
use crate::AppState;

/// How many messages the ring buffer keeps. Deep enough to cover a long turn,
/// shallow enough that a chatty session cannot grow without bound.
const MAX_ENTRIES: usize = 500;

/// Payloads above this serialized size are stored as a preview instead of the
/// whole value — a single `item/fileChange` can carry a megabyte of diff.
const MAX_PAYLOAD_BYTES: usize = 32_768;

/// One JSON-RPC message, in whichever direction it travelled.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WireMessage {
    /// Monotonic within a run; the frontend uses it as a list key.
    pub(crate) seq: u64,
    /// Unix milliseconds.
    pub(crate) at: i64,
    /// `"out"` — app to Codex; `"in"` — Codex to app.
    pub(crate) direction: String,
    /// `request` | `response` | `notification` | `serverRequest` | `error`.
    pub(crate) kind: String,
    pub(crate) method: Option<String>,
    pub(crate) id: Option<i64>,
    /// Pulled out of the params when present, so the viewer can filter by thread.
    pub(crate) thread_id: Option<String>,
    /// The message body: `params` for requests/notifications, `result` or
    /// `error` for responses.
    pub(crate) payload: Value,
    /// True when `payload` is a preview of an oversized body.
    pub(crate) truncated: bool,
}

/// Classify a raw JSON-RPC line into a log entry. `direction` is `"out"` for
/// messages this app wrote and `"in"` for messages Codex sent.
fn classify(seq: u64, direction: &str, value: &Value) -> WireMessage {
    let id = value.get("id").and_then(Value::as_i64);
    let method = value.get("method").and_then(Value::as_str);
    let outbound = direction == "out";
    let kind = match (id, method) {
        // An id with a method is a call; inbound calls are the server asking us
        // (approvals, user input), outbound ones are our own requests.
        (Some(_), Some(_)) if outbound => "request",
        (Some(_), Some(_)) => "serverRequest",
        (Some(_), None) if value.get("error").is_some() => "error",
        (Some(_), None) => "response",
        (None, _) => "notification",
    };
    // A response carries `result`/`error`; everything else carries `params`.
    let body = value
        .get("params")
        .or_else(|| value.get("result"))
        .or_else(|| value.get("error"))
        .cloned()
        .unwrap_or(Value::Null);
    let thread_id = body
        .get("threadId")
        .and_then(Value::as_str)
        .map(str::to_string);
    let (payload, truncated) = shrink(body);
    WireMessage {
        seq,
        at: unix_millis(),
        direction: direction.to_string(),
        kind: kind.to_string(),
        method: method.map(str::to_string),
        id,
        thread_id,
        payload,
        truncated,
    }
}

/// Replace an oversized body with a head-of-string preview so one huge diff
/// cannot blow up the buffer or the event payload.
fn shrink(body: Value) -> (Value, bool) {
    let text = body.to_string();
    if text.len() <= MAX_PAYLOAD_BYTES {
        return (body, false);
    }
    let mut end = MAX_PAYLOAD_BYTES;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    (json!({ "preview": &text[..end] }), true)
}

/// The in-memory buffer plus its on/off switch. Cheap to clone around as an
/// `Arc`; every method is safe to call when logging is off (they no-op).
#[derive(Default)]
pub(crate) struct WireLog {
    enabled: AtomicBool,
    seq: AtomicU64,
    entries: Mutex<VecDeque<WireMessage>>,
}

impl WireLog {
    pub(crate) fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    /// Turn recording on or off. Switching off also empties the buffer so the
    /// captured traffic does not linger after the user opts out.
    pub(crate) fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
        if !enabled {
            self.clear();
        }
    }

    pub(crate) fn clear(&self) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.clear();
        }
    }

    pub(crate) fn entries(&self) -> Vec<WireMessage> {
        self.entries
            .lock()
            .map(|entries| entries.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Record one message and push it to the frontend. A no-op while logging
    /// is off, which is the hot path for every request the app makes.
    pub(crate) fn record(&self, app: Option<&AppHandle>, direction: &str, value: &Value) {
        if !self.enabled() {
            return;
        }
        let seq = self.seq.fetch_add(1, Ordering::Relaxed);
        let entry = classify(seq, direction, value);
        if let Ok(mut entries) = self.entries.lock() {
            if entries.len() == MAX_ENTRIES {
                entries.pop_front();
            }
            entries.push_back(entry.clone());
        }
        if let Some(app) = app {
            let _ = app.emit("codex:wire", entry);
        }
    }
}

/// Start or stop recording. Stopping clears whatever was captured.
#[tauri::command]
pub(crate) fn set_wire_logging(enabled: bool, state: State<'_, AppState>) {
    state.session.wire().set_enabled(enabled);
}

/// The buffered messages, oldest first. Empty while logging is off.
#[tauri::command]
pub(crate) fn read_wire_log(state: State<'_, AppState>) -> Vec<WireMessage> {
    state.session.wire().entries()
}

#[tauri::command]
pub(crate) fn clear_wire_log(state: State<'_, AppState>) {
    state.session.wire().clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_each_message_shape() {
        let outbound_call =
            json!({"id": 3, "method": "thread/start", "params": {"threadId": "t1"}});
        let entry = classify(0, "out", &outbound_call);
        assert_eq!(entry.kind, "request");
        assert_eq!(entry.method.as_deref(), Some("thread/start"));
        assert_eq!(entry.thread_id.as_deref(), Some("t1"));
        assert_eq!(entry.payload, json!({"threadId": "t1"}));

        // The same shape arriving from Codex is the server asking us something.
        assert_eq!(classify(1, "in", &outbound_call).kind, "serverRequest");

        let response = json!({"id": 3, "result": {"ok": true}});
        let entry = classify(2, "in", &response);
        assert_eq!(entry.kind, "response");
        assert_eq!(entry.payload, json!({"ok": true}));

        let failure = json!({"id": 3, "error": {"message": "nope"}});
        let entry = classify(3, "in", &failure);
        assert_eq!(entry.kind, "error");
        assert_eq!(entry.payload, json!({"message": "nope"}));

        let notification = json!({"method": "turn/completed", "params": {}});
        assert_eq!(classify(4, "in", &notification).kind, "notification");
    }

    #[test]
    fn truncates_oversized_payloads_on_a_char_boundary() {
        let body = json!({"diff": "é".repeat(MAX_PAYLOAD_BYTES)});
        let (payload, truncated) = shrink(body);
        assert!(truncated);
        let preview = payload.get("preview").and_then(Value::as_str).unwrap();
        assert!(preview.len() <= MAX_PAYLOAD_BYTES);
        assert!(preview.starts_with("{\"diff\":\""));
    }

    #[test]
    fn records_nothing_until_enabled() {
        let log = WireLog::default();
        log.record(None, "out", &json!({"id": 1, "method": "a", "params": {}}));
        assert!(log.entries().is_empty());

        log.set_enabled(true);
        log.record(None, "out", &json!({"id": 1, "method": "a", "params": {}}));
        assert_eq!(log.entries().len(), 1);
    }

    #[test]
    fn caps_the_buffer_and_keeps_the_newest() {
        let log = WireLog::default();
        log.set_enabled(true);
        for index in 0..(MAX_ENTRIES + 5) {
            log.record(
                None,
                "in",
                &json!({"method": format!("event/{index}"), "params": {}}),
            );
        }
        let entries = log.entries();
        assert_eq!(entries.len(), MAX_ENTRIES);
        assert_eq!(entries[0].method.as_deref(), Some("event/5"));
        assert_eq!(entries[0].seq, 5);
    }

    #[test]
    fn disabling_clears_the_buffer() {
        let log = WireLog::default();
        log.set_enabled(true);
        log.record(None, "in", &json!({"method": "a", "params": {}}));
        log.set_enabled(false);
        assert!(log.entries().is_empty());
    }
}
