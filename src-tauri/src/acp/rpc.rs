//! One ACP agent process speaking JSON-RPC 2.0 over stdio, one message per
//! line in both directions. Our requests are correlated by id and answered
//! through a oneshot; the agent's requests (`session/request_permission`) and
//! notifications (`session/update`) reach the [`RpcSink`], which must answer
//! every request or the agent waits forever.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Stdio};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::AppHandle;
use tokio::sync::oneshot;

use crate::codex::wire::WireLog;
use crate::util::host::Host;
use crate::util::stdio::StderrTail;

/// JSON-RPC's "method not found", for agent requests this client never offered.
pub(crate) const METHOD_NOT_FOUND: i64 = -32601;
/// What a request resolves to when the process dies before answering.
pub(crate) const CONNECTION_CLOSED: i64 = -32099;

/// An error response, kept structured: callers branch on the code (a session
/// locked elsewhere, a prompt already running) and show `data` when the agent
/// put the real reason there.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RpcError {
    pub(crate) code: i64,
    pub(crate) message: String,
    pub(crate) data: Option<Value>,
}

impl RpcError {
    fn from_value(error: &Value) -> Self {
        Self {
            code: error.get("code").and_then(Value::as_i64).unwrap_or(0),
            message: error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("The agent refused the request")
                .to_string(),
            data: error.get("data").filter(|data| !data.is_null()).cloned(),
        }
    }

    fn closed(reason: &str) -> Self {
        Self {
            code: CONNECTION_CLOSED,
            message: reason.to_string(),
            data: None,
        }
    }
}

impl std::fmt::Display for RpcError {
    /// The most specific text the agent gave: a string `data` (maki puts the
    /// provider's error there under a generic "Internal error") wins.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.data.as_ref() {
            Some(Value::String(data)) if !data.is_empty() => f.write_str(data),
            _ => f.write_str(&self.message),
        }
    }
}

/// What the process owner does with what the agent originates.
pub(crate) trait RpcSink: Send + Sync {
    fn on_notification(&self, connection: &Arc<AcpConnection>, method: &str, params: &Value);
    /// `id` is kept as the agent sent it (a number or a string) and must go
    /// back unchanged in the response.
    fn on_request(&self, connection: &Arc<AcpConnection>, id: Value, method: &str, params: &Value);
    /// The process is gone. Called once, after every waiting request failed.
    fn on_closed(&self, reason: &str);
}

type Replies = Mutex<HashMap<i64, oneshot::Sender<Result<Value, RpcError>>>>;

pub(crate) struct AcpConnection {
    /// `None` once [`Self::shutdown`] closed it.
    stdin: Mutex<Option<ChildStdin>>,
    child: Mutex<Child>,
    next_id: AtomicI64,
    pending: Replies,
    alive: AtomicBool,
    closed: AtomicBool,
    closed_notify: tokio::sync::Notify,
    wire: Arc<WireLog>,
    app: Option<AppHandle>,
    stderr: StderrTail,
    /// The agent's name, for messages.
    label: String,
}

/// Dropping a request future (a timeout, a cancelled task) releases its waiter.
struct Waiter<'a> {
    pending: &'a Replies,
    id: i64,
}

impl Drop for Waiter<'_> {
    fn drop(&mut self) {
        if let Ok(mut pending) = self.pending.lock() {
            pending.remove(&self.id);
        }
    }
}

impl Drop for AcpConnection {
    fn drop(&mut self) {
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
        }
    }
}

impl AcpConnection {
    fn write(&self, message: &Value) -> Result<(), String> {
        let mut stdin = self
            .stdin
            .lock()
            .map_err(|_| format!("{} stdin lock was poisoned", self.label))?;
        let Some(stdin) = stdin.as_mut().filter(|_| self.is_alive()) else {
            return Err(self.exit_reason());
        };
        writeln!(stdin, "{message}")
            .and_then(|_| stdin.flush())
            .map_err(|error| format!("Could not write to {}: {error}", self.label))?;
        self.wire.record(self.app.as_ref(), "out", message);
        Ok(())
    }

    /// Send a request and await its response.
    pub(crate) async fn request(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (sender, receiver) = oneshot::channel();
        self.pending
            .lock()
            .map_err(|_| RpcError::closed("ACP pending lock was poisoned"))?
            .insert(id, sender);
        let _waiter = Waiter {
            pending: &self.pending,
            id,
        };
        let message = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        if let Err(error) = self.write(&message) {
            return Err(RpcError::closed(&error));
        }
        receiver
            .await
            .unwrap_or_else(|_| Err(RpcError::closed(&self.exit_reason())))
    }

    pub(crate) fn notify(&self, method: &str, params: Value) -> Result<(), String> {
        self.write(&json!({"jsonrpc": "2.0", "method": method, "params": params}))
    }

