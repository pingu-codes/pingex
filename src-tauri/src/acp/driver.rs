//! One ACP agent process per active thread. `session/new` in most agents
//! (maki among them) closes whatever session the process held, so threads
//! never share a process.
//!
//! The agent picks the session id, so the thread row keeps it
//! (`native_session_id`) and a later process reopens it with `session/load`,
//! whose replay the translator's turn gate drops.

use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::AppHandle;

use super::permissions;
use super::rpc::{self, AcpConnection, RpcError, RpcSink, METHOD_NOT_FOUND};
use super::session::{self as acp, AgentInfo};
use super::spec::{AcpMode, AcpRuntime, AcpSpec, SpawnSettings, DEFAULT_MODEL};
use super::translate::{stop_reason, Translator};
use crate::codex::requests::TurnOptions;
use crate::codex::wire::WireLog;
use crate::harness::driver::{Driver, RequestAnswer, RequestIds};
use crate::harness::sink::EventSink;
use crate::harness::{HarnessEvent, HarnessKind, PermissionOption, StopReason};
use crate::storage::{self, HarnessThread};

const INITIALIZE_TIMEOUT: Duration = Duration::from_secs(30);
/// How long a cancelled prompt may take to answer before its process is
/// killed, which ends the turn either way.
const CANCEL_GRACE: Duration = Duration::from_secs(10);
/// Graceful exit for an idle process being replaced: closing stdin lets the
/// agent save and drop its session lock.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(2);
const MODELS_TTL: Duration = Duration::from_secs(600);

static PROCESS_SERIAL: AtomicU64 = AtomicU64::new(1);

/// A permission request waiting on the user.
struct Pending {
    process: u64,
    rpc_id: Value,
    options: Vec<PermissionOption>,
    connection: Arc<AcpConnection>,
}

type PendingMap = Arc<Mutex<HashMap<i64, Pending>>>;

/// What one process knows of its thread. Shared with the reader thread,
/// which delivers the agent's notifications and requests here.
struct ThreadState {
    serial: u64,
    thread_id: String,
    cwd: String,
    spec: &'static dyn AcpSpec,
    spawn: SpawnSettings,
    events: EventSink,
    translator: Mutex<Translator>,
    session_id: Mutex<Option<String>>,
    /// The ACP mode the session is in, as last reported.
    session_mode: Mutex<Option<String>>,
    model: Mutex<Option<String>>,
    info: Mutex<AgentInfo>,
    pending: PendingMap,
    ids: RequestIds,
    /// Held while a turn starts or ends, so neither interleaves with the
    /// reader's events.
    lifecycle: Mutex<()>,
}

impl ThreadState {
    fn turn_id(&self) -> Option<String> {
        self.translator.lock().ok().and_then(|t| t.turn_id.clone())
    }

    fn session_id(&self) -> Option<String> {
        self.session_id.lock().ok().and_then(|id| id.clone())
    }

    fn emit(&self, event: HarnessEvent) {
        self.events.emit(self.turn_id().as_deref(), event);
    }

    fn persist_mode(&self, mode: &str) {
        let Some(database) = crate::database_for(self.events.app(), self.events.home_key()) else {
            return;
        };
        let thread = self.thread_id.clone();
        let mode = mode.to_string();
        tauri::async_runtime::spawn(async move {
            let _ = storage::set_harness_permission_mode(&database, &thread, &mode, None).await;
        });
    }

    /// Answer every request still open on this process as cancelled, so the
    /// agent is not left waiting.
    fn cancel_pending(&self) {
        let mine: Vec<(i64, Pending)> = match self.pending.lock() {
            Ok(mut pending) => {
                let ids: Vec<i64> = pending
                    .iter()
                    .filter(|(_, p)| p.process == self.serial)
                    .map(|(id, _)| *id)
                    .collect();
                ids.into_iter()
                    .filter_map(|id| pending.remove(&id).map(|p| (id, p)))
                    .collect()
            }
            Err(_) => Vec::new(),
        };
        for (id, pending) in mine {
            let _ = pending
                .connection
                .respond(&pending.rpc_id, permissions::cancelled());
            self.emit(HarnessEvent::RequestCancelled { request_id: id });
        }
    }

