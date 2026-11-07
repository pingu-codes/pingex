//! Browser use: agents drive the user's own Chrome.
//!
//! Pingex holds the one DevTools connection to the user's running browser
//! (auto-connect, see `attach.rs`) and hands each thread the tabs it owns.
//! Codex reaches it through dynamic tools in the `pingex_browser` namespace
//! (`tools.rs`): a few cheap, high-level tools first and raw CDP as the
//! fallback. Ownership is a Chrome tab group per thread, kept by the Pingex
//! Browser Helper extension (`helper.rs`); without the helper, ownership is
//! tracked by Pingex alone. What a thread may do is `policy.rs`.

pub(crate) mod approvals;
pub(crate) mod attach;
pub(crate) mod cdp;
pub(crate) mod commands;
mod helper;
mod keys;
#[cfg(test)]
mod live_tests;
pub(crate) mod policy;
mod snapshot;
mod tab;
pub(crate) mod tools;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use approvals::{BrowserApproval, BrowserApprovalKind, BrowserDecision};
use attach::BrowserChannel;
use cdp::{Cdp, CdpEvent, CLOSED_EVENT};
use tab::Tab;

/// How long the user has to click "Allow" in Chrome's connection dialog.
const ALLOW_TIMEOUT: Duration = Duration::from_secs(60);

/// What the UI layer does for the browser. Tauri in the app
/// (`commands::TauriUi`); a fake in tests.
#[async_trait::async_trait]
pub(crate) trait Ui: Send + Sync {
    fn status_changed(&self, status: &BrowserStatus);
    fn tabs_changed(&self, thread_id: &str);
    async fn ask(&self, approval: BrowserApproval) -> BrowserDecision;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ConnectionState {
    /// Not tried yet; the first browser tool call connects.
    Idle,
    /// No browser advertises a DevTools endpoint: Chrome is closed, or
    /// remote debugging is off in chrome://inspect/#remote-debugging.
    NotFound,
    /// Waiting for the user to allow the connection in Chrome's dialog.
    AwaitingApproval,
    Connected,
    /// Was connected; the browser quit or the user revoked access.
    Lost,
    /// The user did not allow the connection, or it failed.
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BrowserStatus {
    pub state: ConnectionState,
    /// Which browser (`Chrome`, `Brave`, ...), once one was found.
    pub browser: Option<String>,
    /// Whether the Pingex Browser Helper extension is attached.
    pub helper: bool,
    pub message: Option<String>,
}

impl BrowserStatus {
    fn idle() -> Self {
        Self {
            state: ConnectionState::Idle,
            browser: None,
            helper: false,
            message: None,
        }
    }
}

/// A tab a thread owns, as the thread and the UI see it.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OwnedTab {
    /// The short id tools use: `t1`, `t2`.
    pub id: String,
    pub target_id: String,
    pub chrome_tab_id: Option<i64>,
    pub title: String,
    pub url: String,
}

#[derive(Debug, Default)]
struct ThreadScope {
    label: Option<String>,
    tabs: Vec<OwnedTab>,
    next_tab: u32,
    sites: BTreeSet<String>,
    full_access: bool,
    /// The thread's latest turn runs with approval policy `never` (the Full
    /// Access preset): every browser question is answered yes without asking.
    /// Nothing is granted on the way, so a later turn back on Auto asks again.
    unattended: bool,
    group_id: Option<i64>,
}

/// One thread's view of the browser, for the Browser panel.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ThreadBrowser {
    pub tabs: Vec<OwnedTab>,
    pub sites: Vec<String>,
    pub full_access: bool,
}

/// The live connection. Replaced wholesale on reconnect; thread scopes
/// outlive it (their tabs are dropped, their grants kept).
pub(crate) struct Conn {
    cdp: Arc<Cdp>,
    browser: String,
    /// The helper extension's service-worker session.
    helper: Mutex<Option<String>>,
    helper_appeared: tokio::sync::Notify,
    /// Attached tabs by target id.
    tabs: Mutex<HashMap<String, Arc<Tab>>>,
    /// Session id → (target id, is the gate session).
    sessions: Mutex<HashMap<String, (String, bool)>>,
}

