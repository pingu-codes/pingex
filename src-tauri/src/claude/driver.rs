//! One `claude` process per active thread, translated into neutral events and
//! projected onto the Codex notification channel the app already renders.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::AppHandle;
use tauri_specta::Event;

use super::child::{self, ClaudeChild, FrameSink};
use super::modes::{ClaudeExecutionMode, ClaudeModeSettings, ModeStore};
use super::permissions;
use super::translate::Translator;
use crate::codex::events::{CodexEvent, CodexNotification};
use crate::codex::journal::TurnJournal;
use crate::codex::requests::TurnOptions;
use crate::codex::wire::WireLog;
use crate::harness::driver::{Driver, RequestAnswer, RequestIds};
use crate::harness::project::Projector;
use crate::harness::{
    version_below, HarnessEvent, HarnessEventEnvelope, HarnessKind, HarnessRequest,
    HarnessRequestEnvelope, HarnessStatus,
};
use crate::util::host::Host;
use crate::util::json::str_at;

/// The oldest CLI whose `--permission-prompt-tool stdio` exists.
pub(crate) const PROTOCOL_FLOOR: &str = "1.0.59";

/// Everything a permission prompt needs to be answered later.
#[derive(Clone)]
struct Pending {
    process_id: String,
    thread_id: String,
    request_id: String,
    can_use_tool: Value,
    child: Arc<ClaudeChild>,
}

type PendingMap = Arc<Mutex<HashMap<i64, Pending>>>;

/// What a thread's process was started with; a change mid-session goes out
/// as a control request. `mode` is `None` when the turn named no mode, which
/// keeps whatever the process runs in — the same sticky semantics Codex
/// gives a mode-less `turn/start`.
#[derive(Clone, Default, PartialEq, Debug)]
struct Settings {
    model: Option<String>,
    effort: Option<String>,
    mode: Option<String>,
    exit_plan_mode: Option<ClaudeExecutionMode>,
}

impl Settings {
    fn from_options(options: Option<&TurnOptions>) -> Result<Self, String> {
        let Some(options) = options else {
            return Ok(Self::default());
        };
        let plan = options
            .collaboration_mode
            .as_ref()
            .and_then(|mode| str_at(&mode.0, "mode"))
            == Some("plan");
        let auto = options.approvals_reviewer.as_deref() == Some("auto");
        let legacy_execution = match (
            options.approval_policy.as_deref(),
            options.sandbox_mode.as_deref(),
        ) {
            (Some("never"), _) | (_, Some("danger-full-access")) => {
                ClaudeExecutionMode::BypassPermissions
            }
            _ if auto => ClaudeExecutionMode::Auto,
            (_, Some("workspace-write")) => ClaudeExecutionMode::AcceptEdits,
            _ => ClaudeExecutionMode::Default,
        };
        let native = options
            .mode
            .as_ref()
            .map(|native| {
                ClaudeModeSettings::restore(Some(&native.mode), native.exit_plan_mode.as_deref())
            })
            .transpose()?;
        let (mode, exit_plan_mode) = match &native {
            Some(native) => {
                let mode = native.mode.as_str();
                (
                    mode,
                    ClaudeExecutionMode::from_mode(mode).unwrap_or(native.exit_plan_mode),
                )
            }
            None => (
                if plan {
                    "plan"
                } else {
                    legacy_execution.as_str()
                },
                legacy_execution,
            ),
        };
        Ok(Self {
            model: options.model.clone().filter(|m| !m.is_empty()),
            effort: options.effort.clone().filter(|e| !e.is_empty()),
            mode: Some(mode.to_string()),
            exit_plan_mode: Some(exit_plan_mode),
        })
    }

    /// The permission mode a fresh process starts in.
    fn mode_or_default(&self) -> &str {
        self.mode.as_deref().unwrap_or("default")
    }
}

struct ThreadSink {
    process_id: String,
    thread_id: String,
    home_key: String,
    app: AppHandle,
    seq: AtomicU64,
    translator: Mutex<Translator>,
    projector: Mutex<Projector>,
    journal: TurnJournal,
    pending: PendingMap,
    next_request: RequestIds,
    settings: Arc<Mutex<Settings>>,
    lifecycle: Mutex<()>,
    mode_store: Arc<ModeStore>,
}

impl ThreadSink {
    fn turn_id(&self) -> Option<String> {
        self.translator.lock().ok().and_then(|t| t.turn_id.clone())
    }

    /// Push one neutral event out: to `harness:event`, and projected onto the
    /// journal and `codex:event`, under the turn running now.
    fn emit(&self, event: HarnessEvent) {
        self.emit_in(self.turn_id(), event);
    }

    /// [`Self::emit`] under an explicit turn: a turn's closing events are
    /// produced after the translator has already let go of its id.
    fn emit_in(&self, turn_id: Option<String>, event: HarnessEvent) {
        if let HarnessEvent::PermissionModeChanged { mode } = &event {
            self.persist_mode(mode);
        }
        let seq = self.seq.fetch_add(1, Ordering::SeqCst);
        let _ = HarnessEventEnvelope {
            codex_home: self.home_key.clone(),
            harness: HarnessKind::Claude,
            thread_id: self.thread_id.clone(),
            turn_id: turn_id.clone(),
            seq,
            event: event.clone(),
        }
        .emit(&self.app);
        let turn = turn_id.unwrap_or_default();
        let notifications = match self.projector.lock() {
            Ok(mut projector) => projector.project(&self.thread_id, &turn, &event),
            Err(_) => Vec::new(),
        };
        for (method, params) in notifications {
            self.journal.observe(method, &params);
            let _ = CodexEvent {
                codex_home: self.home_key.clone(),
                event: CodexNotification::decode(method, &params),
            }
            .emit(&self.app);
        }
    }

