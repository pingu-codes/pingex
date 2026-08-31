//! Test double for nothing: a real `claude -p`, spoken to over stdio exactly
//! the way `crate::claude::child` does — same base argv (re-exported through
//! `pingex_app_lib::e2e`), same env, one process per test.
//!
//! Opt-in: every test returns early unless `PINGEX_LIVE_E2E=1` (it spends real
//! tokens and needs a Claude Code login). Knobs:
//!
//! - `PINGEX_E2E_CLAUDE` — claude binary (default: resolved like the app does)
//! - `PINGEX_E2E_CLAUDE_MODEL` — model alias (default `haiku`)
//! - `PINGEX_E2E_CLAUDE_CONFIG` — source config dir (default
//!   `~/.claude-personal`, then `~/.claude`)
//! - `PINGEX_E2E_CLAUDE_REUSE_CONFIG=1` — force running against the source
//!   config dir directly instead of a scratch copy
//!
//! When the source dir has a `.credentials.json` it is copied into a scratch
//! `CLAUDE_CONFIG_DIR` (isolation, like live_codex's `auth.json` copy). On
//! macOS the OAuth token often lives only in the Keychain — no file to copy —
//! so the suite falls back to the source dir itself, read-only in spirit:
//! the tests only ever write inside their scratch working directory.

use pingex_app_lib::e2e::{
    claude_permission_result, claude_turn_args, resolve_codex_binary, CLAUDE_BASE_ARGS,
};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

pub const TURN_TIMEOUT: Duration = Duration::from_secs(180);

/// Suite-wide scratch state: where the config and working directories live.
pub struct Setup {
    pub binary: PathBuf,
    pub config_dir: PathBuf,
    pub work: PathBuf,
    pub model: String,
    // Held so the scratch directories outlive every test.
    _root: tempfile::TempDir,
}

static SETUP: OnceLock<Option<Setup>> = OnceLock::new();

/// The suite's scratch setup, or `None` when the suite is disabled.
pub fn setup() -> Option<&'static Setup> {
    SETUP
        .get_or_init(|| {
            if std::env::var("PINGEX_LIVE_E2E").ok().as_deref() != Some("1") {
                eprintln!("live e2e skipped: set PINGEX_LIVE_E2E=1 to run against a real claude");
                return None;
            }
            Some(build_setup().unwrap_or_else(|error| panic!("could not set up claude: {error}")))
        })
        .as_ref()
}

/// Return early from a test unless the suite is enabled.
#[macro_export]
macro_rules! live {
    () => {
        match $crate::harness::setup() {
            Some(setup) => setup,
            None => return,
        }
    };
}

fn home_dir() -> PathBuf {
    dirs::home_dir().expect("home directory")
}

/// Where the user's Claude credentials come from.
fn config_source() -> PathBuf {
    if let Ok(dir) = std::env::var("PINGEX_E2E_CLAUDE_CONFIG") {
        return PathBuf::from(dir);
    }
    for candidate in [".claude-personal", ".claude"] {
        let dir = home_dir().join(candidate);
        if dir.is_dir() {
            return dir;
        }
    }
    home_dir().join(".claude")
}

fn build_setup() -> Result<Setup, String> {
    let binary = match std::env::var("PINGEX_E2E_CLAUDE") {
        Ok(path) => PathBuf::from(path),
        Err(_) => resolve_codex_binary(Path::new("claude"))
            .ok_or("no claude binary found; set PINGEX_E2E_CLAUDE")?,
    };
    let model = std::env::var("PINGEX_E2E_CLAUDE_MODEL").unwrap_or_else(|_| "haiku".to_string());
    let root = tempfile::tempdir().map_err(|error| error.to_string())?;
    let source = config_source();
    let reuse = std::env::var("PINGEX_E2E_CLAUDE_REUSE_CONFIG")
        .ok()
        .as_deref()
        == Some("1");
    let credentials = source.join(".credentials.json");
    // Keychain-only login: there is nothing to copy, so a scratch dir would
    // start every turn with "Not logged in". Use the source dir directly.
    let config_dir = if reuse || !credentials.is_file() {
        source
    } else {
        let scratch = root.path().join("config");
        std::fs::create_dir_all(&scratch).map_err(|error| error.to_string())?;
        std::fs::copy(&credentials, scratch.join(".credentials.json"))
            .map_err(|error| error.to_string())?;
        scratch
    };
    let work = root.path().join("work");
    std::fs::create_dir_all(&work).map_err(|error| error.to_string())?;
    std::fs::write(
        work.join("CLAUDE.md"),
        "Answer as tersely as possible. No preamble, no follow-up questions.\n",
    )
    .map_err(|error| error.to_string())?;
    Ok(Setup {
        binary,
        config_dir,
        work,
        model,
        _root: root,
    })
}

/// One `claude -p` process and the frames it has produced so far.
pub struct ClaudeProcess {
    process: Mutex<Child>,
    stdin: Mutex<ChildStdin>,
    frames: Arc<Mutex<Vec<Value>>>,
    changed: Arc<Condvar>,
    stderr: Arc<Mutex<Vec<String>>>,
    pub session_id: String,
}

impl Drop for ClaudeProcess {
    fn drop(&mut self) {
        if let Ok(mut process) = self.process.lock() {
            let _ = process.kill();
        }
    }
}

