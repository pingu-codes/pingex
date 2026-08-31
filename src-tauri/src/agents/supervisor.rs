//! The registry of running subagent processes.
//!
//! Each spawned agent is its own `codex app-server` child, driving its own real
//! Codex thread. That buys the GUI a lot for free — the thread is readable with
//! `thread/read`, renders in `ThreadView` like any other, and survives the app
//! — but it also means nobody else is watching it: a subagent has no user to
//! approve a command, so it runs under `approvalPolicy: "never"` and anything
//! that still asks is answered defensively here.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::AppHandle;
use tauri_specta::Event;
use tokio::sync::watch;

use crate::agents::tools;
use crate::codex::child::{spawn_child, ChildSink, CodexChild};
use crate::codex::events::{CodexEvent, CodexNotification};
use crate::codex::journal::TurnJournal;
use crate::codex::requests;
use crate::settings::prefs::AgentSettings;
use crate::storage::{self, AgentRunRow};
use crate::util::time::unix_millis;
use crate::HomeContext;

/// Where a run has got to. `Failed` carries the reason so the parent's
/// `wait_agents` can report something better than "it didn't work".
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AgentRunState {
    Starting,
    Running,
    Done,
    Failed(String),
    Killed,
}

impl AgentRunState {
    pub(crate) fn is_terminal(&self) -> bool {
        !matches!(self, Self::Starting | Self::Running)
    }

    pub(crate) fn status(&self) -> &'static str {
        match self {
            Self::Starting | Self::Running => storage::STATUS_RUNNING,
            Self::Done => storage::STATUS_DONE,
            Self::Failed(_) => storage::STATUS_FAILED,
            Self::Killed => storage::STATUS_KILLED,
        }
    }
}

pub(crate) struct AgentRun {
    pub(crate) id: String,
    pub(crate) parent_thread_id: String,
    /// The `item/tool/call` id, which is also the id of the `dynamicToolCall`
    /// item in the parent's transcript. Carried on every update because it is
    /// how the GUI joins a transcript row to the run behind it.
    pub(crate) call_id: Option<String>,
    pub(crate) name: String,
    pub(crate) created_at: i64,
    child: Mutex<Option<Arc<CodexChild>>>,
    child_thread_id: Mutex<Option<String>>,
    /// The turn currently in flight on the child, if any. Used to recognise
    /// which `turn/completed` finishes this run, and to interrupt on kill.
    current_turn_id: Mutex<Option<String>>,
    /// The most recent agent message. Whatever the agent said last *is* the
    /// result — there is no separate return value in the protocol.
    last_message: Mutex<String>,
    state: watch::Sender<AgentRunState>,
}

impl AgentRun {
    pub(crate) fn state(&self) -> AgentRunState {
        self.state.borrow().clone()
    }

    pub(crate) fn subscribe(&self) -> watch::Receiver<AgentRunState> {
        self.state.subscribe()
    }

    pub(crate) fn child_thread_id(&self) -> Option<String> {
        self.child_thread_id.lock().ok().and_then(|id| id.clone())
    }

    pub(crate) fn last_message(&self) -> String {
        self.last_message
            .lock()
            .map(|message| message.clone())
            .unwrap_or_default()
    }

    fn set_state(&self, next: AgentRunState) {
        // `send` only fails when every receiver is gone, which is not a reason
        // to stop tracking the run.
        let _ = self.state.send(next);
    }
}

/// Everything the supervisor needs that outlives one call.
pub(crate) struct AgentSupervisor {
    runs: Mutex<HashMap<String, Arc<AgentRun>>>,
    next_id: AtomicU64,
    /// Namespaces the run ids so a fresh launch cannot reuse one.
    ///
    /// The counter restarts at 1 every launch, and `agent_runs` is keyed by run
    /// id: without this, the first agent of a session collides with the first
    /// agent of the last one. The insert becomes an update of somebody else's
    /// row — which keeps its original parent thread, so the new run never
    /// appears under the thread that spawned it, and the old thread's card
    /// starts pointing at a transcript that is not its own.
    launched_at: i64,
    /// Working directory per thread, remembered when the thread is started.
    ///
    /// A spawn is bounded by its parent's cwd, and the obvious ways to look
    /// that up are both bad here: the cached summaries do not yet contain a
    /// thread created moments ago, and asking Codex means a request on a
    /// thread whose turn is currently blocked waiting for this very tool call.
    thread_cwds: Mutex<HashMap<String, String>>,
}