    fn persist_mode(&self, mode: &str) {
        if let Some(database) = crate::database_for(&self.app, &self.home_key) {
            let exit_mode = self
                .settings
                .lock()
                .ok()
                .and_then(|s| s.exit_plan_mode)
                .map(ClaudeExecutionMode::as_str);
            self.mode_store
                .save(database, &self.thread_id, mode, exit_mode);
        }
    }

    fn emit_all(&self, turn_id: Option<String>, events: Vec<HarnessEvent>) {
        for event in events {
            let ended = matches!(event, HarnessEvent::TurnEnded { .. });
            self.emit_in(turn_id.clone(), event);
            if ended {
                self.deny_pending("Interrupted");
            }
        }
    }

    fn fail_turn(&self, reason: &str) {
        let turn_id = self
            .translator
            .lock()
            .ok()
            .and_then(|mut t| t.turn_id.take());
        self.deny_pending("Claude exited");
        if let Some(turn_id) = turn_id {
            self.emit_in(
                Some(turn_id.clone()),
                HarnessEvent::TurnEnded {
                    turn_id,
                    stop_reason: crate::harness::StopReason::Error,
                    error: Some(reason.into()),
                    duration_ms: None,
                    usage: None,
                },
            );
        }
    }

    /// Answer every prompt still open on this thread with a deny, so nothing
    /// is left blocking the process.
    fn deny_pending(&self, _reason: &str) {
        let mine: Vec<(i64, Pending)> = match self.pending.lock() {
            Ok(mut pending) => {
                let ids: Vec<i64> = pending
                    .iter()
                    .filter(|(_, p)| p.process_id == self.process_id)
                    .map(|(id, _)| *id)
                    .collect();
                ids.into_iter()
                    .filter_map(|id| pending.remove(&id).map(|p| (id, p)))
                    .collect()
            }
            Err(_) => Vec::new(),
        };
        for (id, pending) in mine {
            let _ = pending.child.respond_control(
                &pending.request_id,
                permissions::interrupted_result(&pending.can_use_tool),
            );
            self.emit(HarnessEvent::RequestCancelled { request_id: id });
        }
    }
}

impl FrameSink for ThreadSink {
    fn on_frame(&self, frame: &Value) {
        let _lifecycle = self.lifecycle.lock().unwrap_or_else(|e| e.into_inner());
        if str_at(frame, "type") == Some("system") {
            if let Some(mode) = str_at(frame, "permissionMode")
                .filter(|mode| *mode == "plan" || ClaudeExecutionMode::from_mode(mode).is_some())
            {
                if let Ok(mut settings) = self.settings.lock() {
                    settings.mode = Some(mode.to_string());
                    if let Some(execution) = ClaudeExecutionMode::from_mode(mode) {
                        settings.exit_plan_mode = Some(execution);
                    }
                }
                self.emit(HarnessEvent::PermissionModeChanged {
                    mode: mode.to_string(),
                });
            }
        }
        if str_at(frame, "type") == Some("control_cancel_request") {
            let Some(request_id) = str_at(frame, "request_id") else {
                return;
            };
            let cancelled = self.pending.lock().ok().and_then(|mut pending| {
                let id = pending
                    .iter()
                    .find(|(_, p)| p.process_id == self.process_id && p.request_id == request_id)
                    .map(|(id, _)| *id)?;
                pending.remove(&id);
                Some(id)
            });
            if let Some(id) = cancelled {
                self.emit(HarnessEvent::RequestCancelled { request_id: id });
            }
            return;
        }
        // The turn as it stood before this frame: a `result` frame ends the
        // turn inside `frame`, but its events still belong to it.
        let (turn_id, events) = match self.translator.lock() {
            Ok(mut translator) => translator.frame_in_turn(frame),
            Err(_) => (None, Vec::new()),
        };
        self.emit_all(turn_id, events);
    }

    fn on_control_request(&self, child: &Arc<ClaudeChild>, request_id: &str, request: &Value) {
        match str_at(request, "subtype") {
            Some("can_use_tool") => {
                let (cwd, host) = self
                    .translator
                    .lock()
                    .map(|t| (t.cwd.clone(), t.host.clone()))
                    .unwrap_or_default();
                let exit_mode = self
                    .settings
                    .lock()
                    .ok()
                    .and_then(|s| s.exit_plan_mode)
                    .unwrap_or_default();
                let harness_request =
                    permissions::request_for_mode(request, &cwd, &host, exit_mode);
                let id = self.next_request.next();
                if let Ok(mut pending) = self.pending.lock() {
                    pending.insert(
                        id,
                        Pending {
                            process_id: self.process_id.clone(),
                            thread_id: self.thread_id.clone(),
                            request_id: request_id.to_string(),
                            can_use_tool: request.clone(),
                            child: child.clone(),
                        },
                    );
                }
                let item_id = str_at(request, "tool_use_id").unwrap_or("").to_string();
                // A plan waiting for approval is worth reading in the
                // transcript, not only on the card.
                if matches!(&harness_request, HarnessRequest::Permission { name, .. } if name == "ExitPlanMode")
                {
                    let plan = request
                        .get("input")
                        .and_then(|input| str_at(input, "plan"))
                        .unwrap_or("")
                        .to_string();
                    self.emit(HarnessEvent::AgentMessageChunk {
                        item_id: format!("{item_id}-plan"),
                        text: plan,
                        done: true,
                    });
                }
                let _ = HarnessRequestEnvelope {
                    codex_home: self.home_key.clone(),
                    harness: HarnessKind::Claude,
                    request_id: id,
                    thread_id: self.thread_id.clone(),
                    turn_id: self.turn_id().unwrap_or_default(),
                    item_id,
                    request: harness_request,
                }
                .emit(&self.app);
            }
            // A hook we never registered, or an MCP bridge we do not host:
            // answer with no opinion so the CLI moves on.
            Some("hook_callback") => {
                let _ = child.respond_control(request_id, json!({}));
            }
            Some(other) => {
                let _ = child.respond_control_error(
                    request_id,
                    &format!("{other} is not supported by this client"),
                );
            }
            None => {
                let _ = child.respond_control_error(request_id, "malformed control request");
            }
        }
    }