struct State {
    status: BrowserStatus,
    conn: Option<Arc<Conn>>,
    threads: HashMap<String, ThreadScope>,
    /// Sites allowed for every thread ("Always allow").
    trusted_sites: BTreeSet<String>,
    preferred: Option<BrowserChannel>,
}

pub(crate) struct Inner {
    ui: Arc<dyn Ui>,
    state: Mutex<State>,
    connecting: tokio::sync::Mutex<()>,
    /// Tests point this at a throwaway browser instead of discovering one.
    endpoint_override: Option<String>,
}

/// Cheap to clone; every clone is the same browser.
#[derive(Clone)]
pub(crate) struct Browser(Arc<Inner>);

impl Browser {
    pub(crate) fn new(ui: Arc<dyn Ui>, trusted_sites: BTreeSet<String>) -> Self {
        Self::build(ui, trusted_sites, None)
    }

    #[cfg(test)]
    pub(crate) fn with_endpoint(ui: Arc<dyn Ui>, ws_url: String) -> Self {
        Self::build(ui, BTreeSet::new(), Some(ws_url))
    }

    fn build(
        ui: Arc<dyn Ui>,
        trusted_sites: BTreeSet<String>,
        endpoint_override: Option<String>,
    ) -> Self {
        Self(Arc::new(Inner {
            ui,
            state: Mutex::new(State {
                status: BrowserStatus::idle(),
                conn: None,
                threads: HashMap::new(),
                trusted_sites,
                preferred: None,
            }),
            connecting: tokio::sync::Mutex::new(()),
            endpoint_override,
        }))
    }

    pub(crate) fn status(&self) -> BrowserStatus {
        self.0.state.lock().unwrap().status.clone()
    }

    pub(crate) fn set_preferred(&self, channel: Option<BrowserChannel>) {
        self.0.state.lock().unwrap().preferred = channel;
    }

    fn set_status(&self, status: BrowserStatus) {
        self.0.state.lock().unwrap().status = status.clone();
        self.0.ui.status_changed(&status);
    }

    fn update_status(&self, change: impl FnOnce(&mut BrowserStatus)) {
        let status = {
            let mut state = self.0.state.lock().unwrap();
            change(&mut state.status);
            state.status.clone()
        };
        self.0.ui.status_changed(&status);
    }

    fn live_conn(&self) -> Option<Arc<Conn>> {
        let state = self.0.state.lock().unwrap();
        state.conn.clone().filter(|conn| !conn.cdp.is_closed())
    }

    /// The live connection, connecting first if there is none.
    pub(crate) async fn connection(&self) -> Result<Arc<Conn>, String> {
        if let Some(conn) = self.live_conn() {
            return Ok(conn);
        }
        let _guard = self.0.connecting.lock().await;
        if let Some(conn) = self.live_conn() {
            return Ok(conn);
        }
        self.connect().await
    }