impl Default for AgentSupervisor {
    fn default() -> Self {
        Self {
            runs: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            launched_at: unix_millis(),
            thread_cwds: Mutex::new(HashMap::new()),
        }
    }
}

impl AgentSupervisor {
    pub(crate) fn remember_cwd(&self, thread_id: &str, cwd: &str) {
        if let Ok(mut cwds) = self.thread_cwds.lock() {
            cwds.insert(thread_id.to_string(), cwd.to_string());
        }
    }

    pub(crate) fn cwd_for(&self, thread_id: &str) -> Option<String> {
        self.thread_cwds.lock().ok()?.get(thread_id).cloned()
    }
}

impl AgentSupervisor {
    pub(crate) fn get(&self, run_id: &str) -> Option<Arc<AgentRun>> {
        self.runs.lock().ok()?.get(run_id).cloned()
    }

    fn insert(&self, run: Arc<AgentRun>) {
        if let Ok(mut runs) = self.runs.lock() {
            runs.insert(run.id.clone(), run);
        }
    }

    /// How many runs are still going. Used to enforce the concurrency cap
    /// without holding a permit across the whole (possibly very long) run.
    fn running_count(&self) -> usize {
        self.runs
            .lock()
            .map(|runs| {
                runs.values()
                    .filter(|run| !run.state().is_terminal())
                    .count()
            })
            .unwrap_or(0)
    }

    fn next_run_id(&self) -> String {
        format!(
            "agt_{}_{}",
            self.launched_at,
            self.next_id.fetch_add(1, Ordering::SeqCst)
        )
    }

    /// Kill every running agent. Called on app exit and on a Codex-home switch
    /// (where their `CODEX_HOME` has just become wrong).
    pub(crate) fn kill_all(&self) {
        let runs: Vec<Arc<AgentRun>> = self
            .runs
            .lock()
            .map(|runs| runs.values().cloned().collect())
            .unwrap_or_default();
        for run in runs {
            if run.state().is_terminal() {
                continue;
            }
            if let Ok(mut child) = run.child.lock() {
                if let Some(child) = child.take() {
                    child.kill();
                }
            }
            run.set_state(AgentRunState::Killed);
        }
    }
}

/// What a subagent's child says.
///
/// Everything is forwarded to the frontend as `codex:event`, exactly as the
/// main session's notifications are. A subagent drives a real thread the user
/// can open, and without this that thread is dead on the screen: its items only
/// exist in this process, so an open transcript would sit frozen at whatever
/// `thread/read` returned while the agent kept working. The events are keyed by
/// the child's own thread id, so nothing reaches the parent's view — the
/// reducers already filter on it.
///
/// Its items are journaled exactly as the main session's are, through the same
/// `TurnJournal`: a re-read of the child's thread hands back the conversation
/// and drops the work, and a child that recorded its items without recording
/// its turn boundaries would be re-read by splicing into Codex's renumbered
/// projection — which shows every agent message twice.
struct AgentSink {
    run: Mutex<Option<Arc<AgentRun>>>,
    app: AppHandle,
    /// Canonical home of the context that spawned this agent — the tag on its
    /// events and the key its journal writes resolve their database through.
    home_key: String,
    journal: TurnJournal,
}

impl AgentSink {
    fn new(app: AppHandle, home_key: String) -> Arc<Self> {
        Arc::new(Self {
            run: Mutex::new(None),
            journal: TurnJournal::new(app.clone(), home_key.clone()),
            home_key,
            app,
        })
    }

    fn attach(&self, run: Arc<AgentRun>) {
        if let Ok(mut slot) = self.run.lock() {
            *slot = Some(run);
        }
    }

    fn run(&self) -> Option<Arc<AgentRun>> {
        self.run.lock().ok().and_then(|run| run.clone())
    }
}

