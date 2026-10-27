//! Test double for nothing: a real `maki acp`, spoken to over stdio the way
//! `crate::acp::rpc` does — same argv and requests (re-exported through
//! `pingex_app_lib::e2e`), one process per test.
//!
//! Opt-in: every test returns early unless `PINGEX_LIVE_E2E=1` (it spends real
//! tokens and needs a maki login, `maki auth login <provider>`). Knobs:
//!
//! - `PINGEX_E2E_MAKI` — maki binary (default `maki` on PATH)
//! - `PINGEX_E2E_MAKI_MODEL` — `provider/model` (default: maki's own, its
//!   last-used model)
//!
//! Sessions are created in scratch working directories and deleted with
//! `maki session delete` when each test's agent drops.

use pingex_app_lib::e2e::{acp, maki_args, AcpSpawnSettings};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

pub const TURN_TIMEOUT: Duration = Duration::from_secs(180);

pub struct Setup {
    pub binary: String,
    pub model: Option<String>,
    root: tempfile::TempDir,
}

static SETUP: OnceLock<Option<Setup>> = OnceLock::new();

pub fn setup() -> Option<&'static Setup> {
    SETUP
        .get_or_init(|| {
            if std::env::var("PINGEX_LIVE_E2E").ok().as_deref() != Some("1") {
                eprintln!("live e2e skipped: set PINGEX_LIVE_E2E=1 to run against a real maki");
                return None;
            }
            let binary = std::env::var("PINGEX_E2E_MAKI").unwrap_or_else(|_| "maki".into());
            let status = Command::new(&binary)
                .args(["auth", "status"])
                .output()
                .unwrap_or_else(|error| panic!("could not run {binary}: {error}"));
            assert!(
                String::from_utf8_lossy(&status.stdout).contains('✓'),
                "maki has no provider logged in; run `maki auth login <provider>`"
            );
            Some(Setup {
                binary,
                model: std::env::var("PINGEX_E2E_MAKI_MODEL").ok(),
                root: tempfile::tempdir().expect("scratch root"),
            })
        })
        .as_ref()
}

#[macro_export]
macro_rules! live {
    () => {
        match $crate::harness::setup() {
            Some(setup) => setup,
            None => return,
        }
    };
}

impl Setup {
    /// A fresh working directory for one test.
    pub fn work(&self, name: &str) -> PathBuf {
        let dir = self.root.path().join(name);
        std::fs::create_dir_all(&dir).unwrap();
        // maki records canonical cwds; `/var` on macOS is a symlink.
        dir.canonicalize().unwrap()
    }

    pub fn spawn(&self, cwd: &Path, skips_prompts: bool) -> Agent {
        let args = maki_args(&AcpSpawnSettings {
            model: self.model.clone(),
            skips_prompts,
        });
        Agent::spawn(&self.binary, &args, cwd)
    }
}

/// One message from the agent.
#[derive(Debug, Clone)]
pub enum Incoming {
    Notification(String, Value),
    Request(Value, String, Value),
    Response(i64, Result<Value, Value>),
}

pub struct Agent {
    binary: String,
    child: Child,
    stdin: Option<ChildStdin>,
    inbox: Receiver<Value>,
    next_id: i64,
    pub cwd: PathBuf,
    /// Sessions this agent created, deleted on drop.
    pub sessions: Vec<String>,
}

