//! The Tauri side of browser use: the UI bridge, the events, the commands
//! the Browser panel calls, and running Codex's `item/tool/call` requests.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager, State};
use tauri_specta::Event;

use super::approvals::{Approvals, BrowserApproval, BrowserDecision};
use super::attach::BrowserChannel;
use super::tools::{ToolOutput, NAMESPACE};
use super::{helper, Browser, BrowserStatus, ThreadBrowser, Ui};
use crate::codex::child::CodexChild;
use crate::settings::prefs;

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BrowserStatusChanged {
    pub status: BrowserStatus,
}

/// A thread's tabs or grants changed; the panel re-reads `browser_thread`.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BrowserTabsChanged {
    pub thread_id: String,
}

pub(crate) struct TauriUi {
    app: AppHandle,
    approvals: Approvals,
}

#[async_trait::async_trait]
impl Ui for TauriUi {
    fn status_changed(&self, status: &BrowserStatus) {
        let _ = BrowserStatusChanged {
            status: status.clone(),
        }
        .emit(&self.app);
    }

    fn tabs_changed(&self, thread_id: &str) {
        let _ = BrowserTabsChanged {
            thread_id: thread_id.to_string(),
        }
        .emit(&self.app);
    }

    async fn ask(&self, approval: BrowserApproval) -> BrowserDecision {
        let site = approval.site.clone();
        let decision = self.approvals.ask(&self.app, approval).await;
        if decision == BrowserDecision::AlwaysAllow {
            if let Some(site) = site {
                let _ = prefs::update_overrides(&prefs::settings_path(), |overrides| {
                    if !overrides.browser_trusted_sites.contains(&site) {
                        overrides.browser_trusted_sites.push(site);
                    }
                    Ok(())
                });
            }
        }
        decision
    }
}

/// The browser plus the bookkeeping only the app needs: the UI bridge and
/// the tool calls in flight, so an interrupted turn can stop them.
pub(crate) struct BrowserState {
    pub browser: Browser,
    ui: Arc<TauriUi>,
    calls: Mutex<HashMap<i64, Call>>,
}

struct Call {
    thread_id: String,
    turn_id: String,
    child: Arc<CodexChild>,
    task: tauri::async_runtime::JoinHandle<()>,
}

impl BrowserState {
    pub(crate) fn new(app: AppHandle) -> Self {
        let overrides = prefs::read_overrides(&prefs::settings_path());
        let ui = Arc::new(TauriUi {
            app,
            approvals: Approvals::default(),
        });
        let browser = Browser::new(
            ui.clone(),
            overrides.browser_trusted_sites.into_iter().collect(),
        );
        browser.set_preferred(overrides.browser_channel);
        Self {
            browser,
            ui,
            calls: Mutex::default(),
        }
    }

    fn forget_call(&self, request_id: i64) {
        self.calls.lock().unwrap().remove(&request_id);
    }

    /// Codex dropped a request (the turn was interrupted or moved on).
    pub(crate) fn cancel_request(&self, request_id: i64) {
        if let Some(call) = self.calls.lock().unwrap().remove(&request_id) {
            call.task.abort();
        }
    }

    /// Codex finished or interrupted the turn: its calls no longer matter.
    pub(crate) fn cancel_turn(&self, thread_id: &str, turn_id: &str) {
        self.calls.lock().unwrap().retain(|_, call| {
            let matches = call.thread_id == thread_id && call.turn_id == turn_id;
            if matches {
                call.task.abort();
            }
            !matches
        });
    }

    /// The user pressed Stop: end the thread's calls and tell Codex, which is
    /// still waiting on them.
    fn stop_thread(&self, thread_id: &str) {
        let stopped: Vec<(i64, Call)> = {
            let mut calls = self.calls.lock().unwrap();
            let ids: Vec<i64> = calls
                .iter()
                .filter(|(_, call)| call.thread_id == thread_id)
                .map(|(id, _)| *id)
                .collect();
            ids.into_iter()
                .filter_map(|id| calls.remove(&id).map(|call| (id, call)))
                .collect()
        };
        for (request_id, call) in stopped {
            call.task.abort();
            let output = ToolOutput::fail("The user stopped this browser action.");
            let _ = call.child.respond(request_id, output.to_response());
        }
    }
}

