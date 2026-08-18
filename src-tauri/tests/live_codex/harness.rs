//! Test double for nothing: a real `codex app-server`, spoken to over stdio the
//! same way `crate::codex::child` does, with a scratch `CODEX_HOME` and cwd
//! built for the suite.
//!
//! Opt-in: every test returns early unless `PINGEX_LIVE_E2E=1` (it spends real
//! tokens and needs the user's Codex login). Knobs:
//!
//! - `PINGEX_E2E_MODEL` — model slug (default `gpt-5.6-luna`)
//! - `PINGEX_E2E_CODEX` — codex binary (default: resolved like the app does)
//! - `PINGEX_E2E_AUTH_HOME` — Codex home to copy `auth.json` from (default
//!   `~/.codex-personal`, then `~/.codex`)

use pingex_app_lib::e2e::requests::{self, Request};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex, OnceLock, RwLock};
use std::time::{Duration, Instant};

pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
pub const TURN_TIMEOUT: Duration = Duration::from_secs(180);
pub const DEFAULT_MODEL: &str = "gpt-5.6-luna";

/// Text of the file the mention/attachment fixtures point the model at.
pub const MARKER_TOKEN: &str = "MENTION-OK";
/// The skill the scratch home ships; it tells the model to answer with this.
pub const SKILL_NAME: &str = "e2e-skill";
pub const SKILL_TOKEN: &str = "E2E-SKILL-OK";
pub const MCP_SERVER: &str = "e2e";
pub const MCP_TOOL: &str = "echo";

/// A JSON-RPC error response from the server.
#[derive(Debug, Clone)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
}

impl std::fmt::Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "JSON-RPC error {}: {}", self.code, self.message)
    }
}

/// The finished turn a [`Server::run_turn`] call observed.
#[derive(Debug)]
#[allow(dead_code)]
pub struct TurnOutcome {
    pub turn_id: String,
    pub status: String,
    /// `item/completed` items for this turn, in order.
    pub items: Vec<Value>,
}

impl TurnOutcome {
    /// The last agent message's text, or empty.
    pub fn reply(&self) -> String {
        self.items
            .iter()
            .rev()
            .find(|item| item.get("type").and_then(Value::as_str) == Some("agentMessage"))
            .and_then(|item| item.get("text").and_then(Value::as_str))
            .unwrap_or_default()
            .to_string()
    }

    pub fn item_types(&self) -> Vec<&str> {
        self.items
            .iter()
            .filter_map(|item| item.get("type").and_then(Value::as_str))
            .collect()
    }
}

struct Inner {
    process: Mutex<Child>,
    stdin: Mutex<ChildStdin>,
    next_id: AtomicI64,
    pending: Mutex<HashMap<i64, mpsc::Sender<Value>>>,
    /// Every message the server sent, in order, plus a wakeup for waiters.
    messages: Mutex<Vec<Value>>,
    changed: Condvar,
    stderr: Mutex<Vec<String>>,
}

impl Drop for Inner {
    fn drop(&mut self) {
        if let Ok(mut process) = self.process.lock() {
            let _ = process.kill();
        }
    }
}

#[allow(dead_code)]
pub struct Server {
    /// The live app-server child; swapped wholesale by [`Server::restart`].
    inner: RwLock<Arc<Inner>>,
    pub model: String,
    pub codex_home: PathBuf,
    /// The project directory turns run in.
    pub cwd: PathBuf,
    pub image_path: PathBuf,
}

static SERVER: OnceLock<Option<Server>> = OnceLock::new();

/// The shared server, or `None` when the suite is not enabled. Spawned once
/// per test binary; tests share it (and its scratch home) so the suite stays
/// quick.
pub fn server() -> Option<&'static Server> {
    SERVER
        .get_or_init(|| {
            if std::env::var("PINGEX_LIVE_E2E").ok().as_deref() != Some("1") {
                eprintln!("live e2e skipped: set PINGEX_LIVE_E2E=1 to run against a real codex");
                return None;
            }
            Some(Server::spawn().unwrap_or_else(|error| panic!("could not start codex: {error}")))
        })
        .as_ref()
}