impl ChildSink for AgentSink {
    fn on_notification(&self, method: &str, params: &Value) {
        self.journal.observe(method, params);
        let _ = CodexEvent {
            codex_home: self.home_key.clone(),
            event: CodexNotification::decode(method, params),
        }
        .emit(&self.app);
        let Some(run) = self.run() else {
            return;
        };
        if let Some(next) = apply_child_event(&run, method, params) {
            finish(&self.app, &self.home_key, &run, with_stderr(&run, next));
        }
    }

    fn on_server_request(&self, child: &Arc<CodexChild>, id: i64, method: &str, _params: &Value) {
        // A subagent has nobody to ask. Under `approvalPolicy: "never"` these
        // should not arrive at all; if one does, answer it rather than let the
        // child block forever waiting on a user who will never see it.
        let result = match method {
            "item/commandExecution/requestApproval" | "item/fileChange/requestApproval" => {
                json!({"decision": "denied"})
            }
            "item/tool/requestUserInput" => json!({"answers": {}}),
            // Subagents are spawned without dynamicTools, so this is unexpected.
            "item/tool/call" => json!({"contentItems": [], "success": false}),
            _ => json!({}),
        };
        let _ = child.respond(id, result);
    }

    fn on_closed(&self) {
        let Some(run) = self.run() else {
            return;
        };
        if !run.state().is_terminal() {
            let state = AgentRunState::Failed("The agent's process exited unexpectedly.".into());
            finish(&self.app, &self.home_key, &run, with_stderr(&run, state));
        }
    }
}

/// Fold one child notification into a run, returning the state it moved to.
///
/// Pure with respect to the process: it only reads the notification and the
/// run's own mutexes, so the whole lifecycle is testable without spawning
/// anything.
pub(crate) fn apply_child_event(
    run: &AgentRun,
    method: &str,
    params: &Value,
) -> Option<AgentRunState> {
    match method {
        "item/completed" => {
            let item = params.get("item")?;
            if item.get("type").and_then(Value::as_str) == Some("agentMessage") {
                let text = item.get("text").and_then(Value::as_str).unwrap_or_default();
                if !text.trim().is_empty() {
                    if let Ok(mut last) = run.last_message.lock() {
                        *last = text.to_string();
                    }
                }
            }
            None
        }
        "turn/completed" => {
            // Only the turn we are waiting on ends the run; a follow-up sent
            // with `pingex_send_input` starts a new one.
            let turn = params.get("turn")?;
            let turn_id = turn.get("id").and_then(Value::as_str)?;
            let current = run.current_turn_id.lock().ok()?.clone();
            if current.as_deref() != Some(turn_id) {
                return None;
            }
            if let Ok(mut slot) = run.current_turn_id.lock() {
                *slot = None;
            }
            match turn
                .get("error")
                .and_then(|error| error.get("message"))
                .and_then(Value::as_str)
            {
                Some(message) => Some(AgentRunState::Failed(message.to_string())),
                None => Some(AgentRunState::Done),
            }
        }
        "error" => Some(AgentRunState::Failed(error_message(params))),
        _ => None,
    }
}

/// Append what the child process wrote to stderr to a failure reason.
///
/// The protocol-level message is often generic ("stream error"); the process
/// itself usually says something specific, and without this that is lost.
fn with_stderr(run: &Arc<AgentRun>, state: AgentRunState) -> AgentRunState {
    let AgentRunState::Failed(message) = state else {
        return state;
    };
    let tail = run
        .child
        .lock()
        .ok()
        .and_then(|child| child.as_ref().map(|child| child.stderr_tail()))
        .unwrap_or_default();
    let tail = tail.trim();
    if tail.is_empty() {
        return AgentRunState::Failed(message);
    }
    AgentRunState::Failed(format!("{message}\n{tail}"))
}

/// Pull the human-readable reason out of an `error` notification.
///
/// The payload nests it under `error` (the same shape the frontend's thread
/// reducer reads); a bare `message` is accepted too. Without this the reason is
/// silently replaced by a placeholder, which makes every failure look alike.
fn error_message(params: &Value) -> String {
    params
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .or_else(|| params.get("message").and_then(Value::as_str))
        .map(str::to_string)
        .unwrap_or_else(|| format!("The agent reported an error: {params}"))
}

