use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::AppHandle;
use tauri_specta::Event;

use crate::codex::child::{spawn_child, ChildSink, CodexChild, RequestError};
use crate::codex::compat::{self, Feature};
use crate::codex::events::{
    CodexDisconnected, CodexEvent, CodexNotification, CodexServerRequest, ServerRequest,
};
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
    /// Canonical home this session belongs to; stamped onto every event so a
    /// window bound to another home can drop it.
    home_key: String,
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
    /// Canonical home of the context this child serves — the `codexHome` tag
    /// on everything emitted, and the key background writes resolve their
    /// database through.
    home_key: String,
    resumed: Mutex<HashMap<String, Value>>,
    /// Why this child refused an optional API, keyed by the feature's method
    /// prefix, once it has. Lives on the sink rather than on [`CodexSession`]
    /// so that replacing the child — a binary override, a home switch, a
    /// crash respawn — forgets it structurally, with no reset code to keep in
    /// step. See [`crate::codex::compat`].
    unsupported: Mutex<HashMap<&'static str, String>>,
    /// Journaling and per-turn stream bookkeeping, shared with every subagent's
    /// sink so both keep the same record of what streamed.
    journal: TurnJournal,
    app: AppHandle,
}

impl MainSessionSink {
    fn new(app: AppHandle, home_key: String) -> Self {
        Self {
            resumed: Mutex::new(HashMap::new()),
            unsupported: Mutex::new(HashMap::new()),
            journal: TurnJournal::new(
                app.clone(),
                home_key.clone(),
                crate::harness::HarnessKind::Codex,
            ),
            home_key,
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
        let trimmed = crate::codex::journal::trim_live_params(method, params);
        let params = trimmed.as_ref().unwrap_or(params);
        let _ = CodexEvent {
            codex_home: self.home_key.clone(),
            event: CodexNotification::decode(method, params),
        }
        .emit(&self.app);
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
        let mut request = ServerRequest::decode(method, params);
        if let ServerRequest::ToolRequestUserInput(question) = &mut request {
            if let (Some(turn_id), Some(item_id)) = (&question.turn_id, &question.item_id) {
                question.after_item_id = self.journal.anchor_and_advance(turn_id, item_id);
            }
        }
        let _ = CodexServerRequest {
            codex_home: self.home_key.clone(),
            request_id: id,
            request,
        }
        .emit(&self.app);
    }

    fn on_closed(&self) {
        // Carries the home so one account's child dying does not tear down a
        // window bound to another account.
        let _ = CodexDisconnected {
            codex_home: self.home_key.clone(),
        }
        .emit(&self.app);
    }
}

async fn spawn_session(
    runtime: &RuntimeConfig,
    home_key: String,
    app: AppHandle,
    wire: Arc<WireLog>,
) -> Result<MainSession, String> {
    // Resolve to an absolute path: a Finder-launched bundle has a bare PATH, so
    // spawning bare `codex` would fail even though the CLI is installed; a
    // `wsl.exe` launch has no login PATH at all. Both this and the orphan
    // sweep run `ps`/login shells (through `wsl.exe` on WSL), so they go to
    // a blocking thread rather than stall an async worker.
    let (host, binary) = (runtime.host.clone(), runtime.codex_binary_str());
    let program = tauri::async_runtime::spawn_blocking(move || {
        crate::codex::child::kill_orphaned_app_servers(std::slice::from_ref(&host));
        host.resolve_binary(&binary)
            .ok_or_else(|| crate::codex::binary::missing_message_on(&host, &binary))
    })
    .await
    .map_err(|error| format!("Could not start Codex: {error}"))??;
    let sink = Arc::new(MainSessionSink::new(app.clone(), home_key));
    let child = spawn_child(
        &runtime.host,
        &program,
        &runtime.codex_home_str(),
        "pingex",
        app,
        wire,
        sink.clone(),
    )
    .await?;
    Ok(MainSession { child, sink })
}

/// Send on the live session, and return the response together with the
/// session that answered it. One retry: if writing fails the request never
/// reached Codex, so it is safe to respawn and resend. Failures after a
/// successful write are surfaced as-is because the request may already be
/// executing. Callers keep per-session state (what is resumed) on the
/// returned session, never on whichever is live by the time they look.
async fn served_by<S, SessionFut, SendFut>(
    mut session: impl FnMut() -> SessionFut,
    send: impl Fn(S) -> SendFut,
) -> Result<(Value, S), String>
where
    S: Clone,
    SessionFut: std::future::Future<Output = Result<S, String>>,
    SendFut: std::future::Future<Output = Result<Value, RequestError>>,
{
    for attempt in 0..2 {
        let current = session().await?;
        match send(current.clone()).await {
            Ok(response) => return Ok((response, current)),
            Err(RequestError::NotSent(_)) if attempt == 0 => continue,
            Err(error) => return Err(error.into()),
        }
    }
    unreachable!()
}

impl CodexSession {
    pub(crate) fn new(runtime: SharedRuntime, home_key: String) -> Self {
        Self {
            runtime,
            home_key,
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

    /// Threads with a turn in flight on the live child. Does not spawn a child:
    /// with none running there is nothing in flight to report.
    pub(crate) async fn active_threads(&self) -> Vec<String> {
        let guard = self.inner.lock().await;
        guard
            .as_ref()
            .filter(|session| session.child.is_alive())
            .map(|session| session.sink.journal.open_threads())
            .unwrap_or_default()
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
        let session = spawn_session(
            &runtime,
            self.home_key.clone(),
            app.clone(),
            self.wire.clone(),
        )
        .await?;
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
        Ok(self.request_served(app, method, params).await?.0)
    }

    /// [`Self::request`], also naming the session that answered: a respawn
    /// can happen mid-call, and per-session state (what is resumed) belongs
    /// to the child that actually handled the request.
    async fn request_served(
        &self,
        app: &AppHandle,
        method: &str,
        params: Value,
    ) -> Result<(Value, Arc<MainSessionSink>), String> {
        let (response, (_, sink)) = served_by(
            || self.session(app),
            |(child, _): (Arc<CodexChild>, Arc<MainSessionSink>)| {
                let params = params.clone();
                async move { child.try_request(method, params).await }
            },
        )
        .await?;
        Ok((response, sink))
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
        let request = crate::codex::requests::thread_resume(thread_id);
        // Recorded on the session that served the resume, not whichever one
        // `session()` returns afterwards: a child respawned in between never
        // loaded this thread, and marking it resumed would fail the next turn.
        let (response, sink) = self
            .request_served(app, request.method, request.params)
            .await?;
        sink.resumed
            .lock()
            .map_err(|_| "Codex resumed lock was poisoned".to_string())?
            .insert(thread_id.to_string(), response.clone());
        Ok(response)
    }

    /// Why the live child refuses `feature`, if it has already said so.
    /// `None` means "not yet known to be unsupported" — worth trying.
    pub(crate) async fn unsupported(
        &self,
        app: &AppHandle,
        feature: Feature,
    ) -> Result<Option<String>, String> {
        let (_, sink) = self.session(app).await?;
        let reason = sink
            .unsupported
            .lock()
            .map_err(|_| "Codex feature support lock was poisoned".to_string())?
            .get(feature.method_prefix)
            .cloned();
        Ok(reason)
    }

    /// Remember that the live child lacks `feature`, so later calls can
    /// short-circuit instead of paying a round trip to be refused.
    pub(crate) async fn mark_unsupported(
        &self,
        app: &AppHandle,
        feature: Feature,
        reason: &str,
    ) -> Result<(), String> {
        let (_, sink) = self.session(app).await?;
        sink.unsupported
            .lock()
            .map_err(|_| "Codex feature support lock was poisoned".to_string())?
            .insert(feature.method_prefix, reason.to_string());
        Ok(())
    }

    /// Send a request belonging to an optional `feature`: short-circuit if this
    /// child already refused it, and on a first refusal remember that and
    /// report it under the feature's error prefix. `classify` lets a caller
    /// add feature-specific refusal shapes on top of the generic ones.
    pub(crate) async fn send_gated(
        &self,
        app: &AppHandle,
        feature: Feature,
        request: crate::codex::requests::Request,
        classify: impl Fn(&str) -> Option<String>,
    ) -> Result<Value, String> {
        if let Some(reason) = self.unsupported(app, feature).await? {
            return Err(feature.error(&reason));
        }
        match self.send(app, request).await {
            Ok(response) => Ok(response),
            Err(error) => {
                let reason = compat::method_unsupported(&error, feature.method_prefix)
                    .or_else(|| classify(&error));
                match reason {
                    Some(reason) => {
                        self.mark_unsupported(app, feature, &reason).await?;
                        Err(feature.error(&reason))
                    }
                    None => Err(error),
                }
            }
        }
    }

    /// What the live child said about itself in the `initialize` handshake
    /// (`userAgent`, `platformOs`, …), spawning it if needed.
    pub(crate) async fn server_info(&self, app: &AppHandle) -> Result<Value, String> {
        let (child, _) = self.session(app).await?;
        Ok(child.server_info())
    }

    /// Record that a thread is already live in the session (e.g. one we just
    /// created with `thread/start`).
    pub(crate) async fn mark_resumed(
        &self,
        app: &AppHandle,
        thread_id: &str,
        response: &Value,
    ) -> Result<(), String> {
        let (_, sink) = self.session(app).await?;
        sink.resumed
            .lock()
            .map_err(|_| "Codex resumed lock was poisoned".to_string())?
            .insert(thread_id.to_string(), response.clone());
        Ok(())
    }

    pub(crate) async fn cache_speed_tier(
        &self,
        app: &AppHandle,
        thread_id: &str,
        tier: &str,
    ) -> Result<(), String> {
        let (_, sink) = self.session(app).await?;
        let mut resumed = sink
            .resumed
            .lock()
            .map_err(|_| "Codex resumed lock was poisoned".to_string())?;
        let response = resumed
            .entry(thread_id.to_string())
            .or_insert_with(|| json!({}));
        if !response.is_object() {
            *response = json!({});
        }
        response["serviceTier"] = json!(tier);
        Ok(())
    }

    /// Refresh settings when opening a transcript, including changes made by another client.
    pub(crate) async fn refresh_resumed(
        &self,
        app: &AppHandle,
        thread_id: &str,
    ) -> Result<Value, String> {
        let response = self
            .send(app, crate::codex::requests::thread_resume(thread_id))
            .await?;
        self.mark_resumed(app, thread_id, &response).await?;
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Sessions are numbered; each call to `session()` is a (re)spawn.
    fn sessions() -> (
        Arc<AtomicUsize>,
        impl FnMut() -> std::future::Ready<Result<usize, String>>,
    ) {
        let spawned = Arc::new(AtomicUsize::new(0));
        let counter = spawned.clone();
        (spawned, move || {
            std::future::ready(Ok(counter.fetch_add(1, Ordering::SeqCst) + 1))
        })
    }

    #[tokio::test]
    async fn the_response_comes_with_the_session_that_answered() {
        let (spawned, session) = sessions();
        // Session 1 dies before the write; the respawned session 2 answers.
        let (response, served) = served_by(session, |current: usize| async move {
            if current == 1 {
                Err(RequestError::NotSent("pipe closed".into()))
            } else {
                Ok(json!({"served": current}))
            }
        })
        .await
        .unwrap();
        assert_eq!(served, 2);
        assert_eq!(response["served"], 2);
        assert_eq!(spawned.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn a_request_that_may_have_run_is_not_resent() {
        let (spawned, session) = sessions();
        let error = served_by(session, |_: usize| async {
            Err::<Value, _>(RequestError::Failed("lost after write".into()))
        })
        .await
        .unwrap_err();
        assert_eq!(error, "lost after write");
        assert_eq!(spawned.load(Ordering::SeqCst), 1);
    }
}