    fn on_closed(&self, reason: &str) {
        let _lifecycle = self.lifecycle.lock().unwrap_or_else(|e| e.into_inner());
        self.fail_turn(reason);
    }
}

struct ThreadProcess {
    child: Arc<ClaudeChild>,
    sink: Arc<ThreadSink>,
    settings: Arc<Mutex<Settings>>,
    side_questions_unsupported: AtomicBool,
}

fn resolve_claude_binary(runtime: &ClaudeRuntime) -> Result<String, String> {
    runtime
        .host
        .resolve_binary(&runtime.binary_str())
        .ok_or_else(|| {
            format!(
                "{} Claude Code is needed for Claude threads.",
                missing_claude_message(&runtime.host, &runtime.binary_str())
            )
        })
}

/// Where the driver finds its binary and config directory, and on which host
/// both live.
#[derive(Clone)]
pub(crate) struct ClaudeRuntime {
    pub(crate) binary: PathBuf,
    /// A host path.
    pub(crate) config_dir: Option<PathBuf>,
    pub(crate) host: Host,
}

impl ClaudeRuntime {
    /// Resolve where `claude` lives and which config directory it should use.
    /// Environment wins over the saved settings override, which wins over the
    /// bare-`claude` / `~/.claude` defaults — the same precedence as the
    /// Codex runtime. The saved override is what makes a GUI launch (which
    /// inherits no shell exports) reach the same login as the terminal.
    pub(crate) fn resolve(overrides: &crate::settings::prefs::RuntimeOverrides) -> Self {
        let binary = std::env::var_os("PINGEX_CLAUDE_CLI_PATH")
            .map(PathBuf::from)
            .or_else(|| overrides.claude_binary.as_deref().map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("claude"));
        let host = overrides.claude_host.clone().unwrap_or_default();
        let config_dir = std::env::var_os("CLAUDE_CONFIG_DIR")
            .map(PathBuf::from)
            .filter(|_| host == Host::Native)
            .or_else(|| {
                overrides
                    .claude_config_dir
                    .as_deref()
                    .map(|dir| match &host {
                        Host::Native => crate::codex::binary::expand_tilde(dir),
                        // The distribution's shell expands `~` when the path is
                        // used; keep the text as typed.
                        Host::Wsl { .. } => PathBuf::from(dir),
                    })
            });
        Self {
            binary,
            config_dir,
            host,
        }
    }

    /// The directory Claude writes sessions under, as a host path.
    pub(crate) fn config_dir(&self) -> PathBuf {
        if let Some(dir) = &self.config_dir {
            return dir.clone();
        }
        match &self.host {
            Host::Native => dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("~"))
                .join(".claude"),
            Host::Wsl { .. } => PathBuf::from(self.host.join_str(
                &self.host.home_dir().unwrap_or_else(|| "~".to_string()),
                ".claude",
            )),
        }
    }

    pub(crate) fn config_dir_str(&self) -> String {
        self.config_dir().to_string_lossy().into_owned()
    }

    pub(crate) fn binary_str(&self) -> String {
        self.binary.to_string_lossy().into_owned()
    }
}

pub(crate) struct ClaudeDriver {
    home_key: String,
    runtime: Mutex<ClaudeRuntime>,
    wire: Arc<WireLog>,
    processes: Mutex<HashMap<String, Arc<ThreadProcess>>>,
    pending: PendingMap,
    next_request: RequestIds,
    pub(crate) side_questions: super::side_questions::SideRequests,
    process_start: tokio::sync::Mutex<()>,
    mode_store: Arc<ModeStore>,
}

/// [`crate::codex::binary::missing_message_on`], reworded for Claude.
fn missing_claude_message(host: &Host, binary: &str) -> String {
    crate::codex::binary::missing_message_on(host, binary)
        .replace("Codex CLI", "Claude Code CLI")
        .replace("codex binary", "claude binary")
        .replace("which codex", "which claude")
}

pub(crate) fn status(runtime: &ClaudeRuntime) -> HarnessStatus {
    let host = &runtime.host;
    let config_dir = runtime.config_dir_str();
    let Some(path) = host.resolve_binary(&runtime.binary_str()) else {
        return HarnessStatus {
            available: false,
            path: None,
            version: None,
            config_dir,
            logged_in: None,
            protocol_floor: PROTOCOL_FLOOR.into(),
            message: Some(missing_claude_message(host, &runtime.binary_str())),
        };
    };
    let logged_in = logged_in(host, &path, &config_dir);
    let version = host
        .command(&path, &["--version"], None, &[], &[])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .and_then(|line| line.split_whitespace().next().map(str::to_string));
    let below_floor = version
        .as_deref()
        .is_some_and(|version| version_below(version, PROTOCOL_FLOOR));
    HarnessStatus {
        available: !below_floor,
        path: Some(path),
        logged_in,
        message: below_floor.then(|| {
            format!(
                "Claude Code {} is older than {PROTOCOL_FLOOR}, the first version with the stdio permission prompt.",
                version.clone().unwrap_or_default()
            )
        }),
        version,
        config_dir,
        protocol_floor: PROTOCOL_FLOOR.into(),
    }
}