/// Move a run to a terminal state: persist it, tell the GUI, and release the
/// process if it is still holding one.
fn finish(app: &AppHandle, home_key: &str, run: &Arc<AgentRun>, next: AgentRunState) {
    if run.state().is_terminal() {
        return;
    }
    let terminal = next.is_terminal();
    run.set_state(next.clone());
    if terminal {
        if let Ok(mut child) = run.child.lock() {
            if let Some(child) = child.take() {
                child.kill();
            }
        }
    }
    persist(app, home_key, run, &next);
}

/// `codex:agentRun` — a run changed state. The frontend merges it into the
/// runs it already holds for the parent thread.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type, tauri_specta::Event)]
#[tauri_specta(event_name = "codex:agentRun")]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexAgentRun {
    pub codex_home: String,
    pub run_id: String,
    pub parent_thread_id: String,
    /// Joins this run to its `dynamicToolCall` row in the parent's transcript.
    pub call_id: Option<String>,
    pub child_thread_id: Option<String>,
    pub name: String,
    pub status: String,
    pub result: String,
    pub error: Option<String>,
}

/// Write the run's current shape back to the database and emit `codex:agentRun`
/// so the GUI can update without polling. Best effort: a failed write must not
/// disturb the run it describes.
fn persist(app: &AppHandle, home_key: &str, run: &Arc<AgentRun>, state: &AgentRunState) {
    let _ = CodexAgentRun {
        codex_home: home_key.to_string(),
        run_id: run.id.clone(),
        parent_thread_id: run.parent_thread_id.clone(),
        call_id: run.call_id.clone(),
        child_thread_id: run.child_thread_id(),
        name: run.name.clone(),
        status: state.status().to_string(),
        result: run.last_message(),
        error: match state {
            AgentRunState::Failed(message) => Some(message.clone()),
            _ => None,
        },
    }
    .emit(app);

    let (app, home_key) = (app.clone(), home_key.to_string());
    let (run_id, status) = (run.id.clone(), state.status().to_string());
    let child_thread_id = run.child_thread_id();
    let result = run.last_message();
    let error = match state {
        AgentRunState::Failed(message) => Some(message.clone()),
        _ => None,
    };
    let finished_at = state.is_terminal().then(unix_millis);
    tauri::async_runtime::spawn(async move {
        let Some(database) = crate::database_for(&app, &home_key) else {
            return;
        };
        let _ = storage::update_agent_run(
            &database,
            &run_id,
            Some(&status),
            child_thread_id.as_deref(),
            // Written even when empty, so a follow-up turn clears the answer it
            // supersedes rather than leaving it beside a running agent.
            Some(result.as_str()),
            error.as_deref(),
            finished_at,
        )
        .await;
    });
}

/// Which turn of which thread asked for this agent, and where it may run.
pub(crate) struct SpawnContext<'a> {
    pub(crate) parent_thread_id: &'a str,
    pub(crate) parent_turn_id: &'a str,
    /// The `item/tool/call` id, which is also the transcript item's id — the
    /// only link between the row the GUI shows and this run.
    pub(crate) call_id: Option<&'a str>,
    /// Bounds where the agent may work: a spawn cannot escape it.
    pub(crate) parent_cwd: &'a std::path::Path,
}

