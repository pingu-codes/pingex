//! One `codex app-server` child process, speaking JSON-RPC over stdio.
//!
//! This is deliberately ignorant of what the child is *for*. The main session
//! and every app-owned subagent run the same binary the same way; what differs
//! is only what they do with the notifications and server-initiated requests
//! that come back, which is the [`ChildSink`]'s job.

use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::AppHandle;
use tokio::sync::oneshot;

use crate::codex::wire::WireLog;

/// What a child's owner does with the traffic the child originates. Responses
/// to our own requests never reach the sink — those are correlated by id and
/// handed straight back to the caller.
pub(crate) trait ChildSink: Send + Sync {
    /// A notification (no `id`): `item/completed`, `turn/completed`, …
    fn on_notification(&self, method: &str, params: &Value);

    /// A request *from* the server (has both `id` and `method`): approvals,
    /// `item/tool/requestUserInput`, `item/tool/call`. The sink is responsible
    /// for eventually calling [`CodexChild::respond`] — an unanswered request
    /// blocks only the turn that raised it.
    fn on_server_request(&self, child: &Arc<CodexChild>, id: i64, method: &str, params: &Value);

    /// stdout reached EOF: the child is gone and will send nothing more.
    fn on_closed(&self);
}

/// Why a request failed, split by whether it can safely be resent.
pub(crate) enum RequestError {
    /// The request never reached Codex, so resending it cannot duplicate work.
    NotSent(String),
    /// Codex may already be executing it; resending could run it twice.
    Failed(String),
}

impl From<RequestError> for String {
    fn from(error: RequestError) -> Self {
        match error {
            RequestError::NotSent(message) | RequestError::Failed(message) => message,
        }
    }
}

/// How many stderr lines to keep for diagnostics.
const STDERR_TAIL_LINES: usize = 40;

pub(crate) struct CodexChild {
    stdin: Mutex<ChildStdin>,
    child: Mutex<Child>,
    next_id: AtomicI64,
    pending: Mutex<HashMap<i64, oneshot::Sender<Result<Value, String>>>>,
    alive: AtomicBool,
    wire: Arc<WireLog>,
    /// The tail of the child's stderr. Drained continuously — an undrained
    /// pipe fills at 64 KiB and then blocks the child mid-write — and kept so a
    /// failure can say what the process actually complained about.
    stderr_tail: Arc<Mutex<VecDeque<String>>>,
    /// Kept so writes can push to the wire log without threading a handle
    /// through every caller.
    app: AppHandle,
}

impl Drop for CodexChild {
    fn drop(&mut self) {
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
        }
    }
}

impl CodexChild {
    pub(crate) fn write_line(&self, message: &Value) -> Result<(), String> {
        let mut stdin = self
            .stdin
            .lock()
            .map_err(|_| "Codex stdin lock was poisoned".to_string())?;
        writeln!(stdin, "{message}")
            .map_err(|error| format!("Could not write to Codex: {error}"))?;
        stdin
            .flush()
            .map_err(|error| format!("Could not flush request to Codex: {error}"))?;
        self.wire.record(Some(&self.app), "out", message);
        Ok(())
    }

    pub(crate) fn is_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    pub(crate) fn mark_dead(&self) {
        self.alive.store(false, Ordering::SeqCst);
    }

    /// Send a request and await its response. Unlike the session-level wrapper
    /// there is no respawn-and-retry here: a child that died is simply gone,
    /// which is why callers that own a single child do not need the retry
    /// distinction [`CodexChild::try_request`] makes.
    pub(crate) async fn request(&self, method: &str, params: Value) -> Result<Value, String> {
        self.try_request(method, params).await.map_err(Into::into)
    }

    /// Send a prebuilt [`crate::codex::requests::Request`].
    pub(crate) async fn send(
        &self,
        request: crate::codex::requests::Request,
    ) -> Result<Value, String> {
        self.request(request.method, request.params).await
    }

    /// As [`CodexChild::request`], but distinguishing a request that never left
    /// this process from one that may already be executing on the other side.
    /// Only the former is safe to resend.
    pub(crate) async fn try_request(
        &self,
        method: &str,
        params: Value,
    ) -> Result<Value, RequestError> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (sender, receiver) = oneshot::channel();
        self.pending
            .lock()
            .map_err(|_| RequestError::NotSent("Codex pending lock was poisoned".into()))?
            .insert(id, sender);
        let message = json!({"id": id, "method": method, "params": params});
        if let Err(error) = self.write_line(&message) {
            if let Ok(mut pending) = self.pending.lock() {
                pending.remove(&id);
            }
            self.mark_dead();
            return Err(RequestError::NotSent(error));
        }
        receiver
            .await
            .map_err(|_| RequestError::Failed("Codex exited before responding".into()))?
            .map_err(RequestError::Failed)
    }

    /// Answer a server-initiated request by id.
    pub(crate) fn respond(&self, request_id: i64, result: Value) -> Result<(), String> {
        self.write_line(&json!({"id": request_id, "result": result}))
    }

    /// Refuse a server-initiated request by id. Codex stalls the turn behind
    /// any request it never hears back about, so declining loudly beats
    /// staying silent.
    pub(crate) fn respond_error(
        &self,
        request_id: i64,
        code: i64,
        message: &str,
    ) -> Result<(), String> {
        self.write_line(&json!({
            "id": request_id,
            "error": {"code": code, "message": message},
        }))
    }

    /// The tail of the child's stderr, newest last, as one block. Empty when
    /// the process has said nothing.
    pub(crate) fn stderr_tail(&self) -> String {
        self.stderr_tail
            .lock()
            .map(|lines| lines.iter().cloned().collect::<Vec<_>>().join("\n"))
            .unwrap_or_default()
    }

    pub(crate) fn kill(&self) {
        self.mark_dead();
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
        }
    }

    /// Fail every in-flight request. Called when the child goes away, so no
    /// caller is left awaiting a response that can never arrive.
    pub(crate) fn fail_pending(&self, reason: &str) {
        let senders: Vec<_> = match self.pending.lock() {
            Ok(mut pending) => pending.drain().map(|(_, sender)| sender).collect(),
            Err(_) => Vec::new(),
        };
        for sender in senders {
            let _ = sender.send(Err(reason.to_string()));
        }
    }

    fn resolve_pending(&self, id: i64, value: &Value) {
        let sender = self
            .pending
            .lock()
            .ok()
            .and_then(|mut pending| pending.remove(&id));
        if let Some(sender) = sender {
            let result = match value.get("error") {
                Some(error) => Err(format!("Codex request failed: {error}")),
                None => Ok(value.get("result").cloned().unwrap_or(Value::Null)),
            };
            let _ = sender.send(result);
        }
    }
}

