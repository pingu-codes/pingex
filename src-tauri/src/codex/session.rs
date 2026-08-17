use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};

use crate::codex::child::{spawn_child, ChildSink, CodexChild, RequestError};
use crate::codex::journal::TurnJournal;
use crate::codex::wire::WireLog;
use crate::{RuntimeConfig, SharedRuntime};

/// JSON-RPC "method not found", used to refuse server requests this client
/// does not implement.
const METHOD_NOT_FOUND: i64 = -32601;

/// A single long-lived `codex app-server` process shared by all commands.
/// Responses are matched to callers by request id; notifications and
/// server-initiated requests (approvals) are forwarded to the frontend as
/// Tauri events.
pub(crate) struct CodexSession {
    runtime: SharedRuntime,
    inner: tokio::sync::Mutex<Option<MainSession>>,
    /// Opt-in record of the traffic below, shared with every spawned child so
    /// switching homes does not lose the switch.
    wire: Arc<WireLog>,
}

/// The live child plus the state that only the main session keeps.
struct MainSession {
    child: Arc<CodexChild>,
    sink: Arc<MainSessionSink>,
}

/// What the app does with everything the main app-server child says: journal
/// the work items, cache the settings a thread resolved to, and forward the
/// rest to the frontend.
struct MainSessionSink {
    resumed: Mutex<HashMap<String, Value>>,
    /// Why this child refused `thread/queue/*`, once it has. Lives on the sink
    /// rather than on [`CodexSession`] so that replacing the child — a binary
    /// override, a home switch, a crash respawn — forgets it structurally,
    /// with no reset code to keep in step. See `threads::queue`.
    queue_unsupported: Mutex<Option<String>>,
    /// Journaling and per-turn stream bookkeeping, shared with every subagent's
    /// sink so both keep the same record of what streamed.
    journal: TurnJournal,
    app: AppHandle,
}

impl MainSessionSink {
    fn new(app: AppHandle) -> Self {
        Self {
            resumed: Mutex::new(HashMap::new()),
            queue_unsupported: Mutex::new(None),
            journal: TurnJournal::new(app.clone()),
            app,
        }
    }

    /// Fold the subagent policies a thread just resolved to into the cached
    /// resume response, so the composer can show them without another read.
    fn cache_thread_settings(&self, params: &Value) {
        let (Some(thread_id), Some(settings)) = (
            params.get("threadId").and_then(Value::as_str),
            params.get("threadSettings"),
        ) else {
            return;
        };
        let Ok(mut resumed) = self.resumed.lock() else {
            return;
        };
        let cached = resumed
            .entry(thread_id.to_string())
            .or_insert_with(|| json!({}));
        if !cached.is_object() {
            *cached = json!({});
        }
        if let Some(object) = cached.as_object_mut() {
            for key in ["subagentModelPolicy", "subagentReasoningEffortPolicy"] {
                if let Some(setting) = settings.get(key) {
                    object.insert(key.to_string(), setting.clone());
                }
            }
        }
    }
}

impl ChildSink for MainSessionSink {
    fn on_notification(&self, method: &str, params: &Value) {
        self.journal.observe(method, params);
        if method == "thread/settings/updated" {
            self.cache_thread_settings(params);
        }
        let _ = self.app.emit(
            "codex:event",
            json!({
                "method": method,
                "params": params,
            }),
        );
    }