    async fn connect(&self) -> Result<Arc<Conn>, String> {
        let (url, browser) = match &self.0.endpoint_override {
            Some(url) => (url.clone(), "Chromium".to_string()),
            None => {
                let preferred = self.0.state.lock().unwrap().preferred;
                let endpoints = attach::discover(preferred);
                let Some(endpoint) = endpoints.into_iter().next() else {
                    let message = "No browser with remote debugging is running. Open Chrome (144 or newer), go to chrome://inspect/#remote-debugging and turn on \"Allow remote debugging for this browser instance\".".to_string();
                    self.set_status(BrowserStatus {
                        state: ConnectionState::NotFound,
                        browser: None,
                        helper: false,
                        message: Some(message.clone()),
                    });
                    return Err(message);
                };
                (endpoint.ws_url(), endpoint.channel.label().to_string())
            }
        };
        self.set_status(BrowserStatus {
            state: ConnectionState::AwaitingApproval,
            browser: Some(browser.clone()),
            helper: false,
            message: Some(format!("Allow the connection in {browser}'s dialog.")),
        });
        let cdp = match tokio::time::timeout(ALLOW_TIMEOUT, Cdp::connect(&url)).await {
            Ok(Ok(cdp)) => cdp,
            Ok(Err(error)) => {
                let refused = error.contains("refused") || error.contains("Connection reset");
                let message = if refused {
                    format!("{browser} is not running, or its remote debugging was switched off. Open it and turn on chrome://inspect/#remote-debugging.")
                } else {
                    format!("Could not connect to {browser}: {error}")
                };
                self.set_status(BrowserStatus {
                    state: if refused {
                        ConnectionState::NotFound
                    } else {
                        ConnectionState::Failed
                    },
                    browser: Some(browser),
                    helper: false,
                    message: Some(message.clone()),
                });
                return Err(message);
            }
            Err(_) => {
                let message = format!("{browser} did not allow the connection within {}s. Click \"Allow\" in its dialog, then try again.", ALLOW_TIMEOUT.as_secs());
                self.set_status(BrowserStatus {
                    state: ConnectionState::Failed,
                    browser: Some(browser),
                    helper: false,
                    message: Some(message.clone()),
                });
                return Err(message);
            }
        };
        let conn = Arc::new(Conn {
            cdp: cdp.clone(),
            browser: browser.clone(),
            helper: Mutex::new(None),
            helper_appeared: tokio::sync::Notify::new(),
            tabs: Mutex::new(HashMap::new()),
            sessions: Mutex::new(HashMap::new()),
        });
        {
            let mut state = self.0.state.lock().unwrap();
            state.conn = Some(conn.clone());
            // Tabs from an earlier connection are gone with it.
            for scope in state.threads.values_mut() {
                scope.tabs.clear();
            }
        }
        self.spawn_dispatcher(conn.clone());
        // Existing targets arrive as `targetCreated`, which is how the helper
        // is found; later ones (a helper worker waking up) the same way.
        cdp.call(None, "Target.setDiscoverTargets", json!({"discover": true}))
            .await
            .map_err(|error| error.to_string())?;
        self.set_status(BrowserStatus {
            state: ConnectionState::Connected,
            browser: Some(browser),
            helper: false,
            message: None,
        });
        Ok(conn)
    }