/// Kill `codex app-server` processes that lost their parent (ppid 1). A
/// hard-killed app (e.g. a `tauri dev` rebuild) never drops its session, and
/// the orphan keeps running because the remote-control relay counts as a live
/// transport — so a paired phone keeps talking to the orphan and this fresh
/// process never sees those threads. Reaping the orphan lets the new
/// app-server reclaim the relay.
///
/// Subagent children are covered by the same rule: they are `codex app-server`
/// too, so one that outlives a crashed app is reaped on the next launch.
pub(crate) fn kill_orphaned_app_servers() {
    let Ok(output) = Command::new("ps")
        .args(["-axo", "pid=,ppid=,command="])
        .output()
    else {
        return;
    };
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let mut parts = line.split_whitespace();
        let (Some(pid), Some(ppid)) = (parts.next(), parts.next()) else {
            continue;
        };
        let command: Vec<&str> = parts.collect();
        if ppid == "1"
            && command.first().is_some_and(|arg0| arg0.ends_with("codex"))
            && command.get(1) == Some(&"app-server")
        {
            let _ = Command::new("kill").args(["-9", pid]).status();
        }
    }
}

fn reader_loop(
    child: Arc<CodexChild>,
    sink: Arc<dyn ChildSink>,
    app: AppHandle,
    stdout: std::process::ChildStdout,
) {
    for line in BufReader::new(stdout).lines() {
        let Ok(line) = line else { break };
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        child.wire.record(Some(&app), "in", &value);
        let id = value.get("id").and_then(Value::as_i64);
        let method = value.get("method").and_then(Value::as_str);
        match (id, method) {
            (Some(id), Some(method)) => {
                let params = value.get("params").cloned().unwrap_or(Value::Null);
                sink.on_server_request(&child, id, method, &params);
            }
            (Some(id), None) => child.resolve_pending(id, &value),
            (None, Some(method)) => {
                let params = value.get("params").cloned().unwrap_or(Value::Null);
                sink.on_notification(method, &params);
            }
            (None, None) => {}
        }
    }
    child.mark_dead();
    child.fail_pending("Codex exited before responding");
    sink.on_closed();
}

/// Start a `codex app-server` child, complete the `initialize` handshake, and
/// return it ready for use. `client_name` identifies us in the server's logs;
/// subagents use a distinct one so their traffic is separable.
pub(crate) async fn spawn_child(
    program: &Path,
    codex_home: &Path,
    client_name: &str,
    app: AppHandle,
    wire: Arc<WireLog>,
    sink: Arc<dyn ChildSink>,
) -> Result<Arc<CodexChild>, String> {
    let mut process = Command::new(program)
        .args(["app-server", "--stdio"])
        .env("CODEX_HOME", codex_home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("Could not start {}: {error}", program.display()))?;

    let stdin = process.stdin.take().ok_or("Codex stdin was unavailable")?;
    let stdout = process
        .stdout
        .take()
        .ok_or("Codex stdout was unavailable")?;
    let stderr = process.stderr.take();
    let stderr_tail = Arc::new(Mutex::new(VecDeque::new()));
    if let Some(stderr) = stderr {
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
    let child = Arc::new(CodexChild {
        stdin: Mutex::new(stdin),
        child: Mutex::new(process),
        next_id: AtomicI64::new(1),
        pending: Mutex::new(HashMap::new()),
        alive: AtomicBool::new(true),
        wire,
        stderr_tail,
        app: app.clone(),
    });

    let (sender, receiver) = oneshot::channel();
    child
        .pending
        .lock()
        .map_err(|_| "Codex pending lock was poisoned".to_string())?
        .insert(0, sender);
    let init = crate::codex::requests::initialize(client_name);
    child.write_line(&json!({"id": 0, "method": init.method, "params": init.params}))?;
    child.write_line(&json!({"method": "initialized", "params": {}}))?;

    let reader_child = child.clone();
    std::thread::spawn(move || reader_loop(reader_child, sink, app, stdout));

    receiver
        .await
        .map_err(|_| "Codex exited during initialization".to_string())??;
    Ok(child)
}
