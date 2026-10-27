//! The seam every non-Codex harness sits behind (`features/13-harnesses.md`,
//! "Driver"). Codex keeps its own path through `HomeContext::session`; a
//! command that finds a `harness_threads` row asks [`Drivers`] for that
//! thread's driver instead of assuming which harness it is.

use async_trait::async_trait;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use tauri::AppHandle;

use super::HarnessKind;
use crate::acp::driver::AcpDriver;
use crate::acp::spec::AcpRuntime;
use crate::claude::driver::{ClaudeDriver, ClaudeRuntime};
use crate::codex::requests::TurnOptions;
use crate::codex::wire::WireLog;
use crate::storage::HarnessThread;

/// Request ids handed to the frontend for driver prompts start here, so they
/// never collide with the app-server's JSON-RPC ids.
pub(crate) const REQUEST_ID_BASE: i64 = 1 << 40;

/// One id sequence for every driver in a context: a request id names exactly
/// one prompt, whichever driver raised it.
#[derive(Clone)]
pub(crate) struct RequestIds(Arc<AtomicI64>);

impl Default for RequestIds {
    fn default() -> Self {
        Self(Arc::new(AtomicI64::new(REQUEST_ID_BASE)))
    }
}

impl RequestIds {
    pub(crate) fn next(&self) -> i64 {
        self.0.fetch_add(1, Ordering::SeqCst)
    }
}

/// How the user answered a driver's request.
pub(crate) enum RequestAnswer {
    /// A permission option id, or a Codex decision word (`accept`,
    /// `acceptForSession`, `decline`) the driver maps onto its options.
    Option(String),
    /// Codex-shaped answers to a question form.
    UserInput(Value),
}

/// The error for a verb a harness does not have.
pub(crate) fn unsupported(kind: HarnessKind, what: &str) -> String {
    format!("{} threads do not support {what}", kind.label())
}

#[async_trait]
pub(crate) trait Driver: Send + Sync {
    fn kind(&self) -> HarnessKind;

    /// Send a prompt, spawning the thread's process if it has none.
    /// `has_history`: the journal holds turns for this thread, so its native
    /// session must exist and be resumed. Returns the new turn's id.
    async fn start_turn(
        &self,
        app: &AppHandle,
        thread: &HarnessThread,
        has_history: bool,
        input: &[Value],
        options: Option<&TurnOptions>,
    ) -> Result<String, String>;

    async fn interrupt(&self, thread_id: &str) -> Result<(), String>;

    /// Whether a request id belongs to one of this driver's open prompts.
    fn owns_request(&self, request_id: i64) -> bool;

    async fn respond(&self, request_id: i64, answer: RequestAnswer) -> Result<(), String>;

    /// The composer's model list, in Codex's `model/list` shape.
    async fn models(&self) -> Result<Value, String>;

    /// Threads with a turn in flight on a live process.
    fn active_threads(&self) -> Vec<String>;

    /// Whether any process is live. Editing or removing a Home tears its
    /// context down, so the caller refuses rather than killing a turn.
    fn is_busy(&self) -> bool;

    /// Drop the process for a thread that was deleted or archived.
    fn close_thread(&self, thread_id: &str);

    fn kill_all(&self);

    /// Write out anything buffered for the database (permission modes), so a
    /// read that follows sees it.
    async fn flush(&self) {}

    /// Summarise the thread's context as a turn. Returns the turn id.
    async fn compact(
        &self,
        _app: &AppHandle,
        _thread: &HarnessThread,
        _has_history: bool,
    ) -> Result<String, String> {
        Err(unsupported(self.kind(), "compaction"))
    }

    /// Carry a sidebar rename over to the native session, where it has one.
    async fn rename(&self, _thread_id: &str, _title: &str) {}

    /// What a live process reports its context holds. `None` when it cannot
    /// say.
    async fn context_usage(&self, _thread_id: &str) -> Result<Option<Value>, String> {
        Ok(None)
    }

    /// Remove the native session behind a deleted thread, where the harness
    /// keeps one of its own.
    async fn forget_thread(&self, _thread: &HarnessThread) {}
}

/// Where each driver of a context finds its binary and config.
pub(crate) struct DriverRuntimes {
    pub(crate) claude: ClaudeRuntime,
    /// Per ACP harness. One left out runs its default binary on the Claude
    /// runtime's host.
    pub(crate) acp: BTreeMap<HarnessKind, AcpRuntime>,
}

impl DriverRuntimes {
    pub(crate) fn with_claude(claude: ClaudeRuntime) -> Self {
        Self {
            claude,
            acp: BTreeMap::new(),
        }
    }
}

/// The non-Codex drivers of one context. Every context carries all of them;
/// none spawns anything until a thread of its kind runs a turn.
pub(crate) struct Drivers {
    claude: Arc<ClaudeDriver>,
    acp: Vec<Arc<AcpDriver>>,
}

impl Drivers {
    pub(crate) fn new(home_key: &str, mut runtimes: DriverRuntimes, wire: Arc<WireLog>) -> Self {
        let ids = RequestIds::default();
        let acp = HarnessKind::ALL
            .into_iter()
            .filter_map(|kind| {
                let spec = kind.acp_spec()?;
                let runtime = runtimes.acp.remove(&kind).unwrap_or_else(|| {
                    AcpRuntime::resolve(spec, None, runtimes.claude.host.clone())
                });
                Some(Arc::new(AcpDriver::new(
                    spec,
                    home_key.to_string(),
                    runtime,
                    wire.clone(),
                    ids.clone(),
                )))
            })
            .collect();
        Self {
            claude: Arc::new(ClaudeDriver::new(
                home_key.to_string(),
                runtimes.claude,
                wire,
                ids,
            )),
            acp,
        }
    }

    /// The Claude driver, for what only Claude has: side questions and its
    /// runtime settings.
    pub(crate) fn claude(&self) -> &ClaudeDriver {
        &self.claude
    }

    pub(crate) fn all(&self) -> Vec<Arc<dyn Driver>> {
        let mut all: Vec<Arc<dyn Driver>> = vec![self.claude.clone()];
        all.extend(
            self.acp
                .iter()
                .map(|driver| driver.clone() as Arc<dyn Driver>),
        );
        all
    }

    /// The driver for `kind`; `None` for Codex, which is not behind this seam.
    pub(crate) fn get(&self, kind: HarnessKind) -> Option<Arc<dyn Driver>> {
        self.all().into_iter().find(|driver| driver.kind() == kind)
    }

    pub(crate) fn owner_of(&self, request_id: i64) -> Option<Arc<dyn Driver>> {
        if request_id < REQUEST_ID_BASE {
            return None;
        }
        self.all()
            .into_iter()
            .find(|driver| driver.owns_request(request_id))
    }

    pub(crate) fn active_threads(&self) -> Vec<String> {
        self.all()
            .iter()
            .flat_map(|driver| driver.active_threads())
            .collect()
    }

    pub(crate) fn is_busy(&self) -> bool {
        self.all().iter().any(|driver| driver.is_busy())
    }

    pub(crate) fn kill_all(&self) {
        for driver in self.all() {
            driver.kill_all();
        }
    }

    pub(crate) async fn flush(&self) {
        for driver in self.all() {
            driver.flush().await;
        }
    }
}