    /// Close the turn: the prompt answered, failed, or the process died.
    fn end_turn(&self, stop: StopReason, error: Option<String>) {
        let _lifecycle = self.lifecycle.lock().unwrap_or_else(|e| e.into_inner());
        let ended = self
            .translator
            .lock()
            .ok()
            .and_then(|mut translator| translator.end_turn(stop, error));
        self.cancel_pending();
        if let Some((turn_id, events)) = ended {
            self.events.emit_all(Some(&turn_id), events);
        }
    }

    fn on_permission(&self, connection: &Arc<AcpConnection>, id: Value, params: &Value) {
        let tool_call = params.get("toolCall").cloned().unwrap_or(Value::Null);
        let (item_id, events, described) = match self.translator.lock() {
            Ok(mut translator) => {
                let (item_id, events) = translator.permission(&tool_call);
                let described = tool_call
                    .get("toolCallId")
                    .and_then(Value::as_str)
                    .and_then(|call| translator.describe(call));
                (item_id, events, described)
            }
            Err(_) => (String::new(), Vec::new(), None),
        };
        for event in events {
            self.emit(event);
        }
        let (name, content) = described.unwrap_or_else(|| ("tool".into(), Vec::new()));
        let request = permissions::request_for(params, &name, content, &self.cwd);
        let request_id = self.ids.next();
        if let Ok(mut pending) = self.pending.lock() {
            pending.insert(
                request_id,
                Pending {
                    process: self.serial,
                    rpc_id: id,
                    options: permissions::options(params),
                    connection: connection.clone(),
                },
            );
        }
        self.events.request(
            request_id,
            &self.turn_id().unwrap_or_default(),
            &item_id,
            request,
        );
    }
}

impl RpcSink for ThreadState {
    fn on_notification(&self, _: &Arc<AcpConnection>, method: &str, params: &Value) {
        if method != "session/update" {
            return;
        }
        let Some(update) = params.get("update") else {
            return;
        };
        match update.get("sessionUpdate").and_then(Value::as_str) {
            Some("current_mode_update") => {
                let Some(session_mode) = update.get("currentModeId").and_then(Value::as_str) else {
                    return;
                };
                if let Ok(mut slot) = self.session_mode.lock() {
                    *slot = Some(session_mode.to_string());
                }
                if let Some(mode) = self.spec.app_mode(session_mode, self.spawn.skips_prompts) {
                    self.persist_mode(mode.id);
                    self.emit(HarnessEvent::PermissionModeChanged {
                        mode: mode.id.to_string(),
                    });
                }
            }
            Some("config_option_update") => {
                if let Some(model) = acp::current_model(update) {
                    if let Ok(mut slot) = self.model.lock() {
                        *slot = Some(model);
                    }
                }
            }
            _ => {
                let events = match self.translator.lock() {
                    Ok(mut translator) => translator.update(update),
                    Err(_) => Vec::new(),
                };
                for event in events {
                    self.emit(event);
                }
            }
        }
    }

    fn on_request(&self, connection: &Arc<AcpConnection>, id: Value, method: &str, params: &Value) {
        if method == "session/request_permission" {
            self.on_permission(connection, id, params);
            return;
        }
        // No file system, terminals or forms are offered in `initialize`; an
        // agent asking anyway gets a refusal rather than silence.
        let _ = connection.respond_error(
            &id,
            METHOD_NOT_FOUND,
            &format!("{method} is not supported by this client"),
        );
    }

    fn on_closed(&self, reason: &str) {
        self.end_turn(StopReason::Error, Some(reason.to_string()));
    }
}

struct Process {
    connection: Arc<AcpConnection>,
    state: Arc<ThreadState>,
}

pub(crate) struct AcpDriver {
    spec: &'static dyn AcpSpec,
    home_key: String,
    runtime: Mutex<AcpRuntime>,
    wire: Arc<WireLog>,
    processes: Mutex<HashMap<String, Arc<Process>>>,
    pending: PendingMap,
    ids: RequestIds,
    process_start: tokio::sync::Mutex<()>,
    models: Mutex<Option<(Instant, Value)>>,
}

/// The message for a failed ACP request, with what the user can do about it.
fn describe_error(spec: &dyn AcpSpec, error: &RpcError) -> String {
    let label = spec.kind().label();
    let text = error.to_string();
    if error.code == -32000 && text.contains("another") {
        return format!(
            "This conversation is open in another {label} process. Close it there and try again."
        );
    }
    let lower = text.to_lowercase();
    if lower.contains("auth_required")
        || lower.contains("not authenticated")
        || lower.contains("no api key")
    {
        return format!(
            "{text}\nLog in from a terminal with `{} auth login`.",
            spec.default_binary()
        );
    }
    text
}