/// `item/tool/call` params (v2 `DynamicToolCallParams`).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ToolCall {
    pub thread_id: String,
    pub turn_id: String,
    #[allow(dead_code)]
    pub call_id: String,
    pub namespace: Option<String>,
    pub tool: String,
    #[serde(default)]
    pub arguments: Value,
}

/// Answer an `item/tool/call` from Codex. Ours run on a task (they can wait
/// on the user), anything else is refused at once so its turn moves on.
pub(crate) fn handle_tool_call(
    app: &AppHandle,
    child: &Arc<CodexChild>,
    request_id: i64,
    params: &Value,
) {
    let call = match serde_json::from_value::<ToolCall>(params.clone()) {
        Ok(call) if call.namespace.as_deref() == Some(NAMESPACE) => call,
        Ok(call) => {
            let output = ToolOutput::fail(format!(
                "Pingex has no dynamic tool {}{}.",
                call.namespace
                    .map(|namespace| format!("{namespace}."))
                    .unwrap_or_default(),
                call.tool
            ));
            let _ = child.respond(request_id, output.to_response());
            return;
        }
        Err(error) => {
            let _ = child.respond(
                request_id,
                ToolOutput::fail(format!("Malformed tool call: {error}")).to_response(),
            );
            return;
        }
    };
    let Some(state) = app.try_state::<BrowserState>() else {
        let _ = child.respond(
            request_id,
            ToolOutput::fail("Browser use is not available.").to_response(),
        );
        return;
    };
    let browser = state.browser.clone();
    let reply_to = child.clone();
    let app_for_task = app.clone();
    let (thread_id, turn_id) = (call.thread_id.clone(), call.turn_id.clone());
    // Held across spawn and insert, so a call that finishes at once cannot
    // try to forget itself before it is recorded.
    let mut calls = state.calls.lock().unwrap();
    let task = tauri::async_runtime::spawn(async move {
        let output = browser
            .call_tool(&call.thread_id, &call.tool, &call.arguments)
            .await;
        if let Some(state) = app_for_task.try_state::<BrowserState>() {
            state.forget_call(request_id);
        }
        let _ = reply_to.respond(request_id, output.to_response());
    });
    calls.insert(
        request_id,
        Call {
            thread_id,
            turn_id,
            child: child.clone(),
            task,
        },
    );
}

/// Everything the Browser panel shows for one thread.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BrowserOverview {
    pub status: BrowserStatus,
    pub enabled: bool,
    pub channel: Option<BrowserChannel>,
    pub thread: Option<ThreadBrowser>,
    pub trusted_sites: Vec<String>,
    pub approvals: Vec<BrowserApproval>,
    pub helper_extension_id: String,
    /// Where `browser_install_helper` puts the extension.
    pub helper_dir: String,
}

#[tauri::command]
#[specta::specta]
pub(crate) fn browser_overview(
    thread_id: Option<String>,
    state: State<'_, BrowserState>,
) -> BrowserOverview {
    let overrides = prefs::read_overrides(&prefs::settings_path());
    BrowserOverview {
        status: state.browser.status(),
        enabled: overrides.browser_use.unwrap_or(false),
        channel: overrides.browser_channel,
        thread: thread_id
            .as_deref()
            .map(|thread| state.browser.thread_view(thread)),
        trusted_sites: state.browser.trusted_sites(),
        approvals: state.ui.approvals.list(),
        helper_extension_id: helper::HELPER_EXTENSION_ID.to_string(),
        helper_dir: helper::extension_dir().to_string_lossy().into_owned(),
    }
}

/// Install (or refresh) the Pingex Browser Helper into Pingex's app-data
/// folder and show it in the file manager, ready for "Load unpacked".
#[tauri::command]
#[specta::specta]
pub(crate) fn browser_install_helper(reveal: bool) -> Result<String, String> {
    let dir = helper::extension_dir();
    helper::install_extension(&dir)?;
    if reveal {
        crate::os::reveal_command(&dir.join("manifest.json"), std::env::consts::OS)
            .status()
            .map_err(|error| format!("Could not open the file manager: {error}"))?;
    }
    Ok(dir.to_string_lossy().into_owned())
}