    fn spawn_dispatcher(&self, conn: Arc<Conn>) {
        let mut events = conn.cdp.subscribe();
        let browser = self.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                match events.recv().await {
                    Ok(event) if event.method == CLOSED_EVENT => break,
                    Ok(event) => browser.on_event(&conn, event),
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }
            browser.on_closed(&conn);
        });
    }

    fn on_closed(&self, conn: &Arc<Conn>) {
        let threads: Vec<String> = {
            let mut state = self.0.state.lock().unwrap();
            if !state
                .conn
                .as_ref()
                .is_some_and(|live| Arc::ptr_eq(live, conn))
            {
                return;
            }
            state.conn = None;
            let mut changed = Vec::new();
            for (id, scope) in state.threads.iter_mut() {
                if !scope.tabs.is_empty() {
                    scope.tabs.clear();
                    changed.push(id.clone());
                }
            }
            changed
        };
        self.set_status(BrowserStatus {
            state: ConnectionState::Lost,
            browser: Some(conn.browser.clone()),
            helper: false,
            message: Some(format!("The connection to {} closed.", conn.browser)),
        });
        for thread in threads {
            self.0.ui.tabs_changed(&thread);
        }
    }

    fn on_event(&self, conn: &Arc<Conn>, event: CdpEvent) {
        match event.method.as_str() {
            "Target.targetCreated" | "Target.targetInfoChanged" => {
                let info = &event.params["targetInfo"];
                if helper::is_helper_target(info) && conn.helper.lock().unwrap().is_none() {
                    let browser = self.clone();
                    let conn = conn.clone();
                    let target_id = info["targetId"].as_str().unwrap_or_default().to_string();
                    tauri::async_runtime::spawn(async move {
                        browser.attach_helper(&conn, &target_id).await
                    });
                }
                if event.method == "Target.targetInfoChanged" {
                    self.on_target_changed(info);
                }
                return;
            }
            "Target.targetDestroyed" => {
                let target_id = event.params["targetId"].as_str().unwrap_or_default();
                self.forget_target(conn, target_id);
                return;
            }
            "Target.detachedFromTarget" => {
                let session = event.params["sessionId"].as_str().unwrap_or_default();
                let mut helper = conn.helper.lock().unwrap();
                if helper.as_deref() == Some(session) {
                    *helper = None;
                    drop(helper);
                    self.update_status(|status| status.helper = false);
                }
                return;
            }
            _ => {}
        }
        let Some(session) = event.session_id.as_deref() else {
            return;
        };
        if conn.helper.lock().unwrap().as_deref() == Some(session) {
            if event.method == "Runtime.bindingCalled" {
                self.on_helper_notice(conn, &event.params);
            }
            return;
        }
        let Some((target_id, gate)) = conn.sessions.lock().unwrap().get(session).cloned() else {
            return;
        };
        let Some(tab) = conn.tabs.lock().unwrap().get(&target_id).cloned() else {
            return;
        };
        if gate {
            if event.method == "Fetch.requestPaused" {
                let browser = self.clone();
                let conn = conn.clone();
                tauri::async_runtime::spawn(async move {
                    browser.gate_navigation(&conn, &tab, event.params).await
                });
            }
            return;
        }
        if event.method == "Target.attachedToTarget" {
            // A child target (iframe, worker) the agent auto-attached through
            // raw CDP: its session belongs to the same tab.
            if let Some(child) = event.params["sessionId"].as_str() {
                conn.sessions
                    .lock()
                    .unwrap()
                    .insert(child.to_string(), (target_id.clone(), false));
            }
        }
        tab.record(&event);
    }

    fn on_target_changed(&self, info: &Value) {
        let Some(target_id) = info["targetId"].as_str() else {
            return;
        };
        let changed: Vec<String> = {
            let mut state = self.0.state.lock().unwrap();
            let mut changed = Vec::new();
            for (thread, scope) in state.threads.iter_mut() {
                for tab in scope
                    .tabs
                    .iter_mut()
                    .filter(|tab| tab.target_id == target_id)
                {
                    tab.title = info["title"].as_str().unwrap_or_default().to_string();
                    tab.url = info["url"].as_str().unwrap_or_default().to_string();
                    changed.push(thread.clone());
                }
            }
            changed
        };
        for thread in changed {
            self.0.ui.tabs_changed(&thread);
        }
    }

    /// A tab closed (or is no longer the agent's): drop it everywhere.
    fn forget_target(&self, conn: &Conn, target_id: &str) {
        conn.tabs.lock().unwrap().remove(target_id);
        conn.sessions
            .lock()
            .unwrap()
            .retain(|_, (target, _)| target != target_id);
        let changed: Vec<String> = {
            let mut state = self.0.state.lock().unwrap();
            state
                .threads
                .iter_mut()
                .filter_map(|(thread, scope)| {
                    let before = scope.tabs.len();
                    scope.tabs.retain(|tab| tab.target_id != target_id);
                    (scope.tabs.len() != before).then(|| thread.clone())
                })
                .collect()
        };
        for thread in changed {
            self.0.ui.tabs_changed(&thread);
        }
    }

    // ---- thread scopes -------------------------------------------------

    fn with_scope<R>(&self, thread_id: &str, f: impl FnOnce(&mut ThreadScope) -> R) -> R {
        let mut state = self.0.state.lock().unwrap();
        f(state.threads.entry(thread_id.to_string()).or_default())
    }

    pub(crate) fn thread_view(&self, thread_id: &str) -> ThreadBrowser {
        let state = self.0.state.lock().unwrap();
        match state.threads.get(thread_id) {
            Some(scope) => ThreadBrowser {
                tabs: scope.tabs.clone(),
                sites: scope.sites.iter().cloned().collect(),
                full_access: scope.full_access,
            },
            None => ThreadBrowser {
                tabs: Vec::new(),
                sites: Vec::new(),
                full_access: false,
            },
        }
    }

    pub(crate) fn trusted_sites(&self) -> Vec<String> {
        self.0
            .state
            .lock()
            .unwrap()
            .trusted_sites
            .iter()
            .cloned()
            .collect()
    }

    pub(crate) fn set_label(&self, thread_id: &str, label: &str) {
        let label = label.trim();
        let group = self.with_scope(thread_id, |scope| {
            scope.label = (!label.is_empty()).then(|| label.to_string());
            scope.group_id
        });
        if let (Some(group), Some(conn)) = (group, self.live_conn()) {
            let (title, color) = self.group_style(thread_id);
            tauri::async_runtime::spawn(async move {
                let _ = helper::update_group(&conn, group, &title, color).await;
            });
        }
    }

    fn group_style(&self, thread_id: &str) -> (String, &'static str) {
        let label = self.with_scope(thread_id, |scope| scope.label.clone());
        let title = match label {
            Some(label) => format!("Pingex · {}", truncate_chars(&label, 24)),
            None => "Pingex".to_string(),
        };
        (title, helper::color_for(thread_id))
    }

    pub(crate) fn site_allowed(&self, thread_id: &str, site: &str) -> bool {
        let state = self.0.state.lock().unwrap();
        let thread = state.threads.get(thread_id);
        thread.is_some_and(|scope| scope.full_access || scope.unattended)
            || state
                .trusted_sites
                .iter()
                .chain(thread.into_iter().flat_map(|scope| scope.sites.iter()))
                .any(|granted| policy::grant_covers(granted, site))
    }

    fn grant_site(&self, thread_id: &str, site: &str, always: bool) {
        {
            let mut state = self.0.state.lock().unwrap();
            if always {
                state.trusted_sites.insert(site.to_string());
            }
            state
                .threads
                .entry(thread_id.to_string())
                .or_default()
                .sites
                .insert(site.to_string());
        }
        if always {
            self.0.ui.status_changed(&self.status());
        }
        self.0.ui.tabs_changed(thread_id);
    }

    pub(crate) fn revoke_site(&self, thread_id: Option<&str>, site: &str) {
        let mut state = self.0.state.lock().unwrap();
        match thread_id {
            Some(thread) => {
                if let Some(scope) = state.threads.get_mut(thread) {
                    scope.sites.remove(site);
                }
            }
            None => {
                state.trusted_sites.remove(site);
            }
        }
    }

    pub(crate) fn set_full_access(&self, thread_id: &str, on: bool) {
        self.with_scope(thread_id, |scope| scope.full_access = on);
        self.0.ui.tabs_changed(thread_id);
    }

    fn full_access(&self, thread_id: &str) -> bool {
        self.with_scope(thread_id, |scope| scope.full_access || scope.unattended)
    }

    /// Follow the thread's approval policy: `never` stops browser questions.
    pub(crate) fn set_unattended(&self, thread_id: &str, on: bool) {
        self.with_scope(thread_id, |scope| scope.unattended = on);
    }

    /// Ask for (or recall) permission to visit `url` in this thread.
    pub(crate) async fn ensure_site(
        &self,
        thread_id: &str,
        url: &str,
        detail: &str,
    ) -> Result<(), String> {
        let Some(site) = policy::site_of(url) else {
            return Ok(());
        };
        if self.site_allowed(thread_id, &site) {
            return Ok(());
        }
        let decision = self
            .ask(BrowserApproval {
                id: String::new(),
                thread_id: thread_id.to_string(),
                kind: BrowserApprovalKind::Site,
                site: Some(site.clone()),
                url: Some(url.to_string()),
                tab_title: None,
                detail: detail.to_string(),
            })
            .await;
        match decision {
            BrowserDecision::Deny => Err(format!("The user did not allow visiting {site}.")),
            BrowserDecision::Allow | BrowserDecision::AlwaysAllow => {
                self.grant_site(thread_id, &site, decision == BrowserDecision::AlwaysAllow);
                Ok(())
            }
        }
    }

    /// Ask for Full browser access, unless the thread already has it.
    pub(crate) async fn ensure_full_access(
        &self,
        thread_id: &str,
        why: &str,
    ) -> Result<(), String> {
        if self.full_access(thread_id) {
            return Ok(());
        }
        let decision = self
            .ask(BrowserApproval {
                id: String::new(),
                thread_id: thread_id.to_string(),
                kind: BrowserApprovalKind::FullAccess,
                site: None,
                url: None,
                tab_title: None,
                detail: why.to_string(),
            })
            .await;
        if decision == BrowserDecision::Deny {
            return Err(format!(
                "The user did not grant full browser access ({why})."
            ));
        }
        self.set_full_access(thread_id, true);
        Ok(())
    }

    async fn ask(&self, approval: BrowserApproval) -> BrowserDecision {
        if self.with_scope(&approval.thread_id, |scope| scope.unattended) {
            return BrowserDecision::Allow;
        }
        self.0.ui.ask(approval).await
    }

    /// The navigation gate: every main-frame document request in an owned tab
    /// pauses here, so a navigation started by page JavaScript cannot leave
    /// the thread's granted sites either.
    async fn gate_navigation(&self, conn: &Conn, tab: &Tab, params: Value) {
        let request_id = params["requestId"].as_str().unwrap_or_default().to_string();
        let url = params["request"]["url"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let main_frame = params["frameId"].as_str() == Some(tab.target_id.as_str());
        let allowed = !main_frame
            || match policy::site_of(&url) {
                None => true,
                Some(site) => {
                    self.site_allowed(&tab.thread_id, &site)
                        || self
                            .ensure_site(
                                &tab.thread_id,
                                &url,
                                "A tab this thread owns is navigating here.",
                            )
                            .await
                            .is_ok()
                }
            };
        let (method, params) = if allowed {
            ("Fetch.continueRequest", json!({"requestId": request_id}))
        } else {
            (
                "Fetch.failRequest",
                // `Aborted` cancels the navigation and leaves the current page in
                // place; `BlockedByClient` would replace it with an error page.
                json!({"requestId": request_id, "errorReason": "Aborted"}),
            )
        };
        let _ = conn.cdp.call(Some(&tab.gate), method, params).await;
    }

    // ---- ownership -----------------------------------------------------

    /// Attach to `target_id` and make it one of the thread's tabs.
    async fn own(
        &self,
        conn: &Arc<Conn>,
        thread_id: &str,
        target_id: &str,
    ) -> Result<OwnedTab, String> {
        if let Some(existing) = self.owned_by(thread_id, target_id) {
            return Ok(existing);
        }
        if let Some(other) = self.owner_of(target_id) {
            return Err(format!(
                "That tab already belongs to another Pingex thread ({other})."
            ));
        }
        let tab = Tab::attach(conn, thread_id, target_id).await?;
        let info = conn
            .cdp
            .call(None, "Target.getTargetInfo", json!({"targetId": target_id}))
            .await
            .map(|result| result["targetInfo"].clone())
            .unwrap_or(Value::Null);
        let owned = self.with_scope(thread_id, |scope| {
            scope.next_tab += 1;
            let owned = OwnedTab {
                id: format!("t{}", scope.next_tab),
                target_id: target_id.to_string(),
                chrome_tab_id: None,
                title: info["title"].as_str().unwrap_or_default().to_string(),
                url: info["url"].as_str().unwrap_or_default().to_string(),
            };
            scope.tabs.push(owned.clone());
            owned
        });
        conn.tabs.lock().unwrap().insert(target_id.to_string(), tab);
        // Put it in the thread's group so the user can see it is the agent's.
        if let Some(tab_id) = self.group_into_thread(conn, thread_id, target_id).await {
            self.with_scope(thread_id, |scope| {
                if let Some(tab) = scope.tabs.iter_mut().find(|tab| tab.target_id == target_id) {
                    tab.chrome_tab_id = Some(tab_id);
                }
            });
        }
        self.0.ui.tabs_changed(thread_id);
        Ok(self.owned_by(thread_id, target_id).unwrap_or(owned))
    }

    fn owned_by(&self, thread_id: &str, target_id: &str) -> Option<OwnedTab> {
        let state = self.0.state.lock().unwrap();
        state
            .threads
            .get(thread_id)?
            .tabs
            .iter()
            .find(|tab| tab.target_id == target_id)
            .cloned()
    }

    fn owner_of(&self, target_id: &str) -> Option<String> {
        let state = self.0.state.lock().unwrap();
        state
            .threads
            .iter()
            .find(|(_, scope)| scope.tabs.iter().any(|tab| tab.target_id == target_id))
            .map(|(thread, _)| thread.clone())
    }

    /// The thread's tab with short id `id` (`t1`), attached.
    pub(crate) fn tab(
        &self,
        conn: &Conn,
        thread_id: &str,
        id: &str,
    ) -> Result<(OwnedTab, Arc<Tab>), String> {
        let id = id.trim();
        let owned = {
            let state = self.0.state.lock().unwrap();
            state
                .threads
                .get(thread_id)
                .and_then(|scope| scope.tabs.iter().find(|tab| tab.id == id).cloned())
        };
        let Some(owned) = owned else {
            return Err(format!(
                "No tab {id} in this thread. Call `tabs` to list the thread's tabs, or `open` one."
            ));
        };
        let tab = conn.tabs.lock().unwrap().get(&owned.target_id).cloned();
        match tab {
            Some(tab) => Ok((owned, tab)),
            None => Err(format!(
                "Tab {id} is no longer attached; it may have been closed."
            )),
        }
    }

    pub(crate) fn owned_tabs(&self, thread_id: &str) -> Vec<OwnedTab> {
        self.thread_view(thread_id).tabs
    }

    /// Stop owning a tab: leave it open, out of the group, undriven.
    pub(crate) async fn release(&self, conn: &Conn, thread_id: &str, target_id: &str) {
        let removed = self.with_scope(thread_id, |scope| {
            let before = scope.tabs.len();
            scope.tabs.retain(|tab| tab.target_id != target_id);
            before != scope.tabs.len()
        });
        let tab = conn.tabs.lock().unwrap().remove(target_id);
        if let Some(tab) = tab {
            conn.sessions
                .lock()
                .unwrap()
                .retain(|_, (target, _)| target != target_id);
            tab.detach(conn).await;
        }
        if removed {
            if let Some(tab_id) = helper::tab_id_for(conn, target_id).await {
                let _ = helper::ungroup(conn, tab_id).await;
            }
            self.0.ui.tabs_changed(thread_id);
        }
    }

    async fn group_into_thread(
        &self,
        conn: &Conn,
        thread_id: &str,
        target_id: &str,
    ) -> Option<i64> {
        // A freshly created tab wakes a sleeping helper; give it a moment.
        helper::wait_for(conn, Duration::from_millis(1500)).await?;
        let tab_id = helper::tab_id_for(conn, target_id).await?;
        let (title, color) = self.group_style(thread_id);
        let group = self.with_scope(thread_id, |scope| scope.group_id);
        let group = helper::add_to_group(conn, tab_id, group, &title, color)
            .await
            .ok()?;
        self.with_scope(thread_id, |scope| scope.group_id = Some(group));
        Some(tab_id)
    }

    async fn attach_helper(&self, conn: &Arc<Conn>, target_id: &str) {
        let Ok(session) = helper::attach(conn, target_id).await else {
            return;
        };
        *conn.helper.lock().unwrap() = Some(session);
        conn.helper_appeared.notify_waiters();
        self.update_status(|status| status.helper = true);
    }

    /// The user dragged a tab into or out of a thread's group, or closed it.
    fn on_helper_notice(&self, conn: &Arc<Conn>, params: &Value) {
        if params["name"].as_str() != Some(helper::BINDING) {
            return;
        }
        let Ok(notice) = serde_json::from_str::<Value>(params["payload"].as_str().unwrap_or(""))
        else {
            return;
        };
        if notice["type"].as_str() != Some("tabGroupChanged") {
            if notice["type"].as_str() == Some("groupRemoved") {
                let group = notice["groupId"].as_i64();
                let mut state = self.0.state.lock().unwrap();
                for scope in state.threads.values_mut() {
                    if scope.group_id.is_some() && scope.group_id == group {
                        scope.group_id = None;
                    }
                }
            }
            return;
        }
        let (Some(tab_id), Some(group_id)) = (notice["tabId"].as_i64(), notice["groupId"].as_i64())
        else {
            return;
        };
        // Read ownership now, before a `groupRemoved` that follows this
        // notice clears the group it was in.
        let (owner, joined) = {
            let state = self.0.state.lock().unwrap();
            let owner = state.threads.iter().find_map(|(thread, scope)| {
                let tab = scope
                    .tabs
                    .iter()
                    .find(|tab| tab.chrome_tab_id == Some(tab_id))?;
                Some((thread.clone(), tab.target_id.clone(), scope.group_id))
            });
            let joined = state
                .threads
                .iter()
                .find(|(_, scope)| scope.group_id == Some(group_id))
                .map(|(thread, _)| thread.clone());
            (owner, joined)
        };
        let browser = self.clone();
        let conn = conn.clone();
        tauri::async_runtime::spawn(async move {
            match owner {
                // Still in its own group (Pingex just grouped it): nothing to do.
                Some((_, _, Some(group))) if group == group_id => {}
                // Dragged out of its thread's group: no longer the agent's;
                // perhaps straight into another thread's.
                Some((thread, target_id, _)) => {
                    browser.release(&conn, &thread, &target_id).await;
                    if let Some(other) = joined.filter(|other| *other != thread) {
                        let _ = browser.share(&conn, &other, &target_id).await;
                    }
                }
                // Dragged into a thread's group: the user shares it.
                None => {
                    if let (Some(thread), Some(target_id)) =
                        (joined, helper::target_for(&conn, tab_id).await)
                    {
                        let _ = browser.share(&conn, &thread, &target_id).await;
                    }
                }
            }
        });
    }

    /// The user handed a tab to a thread (dragged it into the group): own it
    /// and grant its site, since the user chose to share it.
    async fn share(
        &self,
        conn: &Arc<Conn>,
        thread_id: &str,
        target_id: &str,
    ) -> Result<OwnedTab, String> {
        let owned = self.own(conn, thread_id, target_id).await?;
        if let Some(site) = policy::site_of(&owned.url) {
            self.grant_site(thread_id, &site, false);
        }
        Ok(owned)
    }
}