impl AcpDriver {
    pub(crate) fn new(
        spec: &'static dyn AcpSpec,
        home_key: String,
        runtime: AcpRuntime,
        wire: Arc<WireLog>,
        ids: RequestIds,
    ) -> Self {
        Self {
            spec,
            home_key,
            runtime: Mutex::new(runtime),
            wire,
            processes: Mutex::new(HashMap::new()),
            pending: Arc::new(Mutex::new(HashMap::new())),
            ids,
            process_start: tokio::sync::Mutex::new(()),
            models: Mutex::new(None),
        }
    }

    pub(crate) fn runtime(&self) -> AcpRuntime {
        self.runtime
            .lock()
            .map(|runtime| runtime.clone())
            .unwrap_or_else(|_| AcpRuntime::resolve(self.spec, None, Default::default()))
    }

    fn process(&self, thread_id: &str) -> Option<Arc<Process>> {
        self.processes
            .lock()
            .ok()
            .and_then(|processes| processes.get(thread_id).cloned())
            .filter(|process| process.connection.is_alive())
    }

    /// The mode a turn runs in: the one it names, else the thread's saved
    /// one, else the harness's default.
    fn mode_for(
        &self,
        thread: &HarnessThread,
        options: Option<&TurnOptions>,
    ) -> Result<&'static AcpMode, String> {
        let label = self.spec.kind().label();
        if let Some(requested) = options.and_then(|options| options.mode.as_ref()) {
            return self
                .spec
                .mode(&requested.mode)
                .ok_or_else(|| format!("{label} has no \"{}\" mode", requested.mode));
        }
        Ok(thread
            .permission_mode
            .as_deref()
            .and_then(|saved| self.spec.mode(saved))
            .unwrap_or(&self.spec.modes()[0]))
    }

    async fn spawn(
        &self,
        app: &AppHandle,
        thread: &HarnessThread,
        has_history: bool,
        spawn: SpawnSettings,
    ) -> Result<Arc<Process>, String> {
        let runtime = self.runtime();
        let label = self.spec.kind().label();
        let program = {
            let runtime = runtime.clone();
            let spec = self.spec;
            tauri::async_runtime::spawn_blocking(move || {
                runtime
                    .host
                    .resolve_binary(&runtime.binary_str())
                    .ok_or_else(|| super::spec::missing_message(spec, &runtime))
            })
            .await
            .map_err(|error| format!("Could not start {label}: {error}"))??
        };
        let state = Arc::new(ThreadState {
            serial: PROCESS_SERIAL.fetch_add(1, Ordering::SeqCst),
            thread_id: thread.thread_id.clone(),
            cwd: thread.cwd.clone(),
            spec: self.spec,
            spawn: spawn.clone(),
            events: EventSink::new(self.spec.kind(), &self.home_key, &thread.thread_id, app),
            translator: Mutex::new(Translator::new(thread.cwd.clone(), self.spec)),
            session_id: Mutex::new(None),
            session_mode: Mutex::new(None),
            model: Mutex::new(spawn.model.clone()),
            info: Mutex::new(AgentInfo::default()),
            pending: self.pending.clone(),
            ids: self.ids.clone(),
            lifecycle: Mutex::new(()),
        });
        let connection = rpc::spawn(
            &runtime.host,
            &program,
            &self.spec.args(&spawn),
            &thread.cwd,
            label,
            Some(app.clone()),
            self.wire.clone(),
            state.clone(),
        )?;
        let process = Arc::new(Process {
            connection: connection.clone(),
            state: state.clone(),
        });
        let opened = self.open_session(app, &process, thread, has_history).await;
        if let Err(error) = opened {
            connection.kill();
            return Err(error);
        }
        if let Ok(mut processes) = self.processes.lock() {
            processes.insert(thread.thread_id.clone(), process.clone());
        }
        Ok(process)
    }

    /// `initialize`, then a new session or the thread's earlier one.
    async fn open_session(
        &self,
        app: &AppHandle,
        process: &Process,
        thread: &HarnessThread,
        has_history: bool,
    ) -> Result<(), String> {
        let spec = self.spec;
        let label = spec.kind().label();
        let connection = &process.connection;
        let initialized = tokio::time::timeout(
            INITIALIZE_TIMEOUT,
            connection.request("initialize", acp::initialize()),
        )
        .await
        .map_err(|_| format!("{label} did not answer initialize"))?
        .map_err(|error| describe_error(spec, &error))?;
        let info = acp::agent_info(&initialized);
        if info.protocol_version != acp::PROTOCOL_VERSION {
            return Err(format!(
                "{label} speaks ACP version {}; this app speaks {}",
                info.protocol_version,
                acp::PROTOCOL_VERSION
            ));
        }
        if let Ok(mut slot) = process.state.info.lock() {
            *slot = info.clone();
        }
        let opened = match thread.native_session_id.as_deref() {
            Some(session_id) => {
                if !info.load_session {
                    return Err(format!("{label} cannot reopen an earlier session"));
                }
                let loaded = connection
                    .request("session/load", acp::load_session(session_id, &thread.cwd))
                    .await
                    .map_err(|error| {
                        if error.code == -32002 || error.to_string().contains("not found") {
                            format!("{label} no longer has this conversation's session, so it cannot continue here.")
                        } else {
                            describe_error(spec, &error)
                        }
                    })?;
                (session_id.to_string(), loaded)
            }
            None if has_history => {
                return Err(format!(
                    "This conversation has no {label} session to continue."
                ));
            }
            None => {
                let created = connection
                    .request("session/new", acp::new_session(&thread.cwd))
                    .await
                    .map_err(|error| describe_error(spec, &error))?;
                let session_id = created
                    .get("sessionId")
                    .and_then(Value::as_str)
                    .ok_or_else(|| format!("{label} returned no session id"))?
                    .to_string();
                if let Some(database) = crate::database_for(app, &self.home_key) {
                    storage::set_native_session_id(&database, &thread.thread_id, &session_id)
                        .await?;
                }
                (session_id, created)
            }
        };
        let (session_id, result) = opened;
        if let Ok(mut slot) = process.state.session_id.lock() {
            *slot = Some(session_id);
        }
        if let Ok(mut slot) = process.state.session_mode.lock() {
            *slot = acp::current_mode(&result);
        }
        if let Some(model) = acp::current_model(&result) {
            if let Ok(mut slot) = process.state.model.lock() {
                *slot = Some(model);
            }
        }
        Ok(())
    }

    /// The thread's process, set up for this turn's model and mode: reused
    /// when it can switch in place, restarted when only a new launch can.
    async fn ensure_process(
        &self,
        app: &AppHandle,
        thread: &HarnessThread,
        has_history: bool,
        mode: &AcpMode,
        model: Option<String>,
    ) -> Result<Arc<Process>, String> {
        let _starting = self.process_start.lock().await;
        let spawn = SpawnSettings {
            model: model.clone(),
            skips_prompts: mode.skips_prompts,
        };
        let mut reuse = None;
        let previous = self
            .processes
            .lock()
            .ok()
            .and_then(|processes| processes.get(&thread.thread_id).cloned());
        if let Some(previous) = previous {
            if previous.connection.is_alive() {
                if previous.state.turn_id().is_some() {
                    return Err("A turn is already running on this thread".into());
                }
                if previous.state.spawn.skips_prompts == mode.skips_prompts {
                    reuse = Some(previous);
                } else {
                    // Prompting is fixed at launch: the idle process goes,
                    // and its session reopens in one launched the other way.
                    previous.state.cancel_pending();
                    previous.connection.shutdown(SHUTDOWN_GRACE).await;
                }
            } else {
                previous.connection.wait_closed().await;
            }
        }
        let process = match reuse {
            Some(process) => process,
            None => {
                return self
                    .spawn_in_mode(app, thread, has_history, spawn, mode)
                    .await
            }
        };
        let session_id = process.state.session_id().unwrap_or_default();
        let current_model = process.state.model.lock().ok().and_then(|m| m.clone());
        if let Some(model) = model.filter(|model| Some(model) != current_model.as_ref()) {
            let switched = process
                .connection
                .request(
                    "session/set_config_option",
                    acp::set_model(&session_id, &model),
                )
                .await;
            match switched {
                Ok(_) => {
                    if let Ok(mut slot) = process.state.model.lock() {
                        *slot = Some(model);
                    }
                }
                Err(_) => {
                    // An agent that cannot switch in place gets `-m` on a
                    // fresh launch instead.
                    process.state.cancel_pending();
                    process.connection.shutdown(SHUTDOWN_GRACE).await;
                    return self
                        .spawn_in_mode(app, thread, has_history, spawn, mode)
                        .await;
                }
            }
        }
        self.apply_mode(&process, mode).await?;
        Ok(process)
    }

    async fn spawn_in_mode(
        &self,
        app: &AppHandle,
        thread: &HarnessThread,
        has_history: bool,
        spawn: SpawnSettings,
        mode: &AcpMode,
    ) -> Result<Arc<Process>, String> {
        let process = self.spawn(app, thread, has_history, spawn).await?;
        self.apply_mode(&process, mode).await?;
        Ok(process)
    }

    async fn apply_mode(&self, process: &Process, mode: &AcpMode) -> Result<(), String> {
        let current = process
            .state
            .session_mode
            .lock()
            .ok()
            .and_then(|m| m.clone());
        if current.as_deref() == Some(mode.session_mode) {
            return Ok(());
        }
        let session_id = process.state.session_id().unwrap_or_default();
        process
            .connection
            .request(
                "session/set_mode",
                acp::set_mode(&session_id, mode.session_mode),
            )
            .await
            .map_err(|error| describe_error(self.spec, &error))?;
        if let Ok(mut slot) = process.state.session_mode.lock() {
            *slot = Some(mode.session_mode.to_string());
        }
        Ok(())
    }

    fn take_pending(&self, request_id: i64) -> Option<Pending> {
        self.pending
            .lock()
            .ok()
            .and_then(|mut pending| pending.remove(&request_id))
    }
}

