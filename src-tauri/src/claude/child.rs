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
use std::process::{Child, ChildStdin, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::AppHandle;
use tokio::sync::oneshot;

use crate::codex::wire::WireLog;
use crate::util::host::Host;

/// What the process owner does with the frames the CLI originates.
pub(crate) trait FrameSink: Send + Sync {
    /// Any stdout frame that is not a control response: `system`,
    /// `assistant`, `user`, `stream_event`, `result`, `control_cancel_request`.
    fn on_frame(&self, frame: &Value);

    /// A `control_request` from the CLI. The sink answers it through
    /// [`ClaudeChild::respond_control`], now or later.
    fn on_control_request(&self, child: &Arc<ClaudeChild>, request_id: &str, request: &Value);

    /// stdout reached EOF.
    fn on_closed(&self, reason: &str);
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

/// Environment every Claude process gets. `CLAUDE_CONFIG_DIR` is added per
/// spawn.
const ENV: [(&str, &str); 1] = [("CLAUDE_CODE_ENTRYPOINT", "sdk-cli")];
/// Inherited API-key variables are stripped so the login in the config
/// directory is what authenticates, not a stray key in the app's environment.
pub const UNSET_ENV: [&str; 2] = ["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN"];

type ControlReplies = Mutex<HashMap<String, oneshot::Sender<Result<Value, String>>>>;

pub(crate) struct ClaudeChild {
    stdin: Mutex<ChildStdin>,
    child: Mutex<Child>,
    next_id: AtomicU64,
    pending: ControlReplies,
    alive: AtomicBool,
    closed: AtomicBool,
    closed_notify: tokio::sync::Notify,
    wire: Arc<WireLog>,
    stderr_tail: Arc<Mutex<VecDeque<String>>>,
    app: Option<AppHandle>,
    stderr_done: Mutex<std::sync::mpsc::Receiver<()>>,
}

/// Dropping a request future during cancellation or timeout releases its waiter.
struct ControlWaiter<'a> {
    pending: &'a ControlReplies,
    id: &'a str,
}

impl Drop for ControlWaiter<'_> {
    fn drop(&mut self) {
        if let Ok(mut pending) = self.pending.lock() {
            pending.remove(self.id);
        }
    }
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
        if !self.is_alive() {
            return Err(self.exit_reason());
        }
        writeln!(stdin, "{frame}")
            .map_err(|error| format!("Could not write to Claude: {error}"))?;
        stdin
            .flush()
            .map_err(|error| format!("Could not flush to Claude: {error}"))?;
        self.wire.record(self.app.as_ref(), "out", frame);
        Ok(())
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

    fn mark_dead(&self) {
        self.alive.store(false, Ordering::SeqCst);
    }

    /// Send a control request (`{subtype: ..}`) and await the CLI's answer.
    pub(crate) async fn control(&self, request: Value) -> Result<Value, String> {
        let id = format!("pingex-{}", self.next_id.fetch_add(1, Ordering::SeqCst));
        self.control_identified(&id, request).await
    }

    pub(crate) async fn control_identified(
        &self,
        id: &str,
        request: Value,
    ) -> Result<Value, String> {
        let (sender, receiver) = oneshot::channel();
        self.pending
            .lock()
            .map_err(|_| "Claude pending lock was poisoned".to_string())?
            .insert(id.to_string(), sender);
        let _waiter = ControlWaiter {
            pending: &self.pending,
            id,
        };
        let frame = json!({"type": "control_request", "request_id": id, "request": request});
        if let Err(error) = self.write_frame(&frame) {
            if let Ok(mut pending) = self.pending.lock() {
                pending.remove(id);
            }
            self.mark_dead();
            return Err(error);
        }
        receiver
            .await
            .map_err(|_| "Claude exited before responding".to_string())?
    }

    /// Cancel this control operation without interrupting the session's turn.
    pub(crate) fn cancel_control(&self, id: &str) -> Result<(), String> {
        if let Ok(mut pending) = self.pending.lock() {
            pending.remove(id);
        }
        self.write_frame(&json!({"type": "control_cancel_request", "request_id": id}))
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

    pub(crate) fn exit_reason(&self) -> String {
        let status = self
            .child
            .lock()
            .ok()
            .and_then(|mut child| child.try_wait().ok().flatten());
        let tail = self.stderr_tail();
        let mut reason = "Claude exited before finishing the turn".to_string();
        if let Some(status) = status {
            reason.push_str(&format!(" ({status})"));
        }
        if !tail.is_empty() {
            reason.push_str(&format!("\n{tail}"));
        }
        reason
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
}

fn resolve_control(pending: &ControlReplies, frame: &Value) -> bool {
    let Some(response) = frame.get("response") else {
        return false;
    };
    // Progress is not a terminal reply and must retain the waiter.
    if !matches!(
        response.get("subtype").and_then(Value::as_str),
        Some("success" | "error")
    ) {
        return false;
    }
    let Some(id) = response.get("request_id").and_then(Value::as_str) else {
        return false;
    };
    let sender = pending
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

fn reader_loop(
    child: Arc<ClaudeChild>,
    sink: Arc<dyn FrameSink>,
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
        child.wire.record(child.app.as_ref(), "in", &value);
        match value.get("type").and_then(Value::as_str) {
            Some("control_response") => {
                resolve_control(&child.pending, &value);
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
            Some("keep_alive" | "control_progress") => {}
            _ => sink.on_frame(&value),
        }
    }
    child.mark_dead();
    // EOF on stdout can race the stderr reader. Bound the drain so a child
    // that merely closed stdout cannot keep a turn running forever.
    if let Ok(done) = child.stderr_done.lock() {
        let _ = done.recv_timeout(std::time::Duration::from_millis(200));
    }
    let reason = child.exit_reason();
    // Reap it: `exit_reason` only collects a child that has already exited,
    // and one that merely closed stdout would otherwise linger as a zombie.
    if let Ok(mut process) = child.child.lock() {
        let _ = process.kill();
        let _ = process.wait();
    }
    child.fail_pending(&reason);
    sink.on_closed(&reason);
    child.closed.store(true, Ordering::SeqCst);
    child.closed_notify.notify_waiters();
}

/// Spawn `claude -p` in `cwd` with the given extra arguments (session flags,
/// model, permission mode). `config_dir` is always set as
/// `CLAUDE_CONFIG_DIR` so the CLI reads its login and writes its session
/// where the driver expects, regardless of what a GUI launch inherited.
/// Inherited API-key variables are stripped so the login in `config_dir` is
/// what authenticates, not a stray key in the app's environment.
#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn(
    host: &Host,
    program: &str,
    cwd: &str,
    config_dir: &str,
    args: &[String],
    app: AppHandle,
    wire: Arc<WireLog>,
    sink: Arc<dyn FrameSink>,
) -> Result<Arc<ClaudeChild>, String> {
    // Every path here is a host path; the host wraps the spawn.
    spawn_with_app(host, program, cwd, config_dir, args, Some(app), wire, sink)
}

#[allow(clippy::too_many_arguments)]
fn spawn_with_app(
    host: &Host,
    program: &str,
    cwd: &str,
    config_dir: &str,
    args: &[String],
    app: Option<AppHandle>,
    wire: Arc<WireLog>,
    sink: Arc<dyn FrameSink>,
) -> Result<Arc<ClaudeChild>, String> {
    let mut argv: Vec<&str> = BASE_ARGS.to_vec();
    argv.extend(args.iter().map(String::as_str));
    let mut env: Vec<(&str, &str)> = ENV.to_vec();
    env.push(("CLAUDE_CONFIG_DIR", config_dir));
    let mut command = host.command(program, &argv, Some(cwd), &env, &UNSET_ENV);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut process = command
        .spawn()
        .map_err(|error| format!("Could not start {program} ({}): {error}", host.label()))?;
    let stdin = process.stdin.take().ok_or("Claude stdin was unavailable")?;
    let stdout = process
        .stdout
        .take()
        .ok_or("Claude stdout was unavailable")?;
    let stderr_tail = Arc::new(Mutex::new(VecDeque::new()));
    let (stderr_finished, stderr_done) = std::sync::mpsc::channel();
    if let Some(stderr) = process.stderr.take() {
        let tail = stderr_tail.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                let Ok(mut tail) = tail.lock() else { break };
                if tail.len() == STDERR_TAIL_LINES {
                    tail.pop_front();
                }
                // Bound each line as well as the count, including UTF-8.
                let mut end = line.len().min(2048);
                while !line.is_char_boundary(end) {
                    end -= 1;
                }
                tail.push_back(line[..end].to_string());
            }
            let _ = stderr_finished.send(());
        });
    }
    let child = Arc::new(ClaudeChild {
        stdin: Mutex::new(stdin),
        child: Mutex::new(process),
        next_id: AtomicU64::new(1),
        pending: Mutex::new(HashMap::new()),
        alive: AtomicBool::new(true),
        closed: AtomicBool::new(false),
        closed_notify: tokio::sync::Notify::new(),
        wire,
        stderr_tail,
        app,
        stderr_done: Mutex::new(stderr_done),
    });
    let reader_child = child.clone();
    std::thread::spawn(move || reader_loop(reader_child, sink, stdout));
    Ok(child)
}