    pub(crate) fn respond(&self, id: &Value, result: Value) -> Result<(), String> {
        self.write(&json!({"jsonrpc": "2.0", "id": id, "result": result}))
    }

    pub(crate) fn respond_error(&self, id: &Value, code: i64, message: &str) -> Result<(), String> {
        self.write(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {"code": code, "message": message},
        }))
    }

    pub(crate) fn is_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    pub(crate) async fn wait_closed(&self) {
        loop {
            let notified = self.closed_notify.notified();
            if self.closed.load(Ordering::SeqCst) {
                return;
            }
            notified.await;
        }
    }

    pub(crate) fn stderr_tail(&self) -> String {
        self.stderr.text()
    }

    pub(crate) fn exit_reason(&self) -> String {
        let status = self
            .child
            .lock()
            .ok()
            .and_then(|mut child| child.try_wait().ok().flatten());
        let mut reason = format!("{} exited before finishing the turn", self.label);
        if let Some(status) = status {
            reason.push_str(&format!(" ({status})"));
        }
        let tail = self.stderr_tail();
        if !tail.is_empty() {
            reason.push_str(&format!("\n{tail}"));
        }
        reason
    }

    pub(crate) fn kill(&self) {
        self.alive.store(false, Ordering::SeqCst);
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
        }
    }

    /// Close stdin so the agent saves and releases its session, then kill it
    /// if it has not gone within `grace`.
    pub(crate) async fn shutdown(&self, grace: Duration) {
        if let Ok(mut stdin) = self.stdin.lock() {
            stdin.take();
        }
        if tokio::time::timeout(grace, self.wait_closed())
            .await
            .is_err()
        {
            self.kill();
            self.wait_closed().await;
        }
    }

    fn fail_pending(&self, reason: &str) {
        let senders: Vec<_> = match self.pending.lock() {
            Ok(mut pending) => pending.drain().map(|(_, sender)| sender).collect(),
            Err(_) => Vec::new(),
        };
        for sender in senders {
            let _ = sender.send(Err(RpcError::closed(reason)));
        }
    }

    fn resolve(&self, message: &Value) {
        let Some(id) = message.get("id").and_then(Value::as_i64) else {
            return;
        };
        let sender = self
            .pending
            .lock()
            .ok()
            .and_then(|mut pending| pending.remove(&id));
        let Some(sender) = sender else {
            return;
        };
        let result = match message.get("error") {
            Some(error) => Err(RpcError::from_value(error)),
            None => Ok(message.get("result").cloned().unwrap_or(Value::Null)),
        };
        let _ = sender.send(result);
    }
}

fn reader_loop(
    connection: Arc<AcpConnection>,
    sink: Arc<dyn RpcSink>,
    stdout: std::process::ChildStdout,
) {
    for line in BufReader::new(stdout).lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        connection
            .wire
            .record(connection.app.as_ref(), "in", &message);
        let method = message.get("method").and_then(Value::as_str);
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        match (method, message.get("id")) {
            (Some(method), Some(id)) => sink.on_request(&connection, id.clone(), method, &params),
            (Some(method), None) => sink.on_notification(&connection, method, &params),
            (None, Some(_)) => connection.resolve(&message),
            (None, None) => {}
        }
    }
    connection.alive.store(false, Ordering::SeqCst);
    connection.stderr.wait_drained(Duration::from_millis(200));
    let reason = connection.exit_reason();
    // Reap it: one that merely closed stdout would otherwise linger.
    if let Ok(mut process) = connection.child.lock() {
        let _ = process.kill();
        let _ = process.wait();
    }
    connection.fail_pending(&reason);
    sink.on_closed(&reason);
    connection.closed.store(true, Ordering::SeqCst);
    connection.closed_notify.notify_waiters();
}