/// A turn's input as ACP content blocks: the text, then each image.
fn prompt_blocks(
    input: &[Value],
    host: &crate::util::host::Host,
    images: bool,
    label: &str,
) -> Result<(String, Vec<Value>), String> {
    let (text, content) = crate::claude::driver::user_content(input, host)?;
    let mut blocks = Vec::new();
    for block in content {
        match block.get("type").and_then(Value::as_str) {
            Some("text") => blocks.push(json!({"type": "text", "text": block["text"]})),
            Some("image") if !images => {
                return Err(format!("{label} does not accept images"));
            }
            Some("image") => blocks.push(json!({
                "type": "image",
                "mimeType": block["source"]["media_type"],
                "data": block["source"]["data"],
            })),
            _ => {}
        }
    }
    Ok((text, blocks))
}

#[async_trait]
impl Driver for AcpDriver {
    fn kind(&self) -> HarnessKind {
        self.spec.kind()
    }

    async fn start_turn(
        &self,
        app: &AppHandle,
        thread: &HarnessThread,
        has_history: bool,
        input: &[Value],
        options: Option<&TurnOptions>,
    ) -> Result<String, String> {
        let label = self.spec.kind().label();
        let mode = self.mode_for(thread, options)?;
        // The default entry means "no preference": keep whatever the process
        // runs rather than restarting it to let the agent choose again.
        let model = options
            .and_then(|options| options.model.clone())
            .filter(|model| !model.is_empty() && model != DEFAULT_MODEL);
        let process = self
            .ensure_process(app, thread, has_history, mode, model.clone())
            .await?;
        let state = process.state.clone();
        let images = state.info.lock().map(|info| info.images).unwrap_or(false);
        let (text, blocks) = prompt_blocks(input, &self.runtime().host, images, label)?;
        let session_id = state
            .session_id()
            .ok_or_else(|| format!("{label} has no open session"))?;
        let turn_id = uuid::Uuid::new_v4().to_string();
        {
            let _lifecycle = state
                .lifecycle
                .lock()
                .map_err(|_| format!("{label} lifecycle lock was poisoned"))?;
            if !process.connection.is_alive() {
                return Err(process.connection.exit_reason());
            }
            if state.turn_id().is_some() {
                return Err("A turn is already running on this thread".into());
            }
            let current_model = state.model.lock().ok().and_then(|m| m.clone());
            if let Ok(mut translator) = state.translator.lock() {
                translator.begin_turn(&turn_id, current_model.clone());
            }
            state.persist_mode(mode.id);
            state.emit(HarnessEvent::TurnStarted {
                turn_id: turn_id.clone(),
                model: current_model,
            });
            state.emit(HarnessEvent::UserMessage {
                item_id: format!("{turn_id}-user"),
                text,
            });
        }
        let connection = process.connection.clone();
        let spec = self.spec;
        tauri::async_runtime::spawn(async move {
            let result = connection
                .request("session/prompt", acp::prompt(&session_id, blocks))
                .await;
            match result {
                Ok(result) => state.end_turn(stop_reason(&result), None),
                // A dead process ends the turn through `on_closed`.
                Err(error) if error.code == rpc::CONNECTION_CLOSED => {}
                Err(error) => state.end_turn(StopReason::Error, Some(describe_error(spec, &error))),
            }
        });
        Ok(turn_id)
    }