#[cfg(test)]
mod control_tests {
    use super::*;

    #[cfg(unix)]
    #[tokio::test]
    async fn restarted_session_uses_resume_and_exit_details_survive_the_stdio_race() {
        use std::os::unix::fs::PermissionsExt;
        struct Sink {
            closed: std::sync::mpsc::Sender<String>,
            release: Mutex<std::sync::mpsc::Receiver<()>>,
        }
        impl FrameSink for Sink {
            fn on_frame(&self, _: &Value) {}
            fn on_control_request(&self, _: &Arc<ClaudeChild>, _: &str, _: &Value) {}
            fn on_closed(&self, reason: &str) {
                self.closed.send(reason.into()).unwrap();
                self.release
                    .lock()
                    .unwrap()
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .unwrap();
            }
        }
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("projects/previous-cwd");
        std::fs::create_dir_all(&project).unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        std::fs::write(project.join(format!("{id}.jsonl")), "{}\n").unwrap();
        let program = temp.path().join("fake-claude");
        std::fs::write(&program, "#!/bin/sh\nfor arg in \"$@\"; do\nif [ \"$arg\" = --resume ]; then echo 'fixture resume failure' >&2; exit 23; fi\ndone\necho 'incorrect new session' >&2\nexit 24\n").unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
        // Two fresh processes model repeated sends after an early exit.
        for _ in 0..2 {
            let resume = super::super::sessions::should_resume(
                &Host::Native,
                temp.path().to_str().unwrap(),
                &id,
                false,
            )
            .unwrap();
            let args = super::super::driver::turn_args(None, None, "plan", resume, &id);
            let (tx, rx) = std::sync::mpsc::channel();
            let (release, released) = std::sync::mpsc::channel();
            let child = spawn_with_app(
                &Host::Native,
                program.to_str().unwrap(),
                temp.path().to_str().unwrap(),
                temp.path().to_str().unwrap(),
                &args,
                None,
                Arc::new(WireLog::default()),
                Arc::new(Sink {
                    closed: tx,
                    release: Mutex::new(released),
                }),
            )
            .unwrap();
            let reason = rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
            assert!(reason.contains("fixture resume failure"), "{reason}");
            assert!(reason.contains("23"), "{reason}");
            assert!(!child.is_alive());
            assert!(
                tokio::time::timeout(std::time::Duration::from_millis(20), child.wait_closed())
                    .await
                    .is_err(),
                "replacement must wait for the old sink's terminal event"
            );
            release.send(()).unwrap();
            tokio::time::timeout(std::time::Duration::from_secs(1), child.wait_closed())
                .await
                .unwrap();
            assert!(child.write_frame(&json!({"type":"user"})).is_err());
            assert!(rx.try_recv().is_err());
        }
    }