/// Return early from a test unless the suite is enabled.
#[macro_export]
macro_rules! live {
    () => {
        match $crate::harness::server() {
            Some(server) => server,
            None => return,
        }
    };
}

fn home_dir() -> PathBuf {
    dirs::home_dir().expect("home directory")
}

/// Where the user's `auth.json` comes from.
fn auth_source() -> PathBuf {
    if let Ok(home) = std::env::var("PINGEX_E2E_AUTH_HOME") {
        return PathBuf::from(home);
    }
    for candidate in [".codex-personal", ".codex"] {
        let dir = home_dir().join(candidate);
        if dir.join("auth.json").is_file() {
            return dir;
        }
    }
    home_dir().join(".codex")
}

fn codex_binary() -> PathBuf {
    let requested = std::env::var("PINGEX_E2E_CODEX").unwrap_or_else(|_| "codex".to_string());
    let requested = Path::new(&requested);
    pingex_app_lib::e2e::resolve_codex_binary(requested)
        .unwrap_or_else(|| panic!("{}", pingex_app_lib::e2e::missing_message(requested)))
}

/// The `mcp_echo` example binary, built on demand (`cargo test --test
/// live_codex` does not build examples the way a bare `cargo test` does).
fn mcp_echo_binary() -> PathBuf {
    let exe = std::env::current_exe().expect("test exe path");
    // target/debug/deps/live_codex-… → target/debug/examples/mcp_echo
    let debug = exe
        .parent()
        .and_then(Path::parent)
        .expect("target dir layout");
    let path = debug.join("examples").join("mcp_echo");
    if !path.is_file() {
        let status = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
            .args(["build", "--example", "mcp_echo"])
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .status()
            .expect("cargo build --example mcp_echo");
        assert!(status.success(), "building the mcp_echo example failed");
    }
    assert!(path.is_file(), "{} missing after build", path.display());
    path
}

/// A 1×1 red PNG.
const PIXEL_PNG: [u8; 69] = [
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
    0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0xF8, 0xCF, 0xC0, 0x00,
    0x00, 0x03, 0x01, 0x01, 0x00, 0xC9, 0xFE, 0x92, 0xEF, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E,
    0x44, 0xAE, 0x42, 0x60, 0x82,
];

fn io_err(error: std::io::Error) -> String {
    error.to_string()
}