/// Spawn one process the way `claude::child::spawn` + `driver::turn_args` do:
/// the shared base argv, the per-session args, `CLAUDE_CODE_ENTRYPOINT` and
/// `CLAUDE_CONFIG_DIR`, API-key variables stripped, cwd in the scratch dir.
pub fn spawn(setup: &Setup, mode: &str) -> ClaudeProcess {
    let session_id = uuid::Uuid::new_v4().to_string();
    let args = claude_turn_args(Some(&setup.model), Some("low"), mode, false, &session_id);
    let mut process = Command::new(&setup.binary)
        .args(CLAUDE_BASE_ARGS)
        .args(&args)
        .current_dir(&setup.work)
        .env("CLAUDE_CODE_ENTRYPOINT", "sdk-cli")
        .env("CLAUDE_CONFIG_DIR", &setup.config_dir)
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("ANTHROPIC_AUTH_TOKEN")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|error| panic!("could not start {}: {error}", setup.binary.display()));
    let stdin = process.stdin.take().expect("claude stdin");
    let stdout = process.stdout.take().expect("claude stdout");
    let stderr = process.stderr.take().expect("claude stderr");

    let frames = Arc::new(Mutex::new(Vec::new()));
    let changed = Arc::new(Condvar::new());
    let stderr_tail = Arc::new(Mutex::new(Vec::new()));

    let for_stderr = stderr_tail.clone();
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if let Ok(mut tail) = for_stderr.lock() {
                tail.push(line);
                if tail.len() > 200 {
                    tail.remove(0);
                }
            }
        }
    });
    let for_frames = frames.clone();
    let for_changed = changed.clone();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let Ok(frame) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            let mut frames = for_frames.lock().expect("frames lock");
            frames.push(frame);
            for_changed.notify_all();
        }
        for_changed.notify_all();
    });

    ClaudeProcess {
        process: Mutex::new(process),
        stdin: Mutex::new(stdin),
        frames,
        changed,
        stderr: stderr_tail,
        session_id,
    }
}

impl ClaudeProcess {
    pub fn write_frame(&self, frame: &Value) {
        let mut stdin = self.stdin.lock().expect("stdin lock");
        writeln!(stdin, "{frame}").expect("write frame");
        stdin.flush().expect("flush frame");
    }

    /// Send a prompt the way `driver::start_turn` builds its `user` frame.
    pub fn send_prompt(&self, text: &str) {
        self.write_frame(&json!({
            "type": "user",
            "session_id": "",
            "parent_tool_use_id": null,
            "uuid": uuid::Uuid::new_v4().to_string(),
            "origin": {"kind": "human"},
            "message": {"role": "user", "content": [{"type": "text", "text": text}]},
        }));
    }

    /// Answer a CLI control request the way `ClaudeChild::respond_control` does.
    pub fn respond_control(&self, request_id: &str, response: Value) {
        self.write_frame(&json!({
            "type": "control_response",
            "response": {"subtype": "success", "request_id": request_id, "response": response},
        }));
    }

    /// Answer a `can_use_tool` with one of the app's own option ids
    /// (`allow`, `deny`, …), through the driver's real mapping.
    pub fn respond_permission(&self, request_id: &str, option_id: &str, can_use_tool: &Value) {
        self.respond_control(
            request_id,
            claude_permission_result(option_id, can_use_tool),
        );
    }

    /// Block until a frame at or after `from` matches; `(index + 1, frame)`.
    pub fn wait_for(
        &self,
        from: usize,
        timeout: Duration,
        mut predicate: impl FnMut(&Value) -> bool,
    ) -> Option<(usize, Value)> {
        let deadline = Instant::now() + timeout;
        let mut frames = self.frames.lock().expect("frames lock");
        let mut index = from;
        loop {
            while index < frames.len() {
                if predicate(&frames[index]) {
                    return Some((index + 1, frames[index].clone()));
                }
                index += 1;
            }
            let now = Instant::now();
            if now >= deadline {
                return None;
            }
            let (guard, _) = self
                .changed
                .wait_timeout(frames, deadline - now)
                .expect("frames lock");
            frames = guard;
        }
    }

    /// Wait for a frame with this `type`, panicking with diagnostics on timeout.
    pub fn expect_frame(&self, from: usize, kind: &str, what: &str) -> (usize, Value) {
        self.wait_for(from, TURN_TIMEOUT, |frame| {
            frame.get("type").and_then(Value::as_str) == Some(kind)
        })
        .unwrap_or_else(|| panic!("timed out waiting for {what}\n{}", self.diagnostics(from)))
    }

    /// Wait for the CLI's `can_use_tool` control request; `(next, request_id, request)`.
    pub fn expect_can_use_tool(&self, from: usize) -> (usize, String, Value) {
        let (next, frame) = self
            .wait_for(from, TURN_TIMEOUT, |frame| {
                frame.get("type").and_then(Value::as_str) == Some("control_request")
                    && frame.pointer("/request/subtype").and_then(Value::as_str)
                        == Some("can_use_tool")
            })
            .unwrap_or_else(|| {
                panic!(
                    "timed out waiting for can_use_tool\n{}",
                    self.diagnostics(from)
                )
            });
        let request_id = frame["request_id"]
            .as_str()
            .expect("request_id")
            .to_string();
        (next, request_id, frame["request"].clone())
    }

    /// Frames since `from` plus the stderr tail, for panic messages.
    pub fn diagnostics(&self, from: usize) -> String {
        let frames = self.frames.lock().expect("frames lock")[from..]
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        let stderr = self
            .stderr
            .lock()
            .map(|lines| lines.join("\n"))
            .unwrap_or_default();
        format!("frames:\n{frames}\nstderr:\n{stderr}")
    }

    /// Assert a `result` frame ended a turn successfully and return it.
    pub fn expect_success(&self, from: usize, what: &str) -> (usize, Value) {
        let (next, result) = self.expect_frame(from, "result", what);
        let subtype = result["subtype"].as_str().unwrap_or("");
        let is_error = result["is_error"].as_bool() == Some(true);
        assert!(
            subtype == "success" && !is_error,
            "{what} failed: {result}\n{}",
            self.diagnostics(from)
        );
        (next, result)
    }
}
