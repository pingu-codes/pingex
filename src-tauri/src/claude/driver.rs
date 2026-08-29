//! One `claude` process per active thread, translated into neutral events and
//! projected onto the Codex notification channel the app already renders.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::AppHandle;
use tauri_specta::Event;

use super::child::{self, ClaudeChild, FrameSink};
use super::permissions;
use super::translate::Translator;
use crate::codex::events::{CodexEvent, CodexNotification};
use crate::codex::journal::TurnJournal;
use crate::codex::requests::TurnOptions;
use crate::codex::wire::WireLog;
use crate::harness::project::Projector;
use crate::harness::{
    HarnessEvent, HarnessEventEnvelope, HarnessKind, HarnessRequest, HarnessRequestEnvelope,
};
use crate::util::json::str_at;

/// Request ids handed to the frontend for Claude prompts start here, so they
/// never collide with the app-server's JSON-RPC ids.
const REQUEST_ID_BASE: i64 = 1 << 40;

/// The oldest CLI whose `--permission-prompt-tool stdio` exists.
pub(crate) const PROTOCOL_FLOOR: &str = "1.0.59";

/// Everything a permission prompt needs to be answered later.
struct Pending {
    thread_id: String,
    request_id: String,
    can_use_tool: Value,
    child: Arc<ClaudeChild>,
}

type PendingMap = Arc<Mutex<HashMap<i64, Pending>>>;

/// What a thread's process was started with; a change mid-session goes out
/// as a control request.
#[derive(Clone, Default, PartialEq)]
struct Settings {
    model: Option<String>,
    effort: Option<String>,
    mode: String,
}

impl Settings {
    fn from_options(options: Option<&TurnOptions>) -> Self {
        let Some(options) = options else {
            return Self {
                mode: "default".into(),
                ..Default::default()
            };
        };
        let plan = options
            .collaboration_mode
            .as_ref()
            .and_then(|mode| str_at(&mode.0, "mode"))
            == Some("plan");
        let mode = if plan {
            "plan"
        } else {
            match (
                options.approval_policy.as_deref(),
                options.sandbox_mode.as_deref(),
            ) {
                (Some("never"), _) | (_, Some("danger-full-access")) => "bypassPermissions",
                (_, Some("workspace-write")) => "acceptEdits",
                _ => "default",
            }
        };
        Self {
            model: options.model.clone().filter(|m| !m.is_empty()),
            effort: options.effort.clone().filter(|e| !e.is_empty()),
            mode: mode.to_string(),
        }
    }
}

struct ThreadSink {
    thread_id: String,
    home_key: String,
    app: AppHandle,
    seq: AtomicU64,
    translator: Mutex<Translator>,
    projector: Mutex<Projector>,
    journal: TurnJournal,
    pending: PendingMap,
    next_request: Arc<AtomicI64>,
}

impl ThreadSink {
    fn turn_id(&self) -> Option<String> {
        self.translator.lock().ok().and_then(|t| t.turn_id.clone())
    }