impl Server {
    fn spawn() -> Result<Server, String> {
        let model = std::env::var("PINGEX_E2E_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_string());
        let root = std::env::temp_dir().join(format!("pingex-live-e2e-{}", std::process::id()));
        let codex_home = root.join("home");
        let cwd = root.join("work");
        std::fs::create_dir_all(codex_home.join("skills").join(SKILL_NAME)).map_err(io_err)?;
        std::fs::create_dir_all(&cwd).map_err(io_err)?;

        let auth = auth_source().join("auth.json");
        std::fs::copy(&auth, codex_home.join("auth.json")).map_err(|error| {
            format!(
                "could not copy {} (set PINGEX_E2E_AUTH_HOME to a logged-in Codex home): {error}",
                auth.display()
            )
        })?;
        std::fs::write(
            codex_home.join("config.toml"),
            format!(
                "model = \"{model}\"\n\
                 model_reasoning_effort = \"low\"\n\
                 approval_policy = \"never\"\n\
                 sandbox_mode = \"workspace-write\"\n\
                 \n\
                 [features]\n\
                 goals = true\n\
                 \n\
                 [mcp_servers.{MCP_SERVER}]\n\
                 command = \"{}\"\n",
                mcp_echo_binary().display()
            ),
        )
        .map_err(io_err)?;
        std::fs::write(
            codex_home.join("skills").join(SKILL_NAME).join("SKILL.md"),
            format!(
                "---\nname: {SKILL_NAME}\ndescription: Test skill for the Pingex live e2e suite. Use when asked to use the e2e skill.\n---\n\
                 When this skill is used, reply with exactly the token {SKILL_TOKEN} and nothing else.\n"
            ),
        )
        .map_err(io_err)?;
        std::fs::write(
            cwd.join("AGENTS.md"),
            "You are under automated test. Answer as tersely as possible: when asked to reply with a token, reply with only that token.\n",
        )
        .map_err(io_err)?;
        std::fs::write(cwd.join("MARKER.md"), format!("{MARKER_TOKEN}\n")).map_err(io_err)?;
        let image_path = cwd.join("pixel.png");
        std::fs::write(&image_path, PIXEL_PNG).map_err(io_err)?;

        let inner = start_session(&codex_home, &cwd)?;
        Ok(Server {
            inner: RwLock::new(inner),
            model,
            codex_home,
            cwd,
            image_path,
        })
    }

    fn inner(&self) -> Arc<Inner> {
        self.inner.read().expect("server lock").clone()
    }

    /// Simulate the app being quit and relaunched: kill the app-server child
    /// and spawn a fresh one on the same `CODEX_HOME` and cwd. Nothing in
    /// memory survives — message cursors from before are invalid, and threads
    /// must be re-attached with `thread/resume`, exactly as the app does.
    pub fn restart(&self) {
        // The reader threads keep the old `Inner` alive, so kill explicitly
        // (and wait, so the thread store's writer lock is released) before
        // the replacement comes up.
        {
            let old = self.inner();
            let mut process = old.process.lock().expect("process lock");
            let _ = process.kill();
            let _ = process.wait();
        }
        let fresh = start_session(&self.codex_home, &self.cwd)
            .unwrap_or_else(|error| panic!("could not restart codex: {error}"));
        *self.inner.write().expect("server lock") = fresh;
    }

    /// The Pingex frontend database for the scratch home. Every call opens
    /// it anew, which is what the app does on launch — so "open, assert" after
    /// [`Server::restart`] is the persistence check.
    pub fn open_db(&self) -> turso::Database {
        block_on(pingex_app_lib::e2e::open_database(&self.codex_home))
            .unwrap_or_else(|error| panic!("could not open the Pingex database: {error}"))
    }

    /// A fresh git repository with one commit under the scratch root.
    pub fn git_repo(&self, name: &str) -> PathBuf {
        let path = self.cwd.parent().expect("scratch root").join(name);
        std::fs::create_dir_all(&path).expect("repo dir");
        std::fs::write(path.join("README.md"), format!("# {name}\n")).expect("readme");
        for args in [
            vec!["init", "-q", "-b", "main"],
            vec!["config", "user.email", "e2e@pingex.test"],
            vec!["config", "user.name", "Pingex E2E"],
            vec!["add", "."],
            vec!["commit", "-q", "-m", "init"],
        ] {
            git(&path, &args);
        }
        path
    }

    fn write(&self, message: &Value) -> Result<(), String> {
        self.inner().write(message)
    }

    /// The last lines the server wrote to stderr, for failure messages.
    pub fn stderr_tail(&self) -> String {
        self.inner().stderr_tail()
    }

    /// Send a request and wait for its response.
    pub fn request(&self, request: Request) -> Result<Value, RpcError> {
        self.inner().request(request)
    }

    /// `request`, panicking with context on any error. Most of the suite is
    /// "the server must accept what the app sends", so this is the default.
    pub fn call(&self, request: Request) -> Value {
        let method = request.method;
        let params = request.params.clone();
        self.request(request).unwrap_or_else(|error| {
            panic!(
                "{method} rejected: {error}\nparams: {}\nstderr:\n{}",
                serde_json::to_string_pretty(&params).unwrap_or_default(),
                self.stderr_tail()
            )
        })
    }

    /// Answer a server-initiated request (approval, user input, tool call).
    pub fn respond(&self, request_id: i64, result: Value) {
        self.write(&json!({"id": request_id, "result": result}))
            .expect("write response");
    }

    /// Position in the message log; pass to `wait_for` to only see later traffic.
    pub fn cursor(&self) -> usize {
        self.inner().messages.lock().expect("bus lock").len()
    }

    /// Block until a message at or after `from` matches, returning it and the
    /// index just past it. `None` on timeout.
    pub fn wait_for(
        &self,
        from: usize,
        timeout: Duration,
        mut predicate: impl FnMut(&Value) -> bool,
    ) -> Option<(usize, Value)> {
        let deadline = Instant::now() + timeout;
        let inner = self.inner();
        let mut messages = inner.messages.lock().expect("bus lock");
        let mut index = from;
        loop {
            while index < messages.len() {
                if predicate(&messages[index]) {
                    return Some((index + 1, messages[index].clone()));
                }
                index += 1;
            }
            let now = Instant::now();
            if now >= deadline {
                return None;
            }
            let (guard, _) = inner
                .changed
                .wait_timeout(messages, deadline - now)
                .expect("bus lock");
            messages = guard;
        }
    }

    /// Wait for a notification with this method whose params match `predicate`.
    pub fn wait_notification(
        &self,
        from: usize,
        method: &str,
        timeout: Duration,
        mut predicate: impl FnMut(&Value) -> bool,
    ) -> Option<(usize, Value)> {
        self.wait_for(from, timeout, |message| {
            message.get("id").is_none()
                && message.get("method").and_then(Value::as_str) == Some(method)
                && predicate(message.get("params").unwrap_or(&Value::Null))
        })
        .map(|(next, message)| (next, message["params"].clone()))
    }

    /// Wait for a server-initiated request with this method; returns
    /// `(next_cursor, request_id, params)`.
    pub fn wait_server_request(
        &self,
        from: usize,
        method: &str,
        timeout: Duration,
    ) -> Option<(usize, i64, Value)> {
        self.wait_for(from, timeout, |message| {
            message.get("id").is_some()
                && message.get("method").and_then(Value::as_str) == Some(method)
        })
        .map(|(next, message)| {
            (
                next,
                message["id"].as_i64().expect("server request id"),
                message["params"].clone(),
            )
        })
    }

    /// Messages recorded since `from` (for diagnostics).
    pub fn messages_since(&self, from: usize) -> Vec<Value> {
        self.inner().messages.lock().expect("bus lock")[from..].to_vec()
    }

    /// Start a thread in the scratch cwd with the app's own params and return
    /// its id.
    pub fn start_thread(&self) -> String {
        let response = self.call(requests::thread_start(
            &self.cwd.display().to_string(),
            None,
            None,
            None,
        ));
        thread_id_of(&response)
    }

    /// Send a `turn/start` built by the app and wait for `turn/completed`.
    pub fn run_turn(&self, request: Request) -> TurnOutcome {
        assert_eq!(request.method, "turn/start", "run_turn wants a turn/start");
        let from = self.cursor();
        let response = self.call(request);
        let turn_id = response
            .pointer("/turn/id")
            .and_then(Value::as_str)
            .unwrap_or_else(|| panic!("turn/start returned no turn id: {response}"))
            .to_string();
        self.await_turn(from, &turn_id)
    }

    /// Like [`Server::run_turn`], but follows the turn the server actually
    /// announces in `turn/started` rather than the id `turn/start` returned —
    /// on a thread with a goal the two can differ (see `adoptStartedTurn` in
    /// the frontend). Returns the outcome and the id `turn/start` returned.
    pub fn run_turn_observed(&self, request: Request) -> (TurnOutcome, String) {
        assert_eq!(
            request.method, "turn/start",
            "run_turn_observed wants a turn/start"
        );
        let from = self.cursor();
        let response = self.call(request);
        let requested = response
            .pointer("/turn/id")
            .and_then(Value::as_str)
            .unwrap_or_else(|| panic!("turn/start returned no turn id: {response}"))
            .to_string();
        let (_, started) = self
            .wait_notification(from, "turn/started", REQUEST_TIMEOUT, |_| true)
            .unwrap_or_else(|| {
                panic!(
                    "no turn/started after turn/start\nstderr:\n{}",
                    self.stderr_tail()
                )
            });
        let started_id = started
            .pointer("/turn/id")
            .and_then(Value::as_str)
            .unwrap_or_else(|| panic!("turn/started without id: {started}"))
            .to_string();
        (self.await_turn(from, &started_id), requested)
    }

    /// Wait until every turn started since `from` has completed (goal threads
    /// can start follow-up turns on their own), returning the ids seen.
    pub fn drain_turns(&self, from: usize, settle: Duration) -> Vec<String> {
        loop {
            let messages = self.messages_since(from);
            let started: Vec<String> = messages
                .iter()
                .filter(|m| m.get("method").and_then(Value::as_str) == Some("turn/started"))
                .filter_map(|m| m.pointer("/params/turn/id").and_then(Value::as_str))
                .map(str::to_string)
                .collect();
            let completed: Vec<String> = messages
                .iter()
                .filter(|m| m.get("method").and_then(Value::as_str) == Some("turn/completed"))
                .filter_map(|m| m.pointer("/params/turn/id").and_then(Value::as_str))
                .map(str::to_string)
                .collect();
            if let Some(open) = started.iter().find(|id| !completed.contains(id)) {
                self.await_turn(from, open);
                continue;
            }
            // Give a goal continuation a moment to appear before declaring quiet.
            let cursor = self.cursor();
            if self
                .wait_notification(cursor, "turn/started", settle, |_| true)
                .is_none()
            {
                return started;
            }
        }
    }

    /// Wait for `turn/completed` for `turn_id`, collecting its completed items.
    pub fn await_turn(&self, from: usize, turn_id: &str) -> TurnOutcome {
        let (_, params) = self
            .wait_notification(from, "turn/completed", TURN_TIMEOUT, |params| {
                params.pointer("/turn/id").and_then(Value::as_str) == Some(turn_id)
            })
            .unwrap_or_else(|| {
                panic!(
                    "turn {turn_id} did not complete within {TURN_TIMEOUT:?}\nlast messages:\n{}\nstderr:\n{}",
                    self.messages_since(from)
                        .iter()
                        .rev()
                        .take(8)
                        .map(|m| truncate(&m.to_string(), 300))
                        .collect::<Vec<_>>()
                        .join("\n"),
                    self.stderr_tail()
                )
            });
        let items = self
            .messages_since(from)
            .into_iter()
            .filter(|message| {
                message.get("method").and_then(Value::as_str) == Some("item/completed")
                    && message.pointer("/params/turnId").and_then(Value::as_str) == Some(turn_id)
            })
            .filter_map(|message| message.pointer("/params/item").cloned())
            .collect();
        TurnOutcome {
            turn_id: turn_id.to_string(),
            status: params
                .pointer("/turn/status")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            items,
        }
    }

    /// The path `skills/list` reports for the suite's skill.
    pub fn skill_path(&self) -> String {
        let response = self.call(requests::skills_list(&[self.cwd.display().to_string()]));
        pingex_app_lib::e2e::parse_skills(&response)
            .into_iter()
            .find(|skill| skill.name == SKILL_NAME)
            .unwrap_or_else(|| panic!("{SKILL_NAME} not in skills/list: {response}"))
            .path
    }
}

/// Spawn `codex app-server --stdio` on `codex_home`/`cwd` and complete the
/// `initialize`/`initialized` handshake, the way `spawn_child` does.
fn start_session(codex_home: &Path, cwd: &Path) -> Result<Arc<Inner>, String> {
    // Same invocation as `crate::codex::child::spawn_child`.
    let mut process = Command::new(codex_binary())
        .args(["app-server", "--stdio"])
        .env("CODEX_HOME", codex_home)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("spawn failed: {error}"))?;
    let stdin = process.stdin.take().ok_or("no stdin")?;
    let stdout = process.stdout.take().ok_or("no stdout")?;
    let stderr = process.stderr.take().ok_or("no stderr")?;