/// Connect now (the first tool call would otherwise do it).
#[tauri::command]
#[specta::specta]
pub(crate) async fn browser_connect(
    state: State<'_, BrowserState>,
) -> Result<BrowserStatus, String> {
    state.browser.connection().await?;
    Ok(state.browser.status())
}

#[tauri::command]
#[specta::specta]
pub(crate) fn browser_set_settings(
    enabled: bool,
    channel: Option<BrowserChannel>,
    state: State<'_, BrowserState>,
) -> Result<(), String> {
    prefs::update_overrides(&prefs::settings_path(), |overrides| {
        overrides.browser_use = Some(enabled);
        overrides.browser_channel = channel;
        Ok(())
    })?;
    state.browser.set_preferred(channel);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub(crate) fn browser_resolve_approval(
    id: String,
    decision: BrowserDecision,
    state: State<'_, BrowserState>,
) -> bool {
    state.ui.approvals.resolve(&state.ui.app, &id, decision)
}

/// Name a thread's tab group after the thread.
#[tauri::command]
#[specta::specta]
pub(crate) fn browser_label_thread(
    thread_id: String,
    label: String,
    state: State<'_, BrowserState>,
) {
    state.browser.set_label(&thread_id, &label);
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn browser_release_tab(
    thread_id: String,
    target_id: String,
    state: State<'_, BrowserState>,
) -> Result<(), String> {
    let browser = state.browser.clone();
    let conn = browser.connection().await?;
    browser.release(&conn, &thread_id, &target_id).await;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn browser_focus_tab(
    chrome_tab_id: i64,
    state: State<'_, BrowserState>,
) -> Result<(), String> {
    let conn = state.browser.connection().await?;
    helper::focus(&conn, chrome_tab_id).await
}

/// Revoke a site grant: one thread's, or (no thread) an "Always allow".
#[tauri::command]
#[specta::specta]
pub(crate) fn browser_revoke_site(
    thread_id: Option<String>,
    site: String,
    state: State<'_, BrowserState>,
) -> Result<(), String> {
    state.browser.revoke_site(thread_id.as_deref(), &site);
    if thread_id.is_none() {
        prefs::update_overrides(&prefs::settings_path(), |overrides| {
            overrides
                .browser_trusted_sites
                .retain(|trusted| trusted != &site);
            Ok(())
        })?;
    }
    if let Some(thread) = thread_id {
        state.ui.tabs_changed(&thread);
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub(crate) fn browser_set_full_access(
    thread_id: String,
    enabled: bool,
    state: State<'_, BrowserState>,
) {
    state.browser.set_full_access(&thread_id, enabled);
}

/// Stop the thread's browser work now: cancel its pending calls (their
/// approvals go with them). Codex sees the calls fail.
#[tauri::command]
#[specta::specta]
pub(crate) fn browser_stop(thread_id: String, state: State<'_, BrowserState>) {
    state.stop_thread(&thread_id);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The `item/tool/call` params exactly as upstream's v2 schema spells them
    /// (`DynamicToolCallParams`), so a rename there fails here.
    #[test]
    fn tool_calls_decode_from_the_v2_wire_shape() {
        let call: ToolCall = serde_json::from_value(json!({
            "threadId": "019a-thread", "turnId": "turn-3", "callId": "call_9",
            "namespace": "pingex_browser", "tool": "click",
            "arguments": {"tab": "t1", "ref": "e12"},
        }))
        .unwrap();
        assert_eq!(call.namespace.as_deref(), Some(NAMESPACE));
        assert_eq!(call.tool, "click");
        assert_eq!(call.arguments["ref"], "e12");
        let bare: ToolCall = serde_json::from_value(json!({
            "threadId": "t", "turnId": "u", "callId": "c", "namespace": null, "tool": "x", "arguments": null,
        }))
        .unwrap();
        assert_eq!(bare.namespace, None);
    }

    /// The reply matches `DynamicToolCallResponse`: `contentItems` + `success`.
    #[test]
    fn failures_reply_in_the_v2_response_shape() {
        assert_eq!(
            ToolOutput::fail("nope").to_response(),
            json!({"contentItems": [{"type": "inputText", "text": "nope"}], "success": false})
        );
    }
}