fn truncate_chars(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let mut cut: String = text.chars().take(limit - 1).collect();
    cut.push('…');
    cut
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct CountingUi {
        asked: Mutex<usize>,
    }

    #[async_trait::async_trait]
    impl Ui for CountingUi {
        fn status_changed(&self, _: &BrowserStatus) {}
        fn tabs_changed(&self, _: &str) {}
        async fn ask(&self, _: BrowserApproval) -> BrowserDecision {
            *self.asked.lock().unwrap() += 1;
            BrowserDecision::Deny
        }
    }

    #[tokio::test]
    async fn full_access_turns_ask_nothing_and_grant_nothing() {
        let ui = Arc::new(CountingUi::default());
        let browser = Browser::new(ui.clone(), BTreeSet::new());
        browser.set_unattended("t", true);
        assert!(browser
            .ensure_site("t", "https://bank.test/", "go")
            .await
            .is_ok());
        assert!(browser.ensure_full_access("t", "cookies").await.is_ok());
        assert_eq!(*ui.asked.lock().unwrap(), 0);

        // Back on a prompting policy: the earlier answers left no grants.
        browser.set_unattended("t", false);
        let view = browser.thread_view("t");
        assert!(view.sites.is_empty() && !view.full_access);
        assert!(browser
            .ensure_site("t", "https://bank.test/", "go")
            .await
            .is_err());
        assert_eq!(*ui.asked.lock().unwrap(), 1);
    }
}