/// Spawn `program args…` in `cwd` on `host`. Every path is a host path.
#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn(
    host: &Host,
    program: &str,
    args: &[String],
    cwd: &str,
    label: &str,
    app: Option<AppHandle>,
    wire: Arc<WireLog>,
    sink: Arc<dyn RpcSink>,
) -> Result<Arc<AcpConnection>, String> {
    let argv: Vec<&str> = args.iter().map(String::as_str).collect();
    let mut command = host.command(program, &argv, Some(cwd), &[], &[]);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut process = command
        .spawn()
        .map_err(|error| format!("Could not start {program} ({}): {error}", host.label()))?;
    let stdin = process.stdin.take().ok_or("Agent stdin was unavailable")?;
    let stdout = process
        .stdout
        .take()
        .ok_or("Agent stdout was unavailable")?;
    let stderr = StderrTail::spawn(process.stderr.take());
    let connection = Arc::new(AcpConnection {
        stdin: Mutex::new(Some(stdin)),
        child: Mutex::new(process),
        next_id: AtomicI64::new(1),
        pending: Mutex::new(HashMap::new()),
        alive: AtomicBool::new(true),
        closed: AtomicBool::new(false),
        closed_notify: tokio::sync::Notify::new(),
        wire,
        app,
        stderr,
        label: label.to_string(),
    });
    let reader = connection.clone();
    std::thread::spawn(move || reader_loop(reader, sink, stdout));
    Ok(connection)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[derive(Default)]
    struct Recorder {
        seen: Mutex<Vec<String>>,
        closed: Mutex<Option<mpsc::Sender<String>>>,
    }

    impl RpcSink for Recorder {
        fn on_notification(&self, _: &Arc<AcpConnection>, method: &str, _: &Value) {
            self.seen.lock().unwrap().push(format!("notify {method}"));
        }
        fn on_request(&self, connection: &Arc<AcpConnection>, id: Value, method: &str, _: &Value) {
            self.seen
                .lock()
                .unwrap()
                .push(format!("request {method} {id}"));
            connection.respond(&id, json!({"ok": true})).unwrap();
        }
        fn on_closed(&self, reason: &str) {
            if let Some(sender) = self.closed.lock().unwrap().take() {
                let _ = sender.send(reason.to_string());
            }
        }
    }

    /// A fake agent: a shell script that answers by line number.
    fn agent(script: &str) -> (tempfile::TempDir, String) {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let program = temp.path().join("fake-agent");
        std::fs::write(&program, format!("#!/bin/sh\n{script}")).unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
        let program = program.to_string_lossy().into_owned();
        (temp, program)
    }

    fn start(
        script: &str,
    ) -> (
        tempfile::TempDir,
        Arc<AcpConnection>,
        Arc<Recorder>,
        mpsc::Receiver<String>,
    ) {
        let (temp, program) = agent(script);
        let (tx, rx) = mpsc::channel();
        let sink = Arc::new(Recorder {
            closed: Mutex::new(Some(tx)),
            ..Default::default()
        });
        let connection = spawn(
            &Host::Native,
            &program,
            &[],
            temp.path().to_str().unwrap(),
            "fake",
            None,
            Arc::new(WireLog::default()),
            sink.clone(),
        )
        .unwrap();
        (temp, connection, sink, rx)
    }

    #[tokio::test]
    async fn notifications_before_a_response_are_delivered_first() {
        let (_temp, connection, sink, _closed) = start(concat!(
            "read line\n",
            "echo '{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{}}'\n",
            "echo '{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"session/request_permission\",\"params\":{}}'\n",
            "read answer\n",
            "echo '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"sessionId\":\"s\"}}'\n",
            "read rest\n",
        ));
        let result = connection.request("session/load", json!({})).await.unwrap();
        assert_eq!(result["sessionId"], "s");
        assert_eq!(
            *sink.seen.lock().unwrap(),
            vec![
                "notify session/update",
                "request session/request_permission 7"
            ]
        );
    }

    #[tokio::test]
    async fn an_error_response_keeps_its_code_and_data() {
        let (_temp, connection, _sink, _closed) = start(concat!(
            "read line\n",
            "echo '{\"jsonrpc\":\"2.0\",\"id\":1,\"error\":{\"code\":-32603,\"message\":\"Internal error\",\"data\":\"API error (400): bad model\"}}'\n",
            "read rest\n",
        ));
        let error = connection
            .request("session/prompt", json!({}))
            .await
            .unwrap_err();
        assert_eq!(error.code, -32603);
        assert_eq!(error.to_string(), "API error (400): bad model");
    }

    #[tokio::test]
    async fn an_exit_fails_waiting_requests_with_the_stderr_tail() {
        let (_temp, connection, _sink, closed) =
            start("read line\necho 'no provider configured' >&2\nexit 3\n");
        let error = connection
            .request("initialize", json!({}))
            .await
            .unwrap_err();
        assert_eq!(error.code, CONNECTION_CLOSED);
        assert!(
            error.message.contains("no provider configured"),
            "{error:?}"
        );
        let reason = closed.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(reason.contains("exit status: 3"), "{reason}");
        assert!(!connection.is_alive());
    }

    #[tokio::test]
    async fn shutdown_closes_stdin_and_waits_for_the_exit() {
        let (_temp, connection, _sink, closed) = start("while read line; do :; done\nexit 0\n");
        connection.shutdown(Duration::from_secs(5)).await;
        assert!(closed.recv_timeout(Duration::from_secs(1)).is_ok());
        assert!(connection.notify("session/cancel", json!({})).is_err());
    }
}