    /// Push one neutral event out: to `harness:event`, and projected onto the
    /// journal and `codex:event`.
    fn emit(&self, event: HarnessEvent) {
        let turn_id = self.turn_id();
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

    fn emit_all(&self, events: Vec<HarnessEvent>) {
        for event in events {
            let ended = matches!(event, HarnessEvent::TurnEnded { .. });
            self.emit(event);
            if ended {
                self.deny_pending("Interrupted");
            }
        }
    }

    /// Answer every prompt still open on this thread with a deny, so nothing
    /// is left blocking the process.
    fn deny_pending(&self, _reason: &str) {
        let mine: Vec<(i64, Pending)> = match self.pending.lock() {
            Ok(mut pending) => {
                let ids: Vec<i64> = pending
                    .iter()
                    .filter(|(_, p)| p.thread_id == self.thread_id)
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
        if str_at(frame, "type") == Some("control_cancel_request") {
            let Some(request_id) = str_at(frame, "request_id") else {
                return;
            };
            let cancelled = self.pending.lock().ok().and_then(|mut pending| {
                let id = pending
                    .iter()
                    .find(|(_, p)| p.request_id == request_id)
                    .map(|(id, _)| *id)?;
                pending.remove(&id);
                Some(id)
            });
            if let Some(id) = cancelled {
                self.emit(HarnessEvent::RequestCancelled { request_id: id });
            }
            return;
        }
        let events = match self.translator.lock() {
            Ok(mut translator) => translator.frame(frame),
            Err(_) => Vec::new(),
        };
        self.emit_all(events);
    }

    fn on_control_request(&self, child: &Arc<ClaudeChild>, request_id: &str, request: &Value) {
        match str_at(request, "subtype") {
            Some("can_use_tool") => {
                let cwd = self
                    .translator
                    .lock()
                    .map(|t| t.cwd.clone())
                    .unwrap_or_default();
                let harness_request = permissions::request_for(request, &cwd);
                let id = self.next_request.fetch_add(1, Ordering::SeqCst);
                if let Ok(mut pending) = self.pending.lock() {
                    pending.insert(
                        id,
                        Pending {
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

    fn on_closed(&self) {
        // A turn cut off by the process dying never gets its `result`.
        let open = self.turn_id();
        if let Some(turn_id) = open {
            if let Ok(mut translator) = self.translator.lock() {
                translator.turn_id = None;
            }
            self.deny_pending("Claude exited");
            self.emit(HarnessEvent::TurnEnded {
                turn_id,
                stop_reason: crate::harness::StopReason::Error,
                error: Some("Claude exited before finishing the turn".into()),
                duration_ms: None,
                usage: None,
            });
        }
    }
}

struct ThreadProcess {
    child: Arc<ClaudeChild>,
    sink: Arc<ThreadSink>,
    settings: Mutex<Settings>,
}

/// Where the driver finds its binary and config directory.
#[derive(Clone)]
pub(crate) struct ClaudeRuntime {
    pub(crate) binary: PathBuf,
    pub(crate) config_dir: Option<PathBuf>,
}

impl ClaudeRuntime {
    pub(crate) fn from_env() -> Self {
        let binary = std::env::var_os("PINGEX_CLAUDE_CLI_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("claude"));
        let config_dir = std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from);
        Self { binary, config_dir }
    }

    /// The directory Claude writes sessions under.
    pub(crate) fn config_dir(&self) -> PathBuf {
        self.config_dir.clone().unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("~"))
                .join(".claude")
        })
    }
}

pub(crate) struct ClaudeDriver {
    home_key: String,
    runtime: ClaudeRuntime,
    wire: Arc<WireLog>,
    processes: Mutex<HashMap<String, Arc<ThreadProcess>>>,
    pending: PendingMap,
    next_request: Arc<AtomicI64>,
}

/// What the settings page and the harness picker need to know: whether a
/// `claude` binary resolves, where, and what it reports as its version.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeStatus {
    pub available: bool,
    pub path: Option<String>,
    pub version: Option<String>,
    pub config_dir: String,
    /// The oldest CLI the driver can drive.
    pub protocol_floor: String,
    pub message: Option<String>,
}

pub(crate) fn status(runtime: &ClaudeRuntime) -> ClaudeStatus {
    let config_dir = runtime.config_dir().display().to_string();
    let Some(path) = crate::codex::binary::resolve(&runtime.binary) else {
        return ClaudeStatus {
            available: false,
            path: None,
            version: None,
            config_dir,
            protocol_floor: PROTOCOL_FLOOR.into(),
            message: Some(crate::codex::binary::missing_message(&runtime.binary)),
        };
    };
    let version = std::process::Command::new(&path)
        .arg("--version")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .and_then(|line| line.split_whitespace().next().map(str::to_string));
    let below_floor = version
        .as_deref()
        .is_some_and(|version| version_below(version, PROTOCOL_FLOOR));
    ClaudeStatus {
        available: !below_floor,
        path: Some(path.display().to_string()),
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

fn version_below(version: &str, floor: &str) -> bool {
    let parse = |text: &str| -> Vec<u64> {
        text.split('.')
            .map(|part| part.trim().parse::<u64>().unwrap_or(0))
            .collect()
    };
    parse(version) < parse(floor)
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
        model("opus", "Opus", "Most capable", false),
        model("sonnet", "Sonnet", "Balanced", false),
        model("haiku", "Haiku", "Fastest and cheapest", false),
    ]})
}

impl ClaudeDriver {
    pub(crate) fn new(home_key: String, runtime: ClaudeRuntime, wire: Arc<WireLog>) -> Self {
        Self {
            home_key,
            runtime,
            wire,
            processes: Mutex::new(HashMap::new()),
            pending: Arc::new(Mutex::new(HashMap::new())),
            next_request: Arc::new(AtomicI64::new(REQUEST_ID_BASE)),
        }
    }

    pub(crate) fn runtime(&self) -> &ClaudeRuntime {
        &self.runtime
    }

    /// Whether a request id belongs to one of this driver's prompts.
    pub(crate) fn owns_request(&self, request_id: i64) -> bool {
        request_id >= REQUEST_ID_BASE
            && self
                .pending
                .lock()
                .map(|pending| pending.contains_key(&request_id))
                .unwrap_or(false)
    }

    fn process(&self, thread_id: &str) -> Option<Arc<ThreadProcess>> {
        self.processes
            .lock()
            .ok()
            .and_then(|processes| processes.get(thread_id).cloned())
            .filter(|process| process.child.is_alive())
    }

    /// Threads with a turn in flight on a live process.
    pub(crate) fn active_threads(&self) -> Vec<String> {
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

    fn resolve_binary(&self) -> Result<PathBuf, String> {
        crate::codex::binary::resolve(&self.runtime.binary).ok_or_else(|| {
            format!(
                "{} Claude Code is needed for Claude threads.",
                crate::codex::binary::missing_message(&self.runtime.binary)
            )
        })
    }

    fn spawn(
        &self,
        app: &AppHandle,
        thread_id: &str,
        cwd: &str,
        resume: bool,
        settings: &Settings,
    ) -> Result<Arc<ThreadProcess>, String> {
        let program = self.resolve_binary()?;
        let mut args: Vec<String> = if resume {
            vec!["--resume".into(), thread_id.into()]
        } else {
            vec!["--session-id".into(), thread_id.into()]
        };
        if let Some(model) = &settings.model {
            args.extend(["--model".into(), model.clone()]);
        }
        if let Some(effort) = &settings.effort {
            args.extend(["--effort".into(), effort.clone()]);
        }
        args.extend(["--permission-mode".into(), settings.mode.clone()]);
        if settings.mode == "bypassPermissions" {
            args.push("--allow-dangerously-skip-permissions".into());
        }
        let sink = Arc::new(ThreadSink {
            thread_id: thread_id.to_string(),
            home_key: self.home_key.clone(),
            app: app.clone(),
            seq: AtomicU64::new(0),
            translator: Mutex::new(Translator::new(cwd.to_string())),
            projector: Mutex::new(Projector::default()),
            journal: TurnJournal::new(app.clone(), self.home_key.clone()),
            pending: self.pending.clone(),
            next_request: self.next_request.clone(),
        });
        let child = child::spawn(
            &program,
            std::path::Path::new(cwd),
            self.runtime.config_dir.as_deref(),
            &args,
            app.clone(),
            self.wire.clone(),
            sink.clone(),
        )?;
        let process = Arc::new(ThreadProcess {
            child,
            sink,
            settings: Mutex::new(settings.clone()),
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
        if let Some(process) = self.process(thread_id) {
            let current = process
                .settings
                .lock()
                .map(|s| s.clone())
                .unwrap_or_default();
            if current.model != settings.model {
                process
                    .child
                    .control(json!({"subtype": "set_model", "model": settings.model}))
                    .await?;
            }
            if current.mode != settings.mode {
                process
                    .child
                    .control(json!({"subtype": "set_permission_mode", "mode": settings.mode}))
                    .await?;
            }
            if let Ok(mut slot) = process.settings.lock() {
                *slot = settings.clone();
            }
            return Ok(process);
        }
        self.spawn(app, thread_id, cwd, resume, settings)
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
        let settings = Settings::from_options(options);
        let process = self
            .ensure_process(app, thread_id, cwd, resume, &settings)
            .await?;
        if process.sink.turn_id().is_some() {
            return Err("A turn is already running on this thread".into());
        }
        let turn_id = uuid::Uuid::new_v4().to_string();
        let text = input
            .iter()
            .filter(|part| str_at(part, "type") == Some("text"))
            .filter_map(|part| str_at(part, "text"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut content: Vec<Value> = vec![json!({"type": "text", "text": text})];
        for part in input {
            if str_at(part, "type") == Some("image") {
                if let Some(url) = str_at(part, "url").or_else(|| str_at(part, "path")) {
                    if let Some(block) = image_block(url) {
                        content.push(block);
                    }
                }
            }
        }
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
        process.sink.emit(HarnessEvent::TurnStarted {
            turn_id: turn_id.clone(),
            model: settings.model.clone(),
        });
        process.sink.emit(HarnessEvent::UserMessage {
            item_id: format!("{turn_id}-user"),
            text,
        });
        if let Err(error) = process.child.write_frame(&frame) {
            if let Ok(mut translator) = process.sink.translator.lock() {
                translator.turn_id = None;
            }
            let tail = process.child.stderr_tail();
            return Err(if tail.is_empty() {
                error
            } else {
                format!("{error}\n{tail}")
            });
        }
        Ok(turn_id)
    }

    pub(crate) async fn interrupt(&self, thread_id: &str) -> Result<(), String> {
        let Some(process) = self.process(thread_id) else {
            return Ok(());
        };
        process.sink.deny_pending("Interrupted");
        // The receipt only exists on newer CLIs; the turn's `result` is what
        // actually ends it either way.
        let _ = process.child.control(json!({"subtype": "interrupt"})).await;
        Ok(())
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
            .take_pending(request_id)
            .ok_or("That request is no longer waiting")?;
        pending.child.respond_control(
            &pending.request_id,
            permissions::permission_result(option_id, &pending.can_use_tool),
        )
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

    /// Rename the session on the Claude side too, so `claude --resume` by
    /// name works from a terminal.
    pub(crate) async fn rename(&self, thread_id: &str, title: &str) {
        if let Some(process) = self.process(thread_id) {
            let _ = process
                .child
                .control(json!({"subtype": "rename_session", "title": title}))
                .await;
        }
    }

    /// Drop the process for a thread that was deleted or archived.
    pub(crate) fn close_thread(&self, thread_id: &str) {
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

    pub(crate) fn kill_all(&self) {
        let Ok(mut processes) = self.processes.lock() else {
            return;
        };
        for (_, process) in processes.drain() {
            process.child.kill();
        }
    }
}

/// A local image as a base64 content block, when it can be read.
fn image_block(path: &str) -> Option<Value> {
    use std::io::Read;
    let path = path.strip_prefix("file://").unwrap_or(path);
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .read_to_end(&mut bytes)
        .ok()?;
    let media_type = match std::path::Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        _ => return None,
    };
    Some(json!({
        "type": "image",
        "source": {"type": "base64", "media_type": media_type, "data": base64_encode(&bytes)},
    }))
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((bytes.len() + 2) / 3 * 4);
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
    fn presets_map_onto_permission_modes() {
        let options = |approval: &str, sandbox: &str| TurnOptions {
            approval_policy: Some(approval.into()),
            sandbox_mode: Some(sandbox.into()),
            ..Default::default()
        };
        assert_eq!(
            Settings::from_options(Some(&options("on-request", "read-only"))).mode,
            "default"
        );
        assert_eq!(
            Settings::from_options(Some(&options("on-request", "workspace-write"))).mode,
            "acceptEdits"
        );
        assert_eq!(
            Settings::from_options(Some(&options("never", "danger-full-access"))).mode,
            "bypassPermissions"
        );
        assert_eq!(Settings::from_options(None).mode, "default");
    }

    #[test]
    fn plan_mode_wins_over_the_preset() {
        let options = TurnOptions {
            approval_policy: Some("never".into()),
            collaboration_mode: Some(crate::util::json::Json(json!({"mode": "plan"}))),
            ..Default::default()
        };
        assert_eq!(Settings::from_options(Some(&options)).mode, "plan");
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