/// Whether the CLI at `path` is logged in when run against `config_dir`.
/// Tries `claude auth status` (newer CLIs); falls back to whether the
/// directory holds credentials at all. `None` means "could not tell" — on
/// macOS the OAuth token may live only in the Keychain.
fn logged_in(host: &Host, path: &str, config_dir: &str) -> Option<bool> {
    let output = host
        .command(
            path,
            &["auth", "status"],
            None,
            &[("CLAUDE_CONFIG_DIR", config_dir)],
            &child::UNSET_ENV,
        )
        .output()
        .ok()?;
    // `claude auth status` prints JSON with a `loggedIn` field and exits 0
    // whether or not a login exists, so only the field is trustworthy.
    if let Ok(status) = serde_json::from_slice::<Value>(&output.stdout) {
        if let Some(logged_in) = status.get("loggedIn").and_then(Value::as_bool) {
            return Some(logged_in);
        }
    }
    // An old CLI without the subcommand: fall back to the credentials file,
    // which only proves a login when it exists (macOS may use the Keychain).
    host.to_local(&host.join_str(config_dir, ".credentials.json"))
        .is_file()
        .then_some(true)
}

/// The Claude models the composer offers. Claude has no `model/list`; the
/// aliases the CLI accepts are the list.
pub(crate) fn models() -> Value {
    let efforts = |levels: &[&str]| -> Vec<Value> {
        levels
            .iter()
            .map(|level| json!({"reasoningEffort": level, "description": ""}))
            .collect()
    };
    let model = |id: &str, name: &str, description: &str, is_default: bool| {
        json!({
            "id": id,
            "model": id,
            "displayName": name,
            "description": description,
            "hidden": false,
            "supportedReasoningEfforts": efforts(&["low", "medium", "high", "xhigh", "max"]),
            "defaultReasoningEffort": "medium",
            "isDefault": is_default,
        })
    };
    json!({"data": [
        model("default", "Default", "Whatever the Claude CLI is configured to use", true),
        model("fable", "Fable", "Most capable", false),
        model("opus", "Opus", "Highly capable", false),
        model("sonnet", "Sonnet", "Balanced", false),
        model("haiku", "Haiku", "Fastest and cheapest", false),
    ]})
}

impl ClaudeDriver {
    pub(crate) fn new(
        home_key: String,
        runtime: ClaudeRuntime,
        wire: Arc<WireLog>,
        next_request: RequestIds,
    ) -> Self {
        Self {
            home_key,
            runtime: Mutex::new(runtime),
            wire,
            processes: Mutex::new(HashMap::new()),
            pending: Arc::new(Mutex::new(HashMap::new())),
            next_request,
            side_questions: Default::default(),
            process_start: tokio::sync::Mutex::new(()),
            mode_store: Arc::new(ModeStore::new()),
        }
    }

    pub(crate) fn runtime(&self) -> ClaudeRuntime {
        self.runtime
            .lock()
            .map(|runtime| runtime.clone())
            .unwrap_or_else(|_| ClaudeRuntime {
                binary: PathBuf::from("claude"),
                config_dir: None,
                host: Host::Native,
            })
    }

    /// Point the driver at a different binary/config dir. Running processes
    /// keep what they were spawned with; the next spawn uses this.
    pub(crate) fn set_runtime(&self, runtime: ClaudeRuntime) {
        if let Ok(mut slot) = self.runtime.lock() {
            *slot = runtime;
        }
    }

    pub(crate) async fn flush_modes(&self) {
        self.mode_store.flush().await;
    }

    async fn reusable_process(&self, thread_id: &str) -> Option<Arc<ThreadProcess>> {
        let previous = self
            .processes
            .lock()
            .ok()
            .and_then(|p| p.get(thread_id).cloned());
        if let Some(previous) = previous {
            if previous.child.is_alive() {
                return Some(previous);
            }
            previous.child.wait_closed().await;
        }
        None
    }

    fn process(&self, thread_id: &str) -> Option<Arc<ThreadProcess>> {
        self.processes
            .lock()
            .ok()
            .and_then(|processes| processes.get(thread_id).cloned())
            .filter(|process| process.child.is_alive())
    }

    /// `has_history`: the journal holds turns for this thread, so its Claude
    /// session must exist and be resumed.
    async fn spawn(
        &self,
        app: &AppHandle,
        thread_id: &str,
        cwd: &str,
        has_history: bool,
        settings: &Settings,
    ) -> Result<Arc<ThreadProcess>, String> {
        let runtime = self.runtime();
        // Binary lookup, `~` expansion and the session probe all touch the
        // host (login shells and the `\\wsl$` share on WSL), and this runs
        // under the driver-wide start lock: keep them off the async worker.
        let (program, config_dir, resume) = {
            let runtime = runtime.clone();
            let thread_id = thread_id.to_string();
            tauri::async_runtime::spawn_blocking(move || -> Result<_, String> {
                let program = resolve_claude_binary(&runtime)?;
                let config_dir =
                    super::sessions::config_path(&runtime.host, &runtime.config_dir_str())?;
                let resume = super::sessions::should_resume(
                    &runtime.host,
                    &config_dir,
                    &thread_id,
                    has_history,
                )?;
                Ok((program, config_dir, resume))
            })
            .await
            .map_err(|error| format!("Could not start Claude: {error}"))??
        };
        let args = turn_args(
            settings.model.as_deref(),
            settings.effort.as_deref(),
            settings.mode_or_default(),
            resume,
            thread_id,
        );
        let settings = Arc::new(Mutex::new(Settings {
            mode: Some(settings.mode_or_default().to_string()),
            ..settings.clone()
        }));
        let sink = Arc::new(ThreadSink {
            process_id: uuid::Uuid::new_v4().to_string(),
            thread_id: thread_id.to_string(),
            home_key: self.home_key.clone(),
            app: app.clone(),
            seq: AtomicU64::new(0),
            translator: Mutex::new(Translator::new(
                cwd.to_string(),
                config_dir.clone(),
                runtime.host.clone(),
            )),
            projector: Mutex::new(Projector::default()),
            journal: TurnJournal::new(app.clone(), self.home_key.clone(), HarnessKind::Claude),
            pending: self.pending.clone(),
            next_request: self.next_request.clone(),
            settings: settings.clone(),
            lifecycle: Mutex::new(()),
            mode_store: self.mode_store.clone(),
        });
        let child = child::spawn(
            &runtime.host,
            &program,
            cwd,
            &config_dir,
            &args,
            app.clone(),
            self.wire.clone(),
            sink.clone(),
        )?;
        let process = Arc::new(ThreadProcess {
            child,
            sink,
            side_questions_unsupported: AtomicBool::new(false),
            settings,
        });
        if let Ok(mut processes) = self.processes.lock() {
            processes.insert(thread_id.to_string(), process.clone());
        }
        Ok(process)
    }