    #[tokio::test]
    async fn progress_does_not_consume_a_reply_and_late_replies_are_ignored() {
        let pending = ControlReplies::default();
        let (sender, mut receiver) = oneshot::channel();
        pending.lock().unwrap().insert("btw-one".into(), sender);
        assert!(!resolve_control(
            &pending,
            &json!({"response": {"subtype": "progress", "request_id": "btw-one", "response": {"status": "started"}}})
        ));
        assert!(matches!(
            receiver.try_recv(),
            Err(oneshot::error::TryRecvError::Empty)
        ));
        let response = json!({"response": {"subtype": "success", "request_id": "btw-one", "response": {"response": "Answer"}}});
        assert!(resolve_control(&pending, &response));
        assert_eq!(receiver.await.unwrap().unwrap()["response"], "Answer");
        assert!(!resolve_control(&pending, &response));
    }

    #[tokio::test]
    async fn dropping_an_initialize_waiter_cleans_it_without_removing_other_requests() {
        let pending = ControlReplies::default();
        let (sender, receiver) = oneshot::channel();
        pending.lock().unwrap().insert("initialize".into(), sender);
        let (other, other_reply) = oneshot::channel();
        pending.lock().unwrap().insert("other".into(), other);
        drop(ControlWaiter {
            pending: &pending,
            id: "initialize",
        });
        assert!(receiver.await.is_err());
        assert!(resolve_control(
            &pending,
            &json!({"response": {"subtype": "error", "request_id": "other", "error": "Disconnected"}})
        ));
        assert_eq!(other_reply.await.unwrap(), Err("Disconnected".into()));
    }
}
