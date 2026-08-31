//! One `claude -p` process speaking stream-json over stdio.
//!
//! Frames are one JSON object per line in both directions. Control requests
//! flow both ways: ours (`interrupt`, `set_model`, …) are correlated by
//! `request_id` and answered through a oneshot; the CLI's (`can_use_tool`)
//! reach the [`FrameSink`], which must eventually answer them or the tool
//! blocks forever.

use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::AppHandle;
use tokio::sync::oneshot;

use crate::codex::wire::WireLog;

/// What the process owner does with the frames the CLI originates.
pub(crate) trait FrameSink: Send + Sync {
    /// Any stdout frame that is not a control response: `system`,
    /// `assistant`, `user`, `stream_event`, `result`, `control_cancel_request`.
    fn on_frame(&self, frame: &Value);

    /// A `control_request` from the CLI. The sink answers it through
    /// [`ClaudeChild::respond_control`], now or later.
    fn on_control_request(&self, child: &Arc<ClaudeChild>, request_id: &str, request: &Value);

    /// stdout reached EOF.
    fn on_closed(&self);
}

const STDERR_TAIL_LINES: usize = 40;

/// The arguments every Claude process gets before the per-session ones.
/// Public so the live e2e suite spawns exactly what the app spawns.
pub const BASE_ARGS: [&str; 9] = [
    "-p",
    "--input-format",
    "stream-json",
    "--output-format",
    "stream-json",
    "--verbose",
    "--include-partial-messages",
    "--permission-prompt-tool",
    "stdio",
];

pub(crate) struct ClaudeChild {
    stdin: Mutex<ChildStdin>,
    child: Mutex<Child>,
    next_id: AtomicU64,
    pending: Mutex<HashMap<String, oneshot::Sender<Result<Value, String>>>>,
    alive: AtomicBool,
    wire: Arc<WireLog>,
    stderr_tail: Arc<Mutex<VecDeque<String>>>,
    app: AppHandle,
}

impl Drop for ClaudeChild {
    fn drop(&mut self) {
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
        }
    }
}

impl ClaudeChild {
    pub(crate) fn write_frame(&self, frame: &Value) -> Result<(), String> {
        let mut stdin = self
            .stdin
            .lock()
            .map_err(|_| "Claude stdin lock was poisoned".to_string())?;
        writeln!(stdin, "{frame}")
            .map_err(|error| format!("Could not write to Claude: {error}"))?;
        stdin
            .flush()
            .map_err(|error| format!("Could not flush to Claude: {error}"))?;
        self.wire.record(Some(&self.app), "out", frame);
        Ok(())
    }

    pub(crate) fn is_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    fn mark_dead(&self) {
        self.alive.store(false, Ordering::SeqCst);
    }

    /// Send a control request (`{subtype: ..}`) and await the CLI's answer.
    pub(crate) async fn control(&self, request: Value) -> Result<Value, String> {
        let id = format!("pingex-{}", self.next_id.fetch_add(1, Ordering::SeqCst));
        let (sender, receiver) = oneshot::channel();
        self.pending
            .lock()
            .map_err(|_| "Claude pending lock was poisoned".to_string())?
            .insert(id.clone(), sender);
        let frame = json!({"type": "control_request", "request_id": id, "request": request});
        if let Err(error) = self.write_frame(&frame) {
            if let Ok(mut pending) = self.pending.lock() {
                pending.remove(&id);
            }
            self.mark_dead();
            return Err(error);
        }
        receiver
            .await
            .map_err(|_| "Claude exited before responding".to_string())?
    }

    /// Answer a CLI-originated control request.
    pub(crate) fn respond_control(&self, request_id: &str, response: Value) -> Result<(), String> {
        self.write_frame(&json!({
            "type": "control_response",
            "response": {"subtype": "success", "request_id": request_id, "response": response},
        }))
    }

    pub(crate) fn respond_control_error(
        &self,
        request_id: &str,
        error: &str,
    ) -> Result<(), String> {
        self.write_frame(&json!({
            "type": "control_response",
            "response": {"subtype": "error", "request_id": request_id, "error": error},
        }))
    }

    pub(crate) fn stderr_tail(&self) -> String {
        self.stderr_tail
            .lock()
            .map(|lines| lines.iter().cloned().collect::<Vec<_>>().join("\n"))
            .unwrap_or_default()
    }