    async fn ensure_process(
        &self,
        app: &AppHandle,
        thread_id: &str,
        cwd: &str,
        resume: bool,
        settings: &Settings,
    ) -> Result<Arc<ThreadProcess>, String> {
        let _starting = self.process_start.lock().await;
        if let Some(process) = self.reusable_process(thread_id).await {
            // Refuse before touching anything: switching model or permission
            // mode here would change the turn that is already running, even
            // though `start_turn` then rejects this send.
            if process.sink.turn_id().is_some() {
                return Err("A turn is already running on this thread".into());
            }
            let current = process
                .settings
                .lock()
                .map(|s| s.clone())
                .unwrap_or_default();
            if current.effort != settings.effort {
                // `--effort` is fixed when the CLI starts and there is no
                // control request for it, so an idle process is restarted
                // under the same session; the conversation resumes from its
                // transcript, exactly as after an app restart.
                process.sink.deny_pending("Restarting");
                process.child.kill();
                process.child.wait_closed().await;
                let restarted = Settings {
                    mode: settings.mode.clone().or(current.mode),
                    exit_plan_mode: settings.exit_plan_mode.or(current.exit_plan_mode),
                    ..settings.clone()
                };
                return self.spawn(app, thread_id, cwd, resume, &restarted).await;
            }
            drop(_starting);
            if current.model != settings.model {
                process
                    .child
                    .control(json!({"subtype": "set_model", "model": settings.model}))
                    .await?;
            }
            if let Some(mode) = &settings.mode {
                if current.mode.as_deref() != Some(mode.as_str()) {
                    process
                        .child
                        .control(json!({"subtype": "set_permission_mode", "mode": mode}))
                        .await?;
                }
            }
            if let Ok(mut slot) = process.settings.lock() {
                *slot = Settings {
                    mode: settings.mode.clone().or(current.mode),
                    exit_plan_mode: settings.exit_plan_mode.or(current.exit_plan_mode),
                    ..settings.clone()
                };
            }
            return Ok(process);
        }
        self.spawn(app, thread_id, cwd, resume, settings).await
    }