    fn on_server_request(&self, child: &Arc<CodexChild>, id: i64, method: &str, params: &Value) {
        // Requests the app answers on its own, without ever bothering the user.
        // These live in Rust rather than the webview for the same reason the
        // agent tools below do — a reply held by the webview does not survive a
        // reload — and because Codex blocks the turn behind any request it
        // never hears back about, so a method we cannot serve still has to be
        // refused rather than dropped.
        match method {
            "currentTime/read" => {
                let seconds = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|since_epoch| since_epoch.as_secs() as i64)
                    .unwrap_or_default();
                let _ = child.respond(id, json!({"currentTimeAt": seconds}));
                return;
            }
            "attestation/generate" | "account/chatgptAuthTokens/refresh" => {
                let _ = child.respond_error(
                    id,
                    METHOD_NOT_FOUND,
                    &format!("{method} is not supported by this client"),
                );
                return;
            }
            _ => {}
        }
        // Our own agent tools are answered in Rust and never reach the
        // frontend: a `wait_agents` response can be outstanding for many
        // minutes, and one held by the webview would not survive a reload.
        // Anything else — including a dynamic tool we do not own — falls
        // through untouched.
        if method == "item/tool/call" && crate::agents::tools::owns(params) {
            let (app, params) = (self.app.clone(), params.clone());
            tauri::async_runtime::spawn(async move {
                crate::agents::handle_tool_call(app, id, params).await;
            });
            return;
        }
        let mut params = params.clone();
        if method == "item/tool/requestUserInput" {
            let ids = (
                params
                    .get("turnId")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                params
                    .get("itemId")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            );
            if let (Some(turn_id), Some(item_id)) = ids {
                let anchor = self.journal.anchor_and_advance(&turn_id, &item_id);
                if let (Some(object), Some(anchor)) = (params.as_object_mut(), anchor) {
                    object.insert("afterItemId".into(), Value::String(anchor));
                }
            }
        }
        let _ = self.app.emit(
            "codex:serverRequest",
            json!({
                "requestId": id,
                "method": method,
                "params": params,
            }),
        );
    }

    fn on_closed(&self) {
        let _ = self.app.emit("codex:disconnected", ());
    }
}

async fn spawn_session(
    runtime: &RuntimeConfig,
    app: AppHandle,
    wire: Arc<WireLog>,
) -> Result<MainSession, String> {
    crate::codex::child::kill_orphaned_app_servers();
    // Resolve to an absolute path: a Finder-launched bundle has a bare PATH, so
    // spawning bare `codex` would fail even though the CLI is installed.
    let program = crate::codex::binary::resolve(&runtime.codex_binary)
        .ok_or_else(|| crate::codex::binary::missing_message(&runtime.codex_binary))?;
    let sink = Arc::new(MainSessionSink::new(app.clone()));
    let child = spawn_child(
        &program,
        std::path::Path::new(&runtime.codex_home),
        "pingex",
        app,
        wire,
        sink.clone(),
    )
    .await?;
    Ok(MainSession { child, sink })
}

impl CodexSession {
    pub(crate) fn new(runtime: SharedRuntime) -> Self {
        Self {
            runtime,
            inner: tokio::sync::Mutex::new(None),
            wire: Arc::new(WireLog::default()),
        }
    }

    /// The wire log shared by every app-server child this session spawns.
    pub(crate) fn wire(&self) -> &Arc<WireLog> {
        &self.wire
    }

    /// Kill the current app-server child (if any) and forget it so the next
    /// request spawns a fresh one — used when the active Codex home changes.
    pub(crate) async fn reset(&self) {
        let mut guard = self.inner.lock().await;
        if let Some(session) = guard.take() {
            session.child.kill();
        }
    }

    /// Kill the app-server child on app exit so it never outlives us holding
    /// the remote-control relay. Sync because Tauri's exit callback is not
    /// async; try_lock keeps it non-blocking on the off chance a request
    /// holds the session lock during shutdown.
    pub(crate) fn kill_child(&self) {
        if let Ok(inner) = self.inner.try_lock() {
            if let Some(session) = inner.as_ref() {
                session.child.kill();
            }
        }
    }

    async fn session(
        &self,
        app: &AppHandle,
    ) -> Result<(Arc<CodexChild>, Arc<MainSessionSink>), String> {
        let mut guard = self.inner.lock().await;
        if let Some(session) = guard.as_ref() {
            if session.child.is_alive() {
                return Ok((session.child.clone(), session.sink.clone()));
            }
        }
        let runtime = self.runtime.read().expect("runtime lock poisoned").clone();
        let session = spawn_session(&runtime, app.clone(), self.wire.clone()).await?;
        let pair = (session.child.clone(), session.sink.clone());
        *guard = Some(session);
        Ok(pair)
    }