/// Start an agent: its own process, its own thread, one turn.
///
/// Returns as soon as the turn is accepted, so a parent can fan out several
/// before waiting on any of them.
pub(crate) async fn spawn_agent(
    app: &AppHandle,
    ctx: &HomeContext,
    settings: &AgentSettings,
    parent: SpawnContext<'_>,
    args: tools::SpawnArgs,
) -> Result<Arc<AgentRun>, String> {
    let SpawnContext {
        parent_thread_id,
        parent_turn_id,
        call_id,
        parent_cwd,
    } = parent;
    if ctx.agents.running_count() >= settings.max_concurrent {
        return Err(format!(
            "{} agents are already running (the limit is {}). Wait for one to finish first.",
            ctx.agents.running_count(),
            settings.max_concurrent
        ));
    }
    let cwd = tools::resolve_cwd(parent_cwd, args.cwd.as_deref())?;
    let sandbox = tools::clamp_sandbox(args.sandbox.as_deref(), &settings.sandbox);
    let prompt = tools::attach_files(&args.prompt, &cwd, &args.files);

    let runtime = ctx.runtime();
    let program = crate::codex::binary::resolve(&runtime.codex_binary)
        .ok_or_else(|| crate::codex::binary::missing_message(&runtime.codex_binary))?;

    let run_id = ctx.agents.next_run_id();
    let (sender, _) = watch::channel(AgentRunState::Starting);
    let run = Arc::new(AgentRun {
        id: run_id.clone(),
        parent_thread_id: parent_thread_id.to_string(),
        call_id: call_id.map(str::to_string),
        name: args.name.clone(),
        created_at: unix_millis(),
        child: Mutex::new(None),
        child_thread_id: Mutex::new(None),
        current_turn_id: Mutex::new(None),
        last_message: Mutex::new(String::new()),
        state: sender,
    });
    ctx.agents.insert(run.clone());

    storage::record_agent_run(
        &ctx.database(),
        &AgentRunRow {
            run_id: run_id.clone(),
            parent_thread_id: parent_thread_id.to_string(),
            parent_turn_id: parent_turn_id.to_string(),
            call_id: call_id.map(str::to_string),
            child_thread_id: None,
            name: args.name.clone(),
            prompt: prompt.clone(),
            cwd: cwd.display().to_string(),
            model: args.model.clone(),
            reasoning_effort: args.effort.clone(),
            status: storage::STATUS_RUNNING.to_string(),
            result: None,
            error: None,
            created_at: run.created_at,
            finished_at: None,
        },
    )
    .await?;

    // Announce the run before the process exists, so its transcript card can
    // show a name and a status straight away rather than after two round trips.
    persist(app, &ctx.home_key, &run, &AgentRunState::Starting);

    let sink = AgentSink::new(app.clone(), ctx.home_key.clone());
    sink.attach(run.clone());
    let child = match spawn_child(
        &program,
        &runtime.codex_home,
        "pingex-agent",
        app.clone(),
        ctx.session.wire().clone(),
        sink.clone(),
    )
    .await
    {
        Ok(child) => child,
        Err(error) => {
            finish(
                app,
                &ctx.home_key,
                &run,
                AgentRunState::Failed(error.clone()),
            );
            return Err(error);
        }
    };
    if let Ok(mut slot) = run.child.lock() {
        *slot = Some(child.clone());
    }

    // No `dynamicTools` for the child: an agent that could spawn agents is a
    // fork bomb one prompt away.
    let started = child
        .send(requests::agent_thread_start(
            &cwd.display().to_string(),
            AGENT_PREAMBLE,
        ))
        .await;
    let child_thread_id = match started {
        Ok(response) => response
            .get("thread")
            .and_then(|thread| thread.get("id"))
            .and_then(Value::as_str)
            .map(str::to_string),
        Err(error) => {
            finish(
                app,
                &ctx.home_key,
                &run,
                AgentRunState::Failed(error.clone()),
            );
            return Err(error);
        }
    };
    let Some(child_thread_id) = child_thread_id else {
        let error = "The agent's process returned no thread.".to_string();
        finish(
            app,
            &ctx.home_key,
            &run,
            AgentRunState::Failed(error.clone()),
        );
        return Err(error);
    };
    if let Ok(mut slot) = run.child_thread_id.lock() {
        *slot = Some(child_thread_id.clone());
    }

    // An unusable model is only rejected once inference starts, which kills the
    // agent outright — and the model picks this string, so "use the luna
    // subagents" becomes `model: "luna"`. Drop what the account cannot run and
    // let the default stand, the same way an unknown sandbox is dropped.
    let model = match &args.model {
        Some(requested) => usable_model(&child, requested).await,
        None => None,
    };
    let request = requests::agent_turn(
        &child_thread_id,
        &prompt,
        sandbox_tag(&sandbox),
        model.as_deref(),
        args.effort.as_deref(),
    );
    // Record what the agent is really running on, not what was asked for.
    let _ = storage::update_agent_run(
        &ctx.database(),
        &run_id,
        None,
        Some(&child_thread_id),
        None,
        None,
        None,
    )
    .await;
    match child.send(request).await {
        Ok(response) => {
            let turn_id = response
                .get("turn")
                .and_then(|turn| turn.get("id"))
                .and_then(Value::as_str)
                .map(str::to_string);
            if let Ok(mut slot) = run.current_turn_id.lock() {
                *slot = turn_id;
            }
        }
        Err(error) => {
            finish(
                app,
                &ctx.home_key,
                &run,
                AgentRunState::Failed(error.clone()),
            );
            return Err(error);
        }
    }

    run.set_state(AgentRunState::Running);
    persist(app, &ctx.home_key, &run, &AgentRunState::Running);
    spawn_deadline(
        app.clone(),
        ctx.home_key.clone(),
        run.clone(),
        settings.timeout_seconds,
    );
    Ok(run)
}