    /// Native /btw uses the session's context without opening a chat turn.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn ask_side_question(
        &self,
        app: &AppHandle,
        thread_id: &str,
        cwd: &str,
        resume: bool,
        saved: Option<&crate::storage::TurnSettings>,
        saved_modes: &ClaudeModeSettings,
        request_id: &str,
        question: &str,
        history: Vec<crate::storage::native_side_questions::SideExchange>,
        mut cancellation: tokio::sync::watch::Receiver<bool>,
    ) -> Result<crate::harness::SideAnswer, String> {
        let wire_id = format!("btw-{request_id}");
        let answer = async {
            let (process, initialize) = {
                let _starting = self.process_start.lock().await;
                match self.reusable_process(thread_id).await {
                    Some(process) => (process, false),
                    None => {
                        let settings = Settings {
                            model: saved.and_then(|s| s.model.clone()),
                            effort: saved.and_then(|s| s.reasoning_effort.clone()),
                            mode: Some(saved_modes.mode.as_str().to_string()),
                            exit_plan_mode: Some(saved_modes.exit_plan_mode),
                        };
                        (
                            self.spawn(app, thread_id, cwd, resume, &settings).await?,
                            true,
                        )
                    }
                }
            };
            if initialize {
                process
                    .child
                    .control(json!({"subtype": "initialize"}))
                    .await?;
            }
            if process.side_questions_unsupported.load(Ordering::SeqCst) {
                return Err("This Claude CLI does not support side questions. Update Claude Code and reopen the thread.".into());
            }
            let result = process
                .child
                .control_identified(&wire_id, super::side_questions::request(question, &history))
                .await;
            if let Err(error) = &result {
                let error = error.to_lowercase();
                if error.contains("unknown control request")
                    || error.contains("unsupported control request")
                {
                    process
                        .side_questions_unsupported
                        .store(true, Ordering::SeqCst);
                    return Err("This Claude CLI does not support side questions. Update Claude Code and reopen the thread.".into());
                }
            }
            result.and_then(super::side_questions::answer)
        };
        tokio::select! {
            biased;
            _ = super::side_questions::cancelled(&mut cancellation) => {
                if let Some(process) = self.process(thread_id) { let _ = process.child.cancel_control(&wire_id); }
                Err("Side question cancelled".into())
            }
            result = tokio::time::timeout(std::time::Duration::from_secs(600), answer) => {
                match result {
                    Ok(result) => result,
                    Err(_) => {
                        if let Some(process) = self.process(thread_id) { let _ = process.child.cancel_control(&wire_id); }
                        Err("Side question timed out. Try again.".into())
                    }
                }
            }
        }
    }

    /// Send a prompt. `resume` says the thread has history on disk from an
    /// earlier process, so the CLI must `--resume` rather than start afresh.
    /// Returns the new turn's id.
    pub(crate) async fn start_turn(
        &self,
        app: &AppHandle,
        thread_id: &str,
        cwd: &str,
        resume: bool,
        input: &[Value],
        options: Option<&TurnOptions>,
    ) -> Result<String, String> {
        let settings = Settings::from_options(options)?;
        // Read attachments before spawning or locking anything: a bad image
        // fails the send up front instead of being dropped silently.
        let (text, content) = user_content(input, &self.runtime().host)?;
        let process = self
            .ensure_process(app, thread_id, cwd, resume, &settings)
            .await?;
        let _lifecycle = process
            .sink
            .lifecycle
            .lock()
            .map_err(|_| "Claude lifecycle lock was poisoned")?;
        if !process.child.is_alive() {
            return Err(process.child.exit_reason());
        }
        if process.sink.turn_id().is_some() {
            return Err("A turn is already running on this thread".into());
        }
        let turn_id = uuid::Uuid::new_v4().to_string();
        let frame = json!({
            "type": "user",
            "session_id": "",
            "parent_tool_use_id": null,
            "uuid": turn_id,
            "origin": {"kind": "human"},
            "message": {"role": "user", "content": content},
        });
        if let Ok(mut translator) = process.sink.translator.lock() {
            translator.turn_id = Some(turn_id.clone());
        }
        if let Some(mode) = process.settings.lock().ok().and_then(|s| s.mode.clone()) {
            process.sink.persist_mode(&mode);
        }
        process.sink.emit(HarnessEvent::TurnStarted {
            turn_id: turn_id.clone(),
            model: settings.model.clone(),
        });
        process.sink.emit(HarnessEvent::UserMessage {
            item_id: format!("{turn_id}-user"),
            text,
        });
        if let Err(error) = process.child.write_frame(&frame) {
            process.child.kill();
            let tail = process.child.stderr_tail();
            let error = if tail.is_empty() {
                error
            } else {
                format!("{error}\n{tail}")
            };
            process.sink.fail_turn(&error);
            return Err(error);
        }
        Ok(turn_id)
    }

    fn take_pending(&self, request_id: i64) -> Option<Pending> {
        self.pending
            .lock()
            .ok()
            .and_then(|mut pending| pending.remove(&request_id))
    }

    /// Answer a permission prompt with one of its option ids (or a Codex
    /// decision word).
    pub(crate) fn respond_option(&self, request_id: i64, option_id: &str) -> Result<(), String> {
        let pending = self
            .pending
            .lock()
            .map_err(|_| "Claude pending lock was poisoned")?
            .get(&request_id)
            .cloned()
            .ok_or("That request is no longer waiting")?;
        let process = self.process(&pending.thread_id);
        let _lifecycle = process.as_ref().map(|process| {
            process
                .sink
                .lifecycle
                .lock()
                .unwrap_or_else(|error| error.into_inner())
        });
        // Older cards used a generic Implement decision. Resolve it against
        // the same remembered mode as the explicitly labelled native choices.
        let option = if option_id == permissions::PLAN_IMPLEMENT {
            let mode = process
                .as_ref()
                .and_then(|p| p.settings.lock().ok().and_then(|s| s.exit_plan_mode))
                .unwrap_or_default();
            format!("plan_mode_{}", mode.as_str())
        } else {
            option_id.to_string()
        };
        pending.child.respond_control(
            &pending.request_id,
            permissions::permission_result(&option, &pending.can_use_tool),
        )?;
        self.take_pending(request_id);
        // Implementing a plan answers with `setMode`, which moves the CLI out
        // of plan mode. Record it, or the next turn's diff would think the
        // process is still planning; and say so, or the composer's pill would.
        if let Some(mode) = permissions::mode_after(&option)
            .filter(|_| str_at(&pending.can_use_tool, "tool_name") == Some("ExitPlanMode"))
        {
            if let Some(process) = process.as_ref() {
                if let Ok(mut slot) = process.settings.lock() {
                    slot.mode = Some(mode.to_string());
                    slot.exit_plan_mode = ClaudeExecutionMode::from_mode(mode);
                }
                process
                    .sink
                    .emit(HarnessEvent::PermissionModeChanged { mode: mode.into() });
            }
        }
        Ok(())
    }

    /// Answer an `AskUserQuestion` with Codex-shaped answers.
    pub(crate) fn respond_user_input(
        &self,
        request_id: i64,
        answers: &Value,
    ) -> Result<(), String> {
        let pending = self
            .take_pending(request_id)
            .ok_or("That question is no longer waiting")?;
        let input = pending
            .can_use_tool
            .get("input")
            .cloned()
            .unwrap_or(Value::Null);
        let tool_use_id = str_at(&pending.can_use_tool, "tool_use_id").unwrap_or("");
        pending.child.respond_control(
            &pending.request_id,
            permissions::user_input_result(&input, tool_use_id, answers),
        )
    }
}

#[async_trait::async_trait]
impl Driver for ClaudeDriver {
    fn kind(&self) -> HarnessKind {
        HarnessKind::Claude
    }

    async fn start_turn(
        &self,
        app: &AppHandle,
        thread: &crate::storage::HarnessThread,
        has_history: bool,
        input: &[Value],
        options: Option<&TurnOptions>,
    ) -> Result<String, String> {
        let turn_id = ClaudeDriver::start_turn(
            self,
            app,
            &thread.thread_id,
            &thread.cwd,
            has_history,
            input,
            options,
        )
        .await?;
        self.flush_modes().await;
        Ok(turn_id)
    }

    async fn interrupt(&self, thread_id: &str) -> Result<(), String> {
        let Some(process) = self.process(thread_id) else {
            return Ok(());
        };
        process.sink.deny_pending("Interrupted");
        // The receipt only exists on newer CLIs; the turn's `result` is what
        // actually ends it either way.
        let _ = process.child.control(json!({"subtype": "interrupt"})).await;
        Ok(())
    }