    let inner = Arc::new(Inner {
        process: Mutex::new(process),
        stdin: Mutex::new(stdin),
        next_id: AtomicI64::new(1),
        pending: Mutex::new(HashMap::new()),
        messages: Mutex::new(Vec::new()),
        changed: Condvar::new(),
        stderr: Mutex::new(Vec::new()),
    });
    let for_stderr = inner.clone();
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if let Ok(mut tail) = for_stderr.stderr.lock() {
                tail.push(line);
                if tail.len() > 200 {
                    tail.remove(0);
                }
            }
        }
    });
    let for_stdout = inner.clone();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let Ok(message) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            let is_response = message.get("id").is_some() && message.get("method").is_none();
            if is_response {
                if let Some(id) = message.get("id").and_then(Value::as_i64) {
                    let sender = for_stdout
                        .pending
                        .lock()
                        .ok()
                        .and_then(|mut p| p.remove(&id));
                    if let Some(sender) = sender {
                        let _ = sender.send(message.clone());
                    }
                }
            }
            let mut messages = for_stdout.messages.lock().expect("bus lock");
            messages.push(message);
            for_stdout.changed.notify_all();
        }
        for_stdout.changed.notify_all();
    });

    inner
        .request(requests::initialize("pingex-e2e"))
        .map_err(|error| error.to_string())?;
    inner.write(&json!({"method": "initialized", "params": {}}))?;
    Ok(inner)
}