/// Kill a run that overruns its budget, so a wedged agent cannot occupy a
/// concurrency slot forever.
fn spawn_deadline(app: AppHandle, home_key: String, run: Arc<AgentRun>, timeout_seconds: u64) {
    tauri::async_runtime::spawn(async move {
        let mut receiver = run.subscribe();
        let deadline = tokio::time::sleep(std::time::Duration::from_secs(timeout_seconds));
        tokio::pin!(deadline);
        loop {
            tokio::select! {
                _ = &mut deadline => {
                    finish(
                        &app,
                        &home_key,
                        &run,
                        AgentRunState::Failed(format!(
                            "The agent ran longer than {timeout_seconds}s and was stopped."
                        )),
                    );
                    return;
                }
                changed = receiver.changed() => {
                    if changed.is_err() || receiver.borrow().is_terminal() {
                        return;
                    }
                }
            }
        }
    });
}

/// The requested model if the account can actually run it, else `None`.
///
/// Asked of the child rather than the main session: the child is idle at this
/// point, while the parent's thread is mid-turn, blocked waiting for the very
/// tool call this is serving.
async fn usable_model(child: &Arc<CodexChild>, requested: &str) -> Option<String> {
    let response = child.send(requests::model_list(100, true)).await.ok()?;
    let known = collect_model_ids(&response);
    // An empty list means the lookup told us nothing, not that no model is
    // valid — passing the request through is better than silently ignoring it.
    if known.is_empty() || known.iter().any(|id| id == requested) {
        return Some(requested.to_string());
    }
    None
}