    /// Close stdin so the CLI drains and exits on its own, then make sure.
    pub(crate) fn kill(&self) {
        self.mark_dead();
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
        }
    }

    fn fail_pending(&self, reason: &str) {
        let senders: Vec<_> = match self.pending.lock() {
            Ok(mut pending) => pending.drain().map(|(_, sender)| sender).collect(),
            Err(_) => Vec::new(),
        };
        for sender in senders {
            let _ = sender.send(Err(reason.to_string()));
        }
    }

    fn resolve_control(&self, frame: &Value) -> bool {
        let Some(response) = frame.get("response") else {
            return false;
        };
        let Some(id) = response.get("request_id").and_then(Value::as_str) else {
            return false;
        };
        let sender = self
            .pending
            .lock()
            .ok()
            .and_then(|mut pending| pending.remove(id));
        let Some(sender) = sender else {
            return false;
        };
        let result = match response.get("subtype").and_then(Value::as_str) {
            Some("error") => Err(response
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("Claude refused the request")
                .to_string()),
            _ => Ok(response.get("response").cloned().unwrap_or(Value::Null)),
        };
        let _ = sender.send(result);
        true
    }
}

fn reader_loop(
    child: Arc<ClaudeChild>,
    sink: Arc<dyn FrameSink>,
    app: AppHandle,
    stdout: std::process::ChildStdout,
) {
    for line in BufReader::new(stdout).lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        child.wire.record(Some(&app), "in", &value);
        match value.get("type").and_then(Value::as_str) {
            Some("control_response") => {
                child.resolve_control(&value);
            }
            Some("control_request") => {
                let request_id = value
                    .get("request_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let request = value.get("request").cloned().unwrap_or(Value::Null);
                sink.on_control_request(&child, &request_id, &request);
            }
            Some("keep_alive") => {}
            _ => sink.on_frame(&value),
        }
    }
    child.mark_dead();
    child.fail_pending("Claude exited before responding");
    sink.on_closed();
}

/// Spawn `claude -p` in `cwd` with the given extra arguments (session flags,
/// model, permission mode). `config_dir` is always set as
/// `CLAUDE_CONFIG_DIR` so the CLI reads its login and writes its session
/// where the driver expects, regardless of what a GUI launch inherited.
/// Inherited API-key variables are stripped so the login in `config_dir` is
/// what authenticates, not a stray key in the app's environment.
pub(crate) fn spawn(
    program: &Path,
    cwd: &Path,
    config_dir: &Path,
    args: &[String],
    app: AppHandle,
    wire: Arc<WireLog>,
    sink: Arc<dyn FrameSink>,
) -> Result<Arc<ClaudeChild>, String> {
    let mut command = Command::new(program);
    command
        .args(BASE_ARGS)
        .args(args)
        .current_dir(cwd)
        .env("CLAUDE_CODE_ENTRYPOINT", "sdk-cli")
        .env("CLAUDE_CONFIG_DIR", config_dir)
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("ANTHROPIC_AUTH_TOKEN")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut process = command
        .spawn()
        .map_err(|error| format!("Could not start {}: {error}", program.display()))?;
    let stdin = process.stdin.take().ok_or("Claude stdin was unavailable")?;
    let stdout = process
        .stdout
        .take()
        .ok_or("Claude stdout was unavailable")?;
    let stderr_tail = Arc::new(Mutex::new(VecDeque::new()));
    if let Some(stderr) = process.stderr.take() {
        let tail = stderr_tail.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                let Ok(mut tail) = tail.lock() else { break };
                if tail.len() == STDERR_TAIL_LINES {
                    tail.pop_front();
                }
                tail.push_back(line);
            }
        });
    }
    let child = Arc::new(ClaudeChild {
        stdin: Mutex::new(stdin),
        child: Mutex::new(process),
        next_id: AtomicU64::new(1),
        pending: Mutex::new(HashMap::new()),
        alive: AtomicBool::new(true),
        wire,
        stderr_tail,
        app: app.clone(),
    });
    let reader_child = child.clone();
    std::thread::spawn(move || reader_loop(reader_child, sink, app, stdout));
    Ok(child)
}