    async fn interrupt(&self, thread_id: &str) -> Result<(), String> {
        let Some(process) = self.process(thread_id) else {
            return Ok(());
        };
        let Some(turn_id) = process.state.turn_id() else {
            return Ok(());
        };
        process.state.cancel_pending();
        let session_id = process.state.session_id().unwrap_or_default();
        process
            .connection
            .notify("session/cancel", acp::cancel(&session_id))?;
        // An agent that never answers the cancelled prompt would leave the
        // turn running forever; killing it ends the turn through `on_closed`.
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(CANCEL_GRACE).await;
            if process.state.turn_id().as_deref() == Some(turn_id.as_str()) {
                process.connection.kill();
            }
        });
        Ok(())
    }

    fn owns_request(&self, request_id: i64) -> bool {
        self.pending
            .lock()
            .map(|pending| pending.contains_key(&request_id))
            .unwrap_or(false)
    }

    async fn respond(&self, request_id: i64, answer: RequestAnswer) -> Result<(), String> {
        let label = self.spec.kind().label();
        let RequestAnswer::Option(answer) = answer else {
            return Err(format!(
                "{label} does not ask questions this app can answer"
            ));
        };
        let pending = self
            .take_pending(request_id)
            .ok_or("That request is no longer waiting")?;
        let Some(option) = permissions::choose(&pending.options, &answer) else {
            // Put it back: the card is still open and can be answered again.
            let options = pending
                .options
                .iter()
                .map(|o| o.option_id.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            if let Ok(mut map) = self.pending.lock() {
                map.insert(request_id, pending);
            }
            return Err(format!("\"{answer}\" is not one of {options}"));
        };
        let option = option.to_string();
        pending
            .connection
            .respond(&pending.rpc_id, permissions::selected(&option))
    }

    async fn models(&self) -> Result<Value, String> {
        if let Some((at, models)) = self.models.lock().ok().and_then(|m| m.clone()) {
            if at.elapsed() < MODELS_TTL {
                return Ok(models);
            }
        }
        let runtime = self.runtime();
        let spec = self.spec;
        let models = tauri::async_runtime::spawn_blocking(move || {
            let program = runtime
                .host
                .resolve_binary(&runtime.binary_str())
                .ok_or_else(|| super::spec::missing_message(spec, &runtime))?;
            spec.models(&runtime.host, &program)
        })
        .await
        .map_err(|error| error.to_string())??;
        if let Ok(mut slot) = self.models.lock() {
            *slot = Some((Instant::now(), models.clone()));
        }
        Ok(models)
    }

    fn active_threads(&self) -> Vec<String> {
        self.processes
            .lock()
            .map(|processes| {
                processes
                    .iter()
                    .filter(|(_, p)| p.connection.is_alive() && p.state.turn_id().is_some())
                    .map(|(id, _)| id.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn is_busy(&self) -> bool {
        self.processes
            .lock()
            .map(|processes| processes.values().any(|p| p.connection.is_alive()))
            .unwrap_or(true)
    }

    fn close_thread(&self, thread_id: &str) {
        let removed = self
            .processes
            .lock()
            .ok()
            .and_then(|mut processes| processes.remove(thread_id));
        if let Some(process) = removed {
            process.state.cancel_pending();
            process.connection.kill();
        }
    }

    fn kill_all(&self) {
        let Ok(mut processes) = self.processes.lock() else {
            return;
        };
        for (_, process) in processes.drain() {
            process.connection.kill();
        }
    }

    async fn forget_thread(&self, thread: &HarnessThread) {
        let Some(args) = thread
            .native_session_id
            .as_deref()
            .and_then(|id| self.spec.delete_session_args(id))
        else {
            return;
        };
        let runtime = self.runtime();
        let _ = tauri::async_runtime::spawn_blocking(move || {
            let Some(program) = runtime.host.resolve_binary(&runtime.binary_str()) else {
                return;
            };
            let argv: Vec<&str> = args.iter().map(String::as_str).collect();
            let _ = runtime
                .host
                .command(&program, &argv, None, &[], &[])
                .output();
        })
        .await;
    }
}