    /// Send a prebuilt [`requests::Request`].
    pub(crate) async fn send(
        &self,
        app: &AppHandle,
        request: crate::codex::requests::Request,
    ) -> Result<Value, String> {
        self.request(app, request.method, request.params).await
    }

    pub(crate) async fn request(
        &self,
        app: &AppHandle,
        method: &str,
        params: Value,
    ) -> Result<Value, String> {
        // One retry: if writing fails the request never reached Codex, so it
        // is safe to respawn and resend. Failures after a successful write are
        // surfaced as-is because the request may already be executing.
        for attempt in 0..2 {
            let (child, _) = self.session(app).await?;
            match child.try_request(method, params.clone()).await {
                Ok(response) => return Ok(response),
                Err(RequestError::NotSent(error)) if attempt == 0 => {
                    let _ = error;
                    continue;
                }
                Err(error) => return Err(error.into()),
            }
        }
        unreachable!()
    }

    /// Answer a server-initiated request (e.g. an approval) by id.
    pub(crate) async fn respond(&self, request_id: i64, result: Value) -> Result<(), String> {
        let guard = self.inner.lock().await;
        let session = guard
            .as_ref()
            .filter(|session| session.child.is_alive())
            .ok_or("Codex is not running")?;
        session.child.respond(request_id, result)
    }

    /// Load a thread into the live session with `thread/resume` once per
    /// session lifetime; a respawned session starts with an empty set.
    pub(crate) async fn ensure_resumed(
        &self,
        app: &AppHandle,
        thread_id: &str,
    ) -> Result<Value, String> {
        let (_, sink) = self.session(app).await?;
        {
            let resumed = sink
                .resumed
                .lock()
                .map_err(|_| "Codex resumed lock was poisoned".to_string())?;
            if let Some(response) = resumed.get(thread_id) {
                return Ok(response.clone());
            }
        }
        let response = self
            .send(app, crate::codex::requests::thread_resume(thread_id))
            .await?;
        let (_, sink) = self.session(app).await?;
        sink.resumed
            .lock()
            .map_err(|_| "Codex resumed lock was poisoned".to_string())?
            .insert(thread_id.to_string(), response.clone());
        Ok(response)
    }

    /// Why the live child refuses `thread/queue/*`, if it has already said so.
    /// `None` means "not yet known to be unsupported" — worth trying.
    pub(crate) async fn queue_unsupported(
        &self,
        app: &AppHandle,
    ) -> Result<Option<String>, String> {
        let (_, sink) = self.session(app).await?;
        let reason = sink
            .queue_unsupported
            .lock()
            .map_err(|_| "Codex queue support lock was poisoned".to_string())?
            .clone();
        Ok(reason)
    }

    /// Remember that the live child has no usable server-side queue, so later
    /// calls can short-circuit instead of paying a round trip to be refused.
    pub(crate) async fn mark_queue_unsupported(
        &self,
        app: &AppHandle,
        reason: &str,
    ) -> Result<(), String> {
        let (_, sink) = self.session(app).await?;
        *sink
            .queue_unsupported
            .lock()
            .map_err(|_| "Codex queue support lock was poisoned".to_string())? =
            Some(reason.to_string());
        Ok(())
    }

    /// Record that a thread is already live in the session (e.g. one we just
    /// created with `thread/start`).
    pub(crate) async fn mark_resumed(
        &self,
        app: &AppHandle,
        thread_id: &str,
    ) -> Result<(), String> {
        let (_, sink) = self.session(app).await?;
        sink.resumed
            .lock()
            .map_err(|_| "Codex resumed lock was poisoned".to_string())?
            .insert(thread_id.to_string(), Value::Null);
        Ok(())
    }
}