    fn owns_request(&self, request_id: i64) -> bool {
        self.pending
            .lock()
            .map(|pending| pending.contains_key(&request_id))
            .unwrap_or(false)
    }

    async fn respond(&self, request_id: i64, answer: RequestAnswer) -> Result<(), String> {
        match answer {
            RequestAnswer::Option(option) => {
                self.respond_option(request_id, &option)?;
                self.flush_modes().await;
                Ok(())
            }
            RequestAnswer::UserInput(answers) => self.respond_user_input(request_id, &answers),
        }
    }

    async fn models(&self) -> Result<Value, String> {
        Ok(models())
    }

    fn active_threads(&self) -> Vec<String> {
        self.processes
            .lock()
            .map(|processes| {
                processes
                    .iter()
                    .filter(|(_, p)| p.child.is_alive() && p.sink.turn_id().is_some())
                    .map(|(id, _)| id.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn is_busy(&self) -> bool {
        self.processes
            .lock()
            .map(|processes| processes.values().any(|process| process.child.is_alive()))
            .unwrap_or(true)
    }

    fn close_thread(&self, thread_id: &str) {
        let removed = self
            .processes
            .lock()
            .ok()
            .and_then(|mut processes| processes.remove(thread_id));
        if let Some(process) = removed {
            process.sink.deny_pending("Closed");
            process.child.kill();
        }
    }

    fn kill_all(&self) {
        let Ok(mut processes) = self.processes.lock() else {
            return;
        };
        for (_, process) in processes.drain() {
            process.child.kill();
        }
    }

    async fn flush(&self) {
        self.flush_modes().await;
    }

    /// `/compact` runs as a turn and lands as a `compact_boundary`, which the
    /// projector turns into the same `thread/compacted` Codex sends.
    async fn compact(
        &self,
        app: &AppHandle,
        thread: &crate::storage::HarnessThread,
        has_history: bool,
    ) -> Result<String, String> {
        let saved = ClaudeModeSettings::restore(
            thread.permission_mode.as_deref(),
            thread.plan_exit_mode.as_deref(),
        )?;
        let options = Some(TurnOptions {
            mode: Some(crate::codex::requests::HarnessMode {
                mode: saved.mode.as_str().to_string(),
                exit_plan_mode: Some(saved.exit_plan_mode.as_str().to_string()),
            }),
            ..Default::default()
        });
        ClaudeDriver::start_turn(
            self,
            app,
            &thread.thread_id,
            &thread.cwd,
            has_history,
            &[json!({"type": "text", "text": "/compact"})],
            options.as_ref(),
        )
        .await
    }

    /// Rename the session on the Claude side too, so `claude --resume` by
    /// name works from a terminal.
    async fn rename(&self, thread_id: &str, title: &str) {
        if let Some(process) = self.process(thread_id) {
            let _ = process
                .child
                .control(json!({"subtype": "rename_session", "title": title}))
                .await;
        }
    }

    /// Ask a live thread's process what its context holds
    /// (`get_context_usage`). `None` between processes.
    async fn context_usage(&self, thread_id: &str) -> Result<Option<Value>, String> {
        let Some(process) = self.process(thread_id) else {
            return Ok(None);
        };
        process
            .child
            .control(json!({"subtype": "get_context_usage"}))
            .await
            .map(Some)
    }
}

/// The per-session arguments `spawn` appends after [`child::BASE_ARGS`]:
/// session identity, model/effort, and the permission mode. Public so the
/// live e2e suite replays exactly what the app sends.
pub fn turn_args(
    model: Option<&str>,
    effort: Option<&str>,
    mode: &str,
    resume: bool,
    thread_id: &str,
) -> Vec<String> {
    let mut args: Vec<String> = if resume {
        vec!["--resume".into(), thread_id.into()]
    } else {
        vec!["--session-id".into(), thread_id.into()]
    };
    if let Some(model) = model {
        args.extend(["--model".into(), model.into()]);
    }
    if let Some(effort) = effort {
        args.extend(["--effort".into(), effort.into()]);
    }
    args.extend(["--permission-mode".into(), mode.into()]);
    // Always allowed, whatever the starting mode: the CLI refuses a later
    // `set_permission_mode` to bypassPermissions on a process launched
    // without it, so a thread started under Ask could never switch to Bypass.
    args.push("--allow-dangerously-skip-permissions".into());
    args
}

/// The largest image sent inline; matches the composer's attachment cap.
const MAX_IMAGE_BYTES: u64 = 20 * 1024 * 1024;

/// A turn's input as the CLI's `user` message content: the text parts joined
/// into one block, then each image inline. Returns the joined text too, for
/// the transcript. Images arrive as the composer's `localImage` (a host path
/// of a staged file) or a protocol `image` with a `file://` url or path.
pub(crate) fn user_content(input: &[Value], host: &Host) -> Result<(String, Vec<Value>), String> {
    let text = input
        .iter()
        .filter(|part| str_at(part, "type") == Some("text"))
        .filter_map(|part| str_at(part, "text"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut content = vec![json!({"type": "text", "text": text})];
    for part in input {
        if !matches!(str_at(part, "type"), Some("image" | "localImage")) {
            continue;
        }
        let Some(path) = str_at(part, "path").or_else(|| str_at(part, "url")) else {
            continue;
        };
        let path = path.strip_prefix("file://").unwrap_or(path);
        content.push(image_block(&host.to_local(path))?);
    }
    Ok((text, content))
}

/// A local image as a base64 content block.
fn image_block(path: &std::path::Path) -> Result<Value, String> {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let media_type = match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        _ => {
            return Err(format!(
                "{name}: only PNG, JPEG, GIF or WebP images can be sent"
            ))
        }
    };
    let size = std::fs::metadata(path)
        .map_err(|error| format!("Could not read image {name}: {error}"))?
        .len();
    if size > MAX_IMAGE_BYTES {
        return Err(format!("{name} is larger than 20 MB"));
    }
    let bytes =
        std::fs::read(path).map_err(|error| format!("Could not read image {name}: {error}"))?;
    Ok(json!({
        "type": "image",
        "source": {"type": "base64", "media_type": media_type, "data": base64_encode(&bytes)},
    }))
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_modes_override_legacy_presets_and_retain_the_plan_exit_mode() {
        for mode in ClaudeExecutionMode::ALL {
            let options: TurnOptions = serde_json::from_value(json!({
                "mode": {"mode":"plan", "exitPlanMode":mode.as_str()},
                "approvalPolicy":"never", "sandboxMode":"danger-full-access"
            }))
            .unwrap();
            let settings = Settings::from_options(Some(&options)).unwrap();
            assert_eq!(settings.mode.as_deref(), Some("plan"));
            assert_eq!(settings.exit_plan_mode, Some(mode));
            let options: TurnOptions = serde_json::from_value(json!({
                "mode": {"mode":mode.as_str(), "exitPlanMode":"default"},
                "collaborationMode": {"mode":"plan"}
            }))
            .unwrap();
            let settings = Settings::from_options(Some(&options)).unwrap();
            assert_eq!(settings.mode.as_deref(), Some(mode.as_str()));
            assert_eq!(settings.exit_plan_mode, Some(mode));
        }
        for refused in [
            json!({"mode":{"mode":"unknown", "exitPlanMode":"default"}}),
            json!({"mode":{"mode":"plan", "exitPlanMode":"plan"}}),
            json!({"mode":{"mode":"build"}}),
        ] {
            let options: TurnOptions = serde_json::from_value(refused).unwrap();
            assert!(Settings::from_options(Some(&options)).is_err());
        }
    }

    #[test]
    fn presets_map_onto_permission_modes() {
        let options = |approval: &str, sandbox: &str| TurnOptions {
            approval_policy: Some(approval.into()),
            sandbox_mode: Some(sandbox.into()),
            ..Default::default()
        };
        assert_eq!(
            Settings::from_options(Some(&options("on-request", "read-only")))
                .unwrap()
                .mode
                .as_deref(),
            Some("default")
        );
        assert_eq!(
            Settings::from_options(Some(&options("on-request", "workspace-write")))
                .unwrap()
                .mode
                .as_deref(),
            Some("acceptEdits")
        );
        assert_eq!(
            Settings::from_options(Some(&options("never", "danger-full-access")))
                .unwrap()
                .mode
                .as_deref(),
            Some("bypassPermissions")
        );
        let auto = TurnOptions {
            approvals_reviewer: Some("auto".into()),
            ..options("on-request", "workspace-write")
        };
        assert_eq!(
            Settings::from_options(Some(&auto)).unwrap().mode.as_deref(),
            Some("auto")
        );
        // A mode-less turn keeps whatever the process runs in.
        assert_eq!(Settings::from_options(None).unwrap().mode, None);
        assert_eq!(
            Settings::from_options(None).unwrap().mode_or_default(),
            "default"
        );
    }

    #[test]
    fn every_process_can_switch_to_bypass_later() {
        for mode in ["default", "acceptEdits", "plan", "bypassPermissions"] {
            let args = turn_args(None, None, mode, false, "t");
            assert!(
                args.iter()
                    .any(|arg| arg == "--allow-dangerously-skip-permissions"),
                "{mode}"
            );
        }
    }

    #[test]
    fn user_content_inlines_the_composers_local_images() {
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("shot.PNG");
        std::fs::write(&image, b"hi").unwrap();
        let input = [
            json!({"type": "text", "text": "look"}),
            json!({"type": "localImage", "path": image.to_string_lossy()}),
            json!({"type": "text", "text": "here"}),
        ];
        let (text, content) = user_content(&input, &Host::Native).unwrap();
        assert_eq!(text, "look\nhere");
        assert_eq!(content.len(), 2);
        assert_eq!(content[1]["source"]["media_type"], "image/png");
        assert_eq!(content[1]["source"]["data"], "aGk=");
    }

    #[test]
    fn user_content_refuses_images_it_cannot_send() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("gone.png");
        let error = user_content(
            &[json!({"type": "localImage", "path": missing.to_string_lossy()})],
            &Host::Native,
        )
        .unwrap_err();
        assert!(error.contains("gone.png"), "{error}");
        let bmp = dir.path().join("old.bmp");
        std::fs::write(&bmp, b"x").unwrap();
        let error = user_content(
            &[json!({"type": "image", "url": format!("file://{}", bmp.display())})],
            &Host::Native,
        )
        .unwrap_err();
        assert!(error.contains("PNG, JPEG"), "{error}");
    }

    #[test]
    fn plan_mode_wins_over_the_preset() {
        let options = TurnOptions {
            approval_policy: Some("never".into()),
            collaboration_mode: Some(crate::util::json::Json(json!({"mode": "plan"}))),
            ..Default::default()
        };
        assert_eq!(
            Settings::from_options(Some(&options))
                .unwrap()
                .mode
                .as_deref(),
            Some("plan")
        );
    }

    #[test]
    fn the_floor_is_compared_numerically() {
        assert!(version_below("1.0.58", PROTOCOL_FLOOR));
        assert!(!version_below("1.0.59", PROTOCOL_FLOOR));
        assert!(!version_below("2.1.251", PROTOCOL_FLOOR));
        assert!(!version_below("10.0.0", "9.9.9"));
    }

    #[test]
    fn base64_matches_the_standard_alphabet() {
        assert_eq!(base64_encode(b"hi"), "aGk=");
        assert_eq!(base64_encode(b"hello"), "aGVsbG8=");
    }
}