impl Inner {
    fn write(&self, message: &Value) -> Result<(), String> {
        let mut stdin = self.stdin.lock().map_err(|_| "stdin poisoned")?;
        writeln!(stdin, "{message}").map_err(io_err)?;
        stdin.flush().map_err(io_err)
    }

    fn stderr_tail(&self) -> String {
        self.stderr
            .lock()
            .map(|lines| {
                lines
                    .iter()
                    .rev()
                    .take(20)
                    .rev()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default()
    }

    fn request(&self, request: Request) -> Result<Value, RpcError> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (sender, receiver) = mpsc::channel();
        self.pending
            .lock()
            .expect("pending lock")
            .insert(id, sender);
        let message = json!({"id": id, "method": request.method, "params": request.params});
        if let Err(error) = self.write(&message) {
            panic!(
                "could not write {}: {error}\n{}",
                request.method,
                self.stderr_tail()
            );
        }
        let response = receiver.recv_timeout(REQUEST_TIMEOUT).unwrap_or_else(|_| {
            panic!(
                "no response to {} within {:?}\nstderr:\n{}",
                request.method,
                REQUEST_TIMEOUT,
                self.stderr_tail()
            )
        });
        if let Some(error) = response.get("error") {
            return Err(RpcError {
                code: error
                    .get("code")
                    .and_then(Value::as_i64)
                    .unwrap_or_default(),
                message: error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            });
        }
        Ok(response.get("result").cloned().unwrap_or(Value::Null))
    }
}

/// Run an async storage call to completion on a private runtime.
pub fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime")
        .block_on(future)
}

/// Run `git` in `dir`, panicking on failure.
pub fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|error| panic!("git {args:?}: {error}"));
    assert!(
        output.status.success(),
        "git {args:?} in {} failed:\n{}",
        dir.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

pub fn thread_id_of(response: &Value) -> String {
    response
        .pointer("/thread/id")
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("no thread id in {response}"))
        .to_string()
}

pub fn truncate(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}