impl Agent {
    fn spawn(binary: &str, args: &[String], cwd: &Path) -> Agent {
        let mut child = Command::new(binary)
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap_or_else(|error| panic!("could not start {binary}: {error}"));
        let stdout = child.stdout.take().unwrap();
        let (sender, inbox) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Ok(value) = serde_json::from_str::<Value>(&line) {
                    if sender.send(value).is_err() {
                        break;
                    }
                }
            }
        });
        Agent {
            binary: binary.to_string(),
            stdin: child.stdin.take(),
            child,
            inbox,
            next_id: 1,
            cwd: cwd.to_path_buf(),
            sessions: Vec::new(),
        }
    }

    fn write(&mut self, message: Value) {
        let stdin = self.stdin.as_mut().expect("stdin open");
        writeln!(stdin, "{message}").unwrap();
        stdin.flush().unwrap();
    }

    pub fn send(&mut self, method: &str, params: Value) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        self.write(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        id
    }

    pub fn notify(&mut self, method: &str, params: Value) {
        self.write(json!({"jsonrpc": "2.0", "method": method, "params": params}));
    }

    pub fn respond(&mut self, id: &Value, result: Value) {
        self.write(json!({"jsonrpc": "2.0", "id": id, "result": result}));
    }

    pub fn next(&mut self, deadline: Instant) -> Option<Incoming> {
        let wait = deadline.saturating_duration_since(Instant::now());
        let message = match self.inbox.recv_timeout(wait) {
            Ok(message) => message,
            Err(RecvTimeoutError::Timeout) => panic!("maki went quiet"),
            Err(RecvTimeoutError::Disconnected) => return None,
        };
        let method = message
            .get("method")
            .and_then(Value::as_str)
            .map(str::to_string);
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        Some(match (method, message.get("id")) {
            (Some(method), Some(id)) => Incoming::Request(id.clone(), method, params),
            (Some(method), None) => Incoming::Notification(method, params),
            (None, id) => Incoming::Response(
                id.and_then(Value::as_i64).unwrap_or(-1),
                match message.get("error") {
                    Some(error) => Err(error.clone()),
                    None => Ok(message.get("result").cloned().unwrap_or(Value::Null)),
                },
            ),
        })
    }

    /// Send a request and collect everything until its response.
    pub fn call(
        &mut self,
        method: &str,
        params: Value,
        mut on_request: impl FnMut(&mut Agent, &Value, &str, &Value),
    ) -> (Result<Value, Value>, Vec<Incoming>) {
        let id = self.send(method, params);
        let deadline = Instant::now() + TURN_TIMEOUT;
        let mut seen = Vec::new();
        while let Some(message) = self.next(deadline) {
            match &message {
                Incoming::Response(answered, result) if *answered == id => {
                    return (result.clone(), seen);
                }
                Incoming::Request(rid, method, params) => {
                    let (rid, method, params) = (rid.clone(), method.clone(), params.clone());
                    on_request(self, &rid, &method, &params);
                }
                _ => {}
            }
            seen.push(message);
        }
        panic!("maki exited before answering {method}");
    }

    pub fn initialize(&mut self) -> Value {
        self.call("initialize", acp::initialize(), |_, _, _, _| {})
            .0
            .expect("initialize")
    }

    pub fn new_session(&mut self) -> String {
        self.initialize();
        let cwd = self.cwd.to_string_lossy().into_owned();
        let created = self
            .call("session/new", acp::new_session(&cwd), |_, _, _, _| {})
            .0
            .expect("session/new");
        let id = created["sessionId"]
            .as_str()
            .expect("session id")
            .to_string();
        self.sessions.push(id.clone());
        id
    }

    pub fn prompt(
        &mut self,
        session: &str,
        text: &str,
        on_request: impl FnMut(&mut Agent, &Value, &str, &Value),
    ) -> (Result<Value, Value>, Vec<Incoming>) {
        self.call(
            "session/prompt",
            acp::prompt(session, vec![json!({"type": "text", "text": text})]),
            on_request,
        )
    }

    /// Close stdin and wait for the exit, as the driver's graceful shutdown.
    pub fn close(&mut self, grace: Duration) -> bool {
        self.stdin.take();
        let deadline = Instant::now() + grace;
        while Instant::now() < deadline {
            if self.child.try_wait().ok().flatten().is_some() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }
}

impl Drop for Agent {
    fn drop(&mut self) {
        if !self.close(Duration::from_secs(2)) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        for session in &self.sessions {
            let _ = Command::new(&self.binary)
                .args(["session", "delete", session])
                .output();
        }
    }
}

/// The `session/update` payloads among `seen`.
pub fn updates(seen: &[Incoming]) -> Vec<Value> {
    seen.iter()
        .filter_map(|message| match message {
            Incoming::Notification(method, params) if method == "session/update" => {
                Some(params["update"].clone())
            }
            _ => None,
        })
        .collect()
}

pub fn kinds(updates: &[Value]) -> Vec<String> {
    updates
        .iter()
        .filter_map(|update| update["sessionUpdate"].as_str().map(str::to_string))
        .collect()
}

pub fn reply_text(updates: &[Value]) -> String {
    updates
        .iter()
        .filter(|update| update["sessionUpdate"] == "agent_message_chunk")
        .filter_map(|update| update["content"]["text"].as_str())
        .collect()
}