/// Model ids out of a `model/list` response.
pub fn collect_model_ids(response: &Value) -> Vec<String> {
    response
        .get("data")
        .or_else(|| response.get("models"))
        .and_then(Value::as_array)
        .map(|models| {
            models
                .iter()
                .filter_map(|model| model.get("id").and_then(Value::as_str))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

pub fn sandbox_tag(sandbox: &str) -> &'static str {
    match sandbox {
        "read-only" => "readOnly",
        _ => "workspaceWrite",
    }
}

/// Send a follow-up to an agent whose current turn has finished.
pub(crate) async fn send_input(
    app: &AppHandle,
    home_key: &str,
    run: &Arc<AgentRun>,
    text: &str,
) -> Result<(), String> {
    if run.state().is_terminal() && run.state() != AgentRunState::Done {
        return Err("That agent is no longer running.".into());
    }
    let child = run
        .child
        .lock()
        .ok()
        .and_then(|child| child.clone())
        .ok_or("That agent's process has already exited.")?;
    let thread_id = run
        .child_thread_id()
        .ok_or("That agent has not started a thread yet.")?;
    if run
        .current_turn_id
        .lock()
        .ok()
        .and_then(|turn| turn.clone())
        .is_some()
    {
        return Err("That agent is still working; wait for it before sending more input.".into());
    }
    let response = child
        .send(requests::agent_followup(&thread_id, text))
        .await?;
    if let Ok(mut slot) = run.current_turn_id.lock() {
        *slot = response
            .get("turn")
            .and_then(|turn| turn.get("id"))
            .and_then(Value::as_str)
            .map(str::to_string);
    }
    // The last agent message *is* the result, so the previous turn's answer is
    // no longer one: leaving it in place shows the agent working beside the
    // reply it has already superseded.
    if let Ok(mut last) = run.last_message.lock() {
        last.clear();
    }
    run.set_state(AgentRunState::Running);
    persist(app, home_key, run, &AgentRunState::Running);
    Ok(())
}

/// Stop an agent: interrupt the turn if we can, then kill the process.
pub(crate) async fn kill(
    app: &AppHandle,
    home_key: &str,
    run: &Arc<AgentRun>,
    reason: Option<&str>,
) {
    let child = run.child.lock().ok().and_then(|child| child.clone());
    if let (Some(child), Some(thread_id), Some(turn_id)) = (
        child.as_ref(),
        run.child_thread_id(),
        run.current_turn_id
            .lock()
            .ok()
            .and_then(|turn| turn.clone()),
    ) {
        // Best effort: a wedged child may never answer, which is exactly the
        // case kill exists for.
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            child.send(requests::turn_interrupt(&thread_id, &turn_id)),
        )
        .await;
    }
    let next = match reason {
        Some(reason) if !reason.trim().is_empty() => {
            AgentRunState::Failed(format!("Stopped: {}", reason.trim()))
        }
        _ => AgentRunState::Killed,
    };
    finish(app, home_key, run, next);
}

/// The developer instructions every spawned agent starts with.
pub const AGENT_PREAMBLE: &str = "\
You are a background agent spawned by Pingex to carry out one task.

You are running on your own, with no user watching: nobody can answer a \
question or approve anything for you. Do not ask for confirmation — make a \
reasonable decision and say what you assumed.

Your final message is the whole result, and it is the only thing your caller \
sees. Make it self-contained: state what you found or did, with the specifics \
(paths, names, numbers) rather than a summary that assumes shared context.";

#[cfg(test)]
mod tests {
    use super::*;

    fn run() -> AgentRun {
        let (sender, _) = watch::channel(AgentRunState::Running);
        AgentRun {
            id: "agt_1".into(),
            parent_thread_id: "thread-1".into(),
            call_id: Some("call-1".into()),
            name: "probe".into(),
            created_at: 0,
            child: Mutex::new(None),
            child_thread_id: Mutex::new(Some("child-1".into())),
            current_turn_id: Mutex::new(Some("turn-1".into())),
            last_message: Mutex::new(String::new()),
            state: sender,
        }
    }

    fn completed(item: Value) -> Value {
        json!({"threadId": "child-1", "turnId": "turn-1", "item": item})
    }

    #[test]
    fn the_last_agent_message_becomes_the_result() {
        let run = run();
        assert!(apply_child_event(
            &run,
            "item/completed",
            &completed(json!({"id": "m1", "type": "agentMessage", "text": "first"}))
        )
        .is_none());
        assert_eq!(run.last_message(), "first");

        apply_child_event(
            &run,
            "item/completed",
            &completed(json!({"id": "m2", "type": "agentMessage", "text": "second"})),
        );
        assert_eq!(run.last_message(), "second");
    }

    #[test]
    fn other_items_and_empty_messages_do_not_disturb_the_result() {
        let run = run();
        apply_child_event(
            &run,
            "item/completed",
            &completed(json!({"id": "m1", "type": "agentMessage", "text": "kept"})),
        );
        apply_child_event(
            &run,
            "item/completed",
            &completed(json!({"id": "c1", "type": "commandExecution", "text": "ls"})),
        );
        apply_child_event(
            &run,
            "item/completed",
            &completed(json!({"id": "m2", "type": "agentMessage", "text": "  "})),
        );
        assert_eq!(run.last_message(), "kept");
    }

    #[test]
    fn the_turn_we_are_waiting_on_completes_the_run() {
        let run = run();
        let next = apply_child_event(
            &run,
            "turn/completed",
            &json!({"turn": {"id": "turn-1", "status": "completed"}}),
        );
        assert_eq!(next, Some(AgentRunState::Done));
        // The turn is cleared, so a follow-up can start a new one.
        assert!(run.current_turn_id.lock().unwrap().is_none());
    }

    #[test]
    fn a_turn_belonging_to_something_else_is_ignored() {
        let run = run();
        assert!(apply_child_event(
            &run,
            "turn/completed",
            &json!({"turn": {"id": "some-other-turn"}})
        )
        .is_none());
        assert_eq!(
            run.current_turn_id.lock().unwrap().as_deref(),
            Some("turn-1")
        );
    }

    #[test]
    fn a_failed_turn_carries_its_reason() {
        let run = run();
        let next = apply_child_event(
            &run,
            "turn/completed",
            &json!({"turn": {"id": "turn-1", "error": {"message": "context exceeded"}}}),
        );
        assert_eq!(next, Some(AgentRunState::Failed("context exceeded".into())));
    }

    #[test]
    fn a_stream_error_fails_the_run_with_the_reason_codex_gave() {
        let run = run();
        // The reason is nested under `error`; reading only a top-level
        // `message` replaced every failure with the same placeholder.
        assert_eq!(
            apply_child_event(
                &run,
                "error",
                &json!({"error": {"message": "The 'luna' model is not supported"}})
            ),
            Some(AgentRunState::Failed(
                "The 'luna' model is not supported".into()
            ))
        );
        assert_eq!(
            apply_child_event(&run, "error", &json!({"message": "stream died"})),
            Some(AgentRunState::Failed("stream died".into()))
        );
        // Even with nothing useful in the payload it still fails, rather than
        // leaving the run running forever — and says what it did see.
        assert!(matches!(
            apply_child_event(&run, "error", &json!({"unexpected": 1})),
            Some(AgentRunState::Failed(message)) if message.contains("unexpected")
        ));
    }

    #[test]
    fn reads_model_ids_out_of_a_model_list_response() {
        let response = json!({"data": [{"id": "gpt-5.2"}, {"id": "gpt-5.6-terra"}]});
        assert_eq!(
            collect_model_ids(&response),
            vec!["gpt-5.2", "gpt-5.6-terra"]
        );
        // A shape we do not recognise yields nothing, which is treated as
        // "could not check" rather than "nothing is valid".
        assert!(collect_model_ids(&json!({"other": []})).is_empty());
        assert!(collect_model_ids(&json!({"data": [{"name": "no id"}]})).is_empty());
    }

    #[test]
    fn unrelated_notifications_change_nothing() {
        let run = run();
        assert!(apply_child_event(&run, "turn/started", &json!({})).is_none());
        assert!(apply_child_event(&run, "item/started", &json!({})).is_none());
        assert!(apply_child_event(&run, "item/completed", &json!({})).is_none());
    }

    #[test]
    fn terminal_states_are_recognised_and_map_to_stored_statuses() {
        assert!(!AgentRunState::Starting.is_terminal());
        assert!(!AgentRunState::Running.is_terminal());
        assert!(AgentRunState::Done.is_terminal());
        assert!(AgentRunState::Killed.is_terminal());
        assert!(AgentRunState::Failed("x".into()).is_terminal());

        assert_eq!(AgentRunState::Running.status(), storage::STATUS_RUNNING);
        assert_eq!(AgentRunState::Done.status(), storage::STATUS_DONE);
        assert_eq!(AgentRunState::Killed.status(), storage::STATUS_KILLED);
        assert_eq!(
            AgentRunState::Failed("x".into()).status(),
            storage::STATUS_FAILED
        );
    }

    #[test]
    fn sandbox_names_map_to_protocol_tags() {
        assert_eq!(sandbox_tag("read-only"), "readOnly");
        assert_eq!(sandbox_tag("workspace-write"), "workspaceWrite");
        // Never widens: an unknown name lands on the narrower of the two we use.
        assert_eq!(sandbox_tag("danger-full-access"), "workspaceWrite");
    }

    #[test]
    fn run_ids_are_distinct() {
        // A stub returning one id for every spawn makes the model poll forever,
        // so uniqueness is load-bearing rather than cosmetic.
        let supervisor = AgentSupervisor::default();
        let ids: Vec<String> = (0..5).map(|_| supervisor.next_run_id()).collect();
        let unique: std::collections::HashSet<&String> = ids.iter().collect();
        assert_eq!(unique.len(), ids.len());
    }

    #[test]
    fn a_new_launch_cannot_reuse_the_last_ones_run_ids() {
        // The counter restarts at 1 each launch and the stored row is keyed by
        // run id, so a collision silently rewrites the previous session's row
        // instead of recording the new run.
        let first = AgentSupervisor::default();
        let second = AgentSupervisor {
            launched_at: first.launched_at + 1,
            ..AgentSupervisor::default()
        };
        assert_ne!(first.next_run_id(), second.next_run_id());
    }
}
