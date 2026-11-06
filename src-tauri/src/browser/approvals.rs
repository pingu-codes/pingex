//! Questions Pingex asks the user on the browser's behalf: may this thread
//! visit a site, take over a tab, upload a file, reach the whole browser.
//!
//! These are Pingex's own approvals, not Codex server requests: the dynamic
//! tool call that needs one simply waits on it. The pending set lives in
//! Rust so a reloaded webview can fetch it again (`browser_state`).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::AppHandle;
use tauri_specta::Event;
use tokio::sync::oneshot;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) enum BrowserApprovalKind {
    /// Visit (or let an owned tab navigate to) a site the thread has no
    /// grant for.
    Site,
    /// Move one of the user's tabs into the thread's group.
    TakeTab,
    /// Upload local files into a page.
    Upload,
    /// Reach beyond owned tabs: every tab, every site's cookies, the
    /// browser target.
    FullAccess,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BrowserApproval {
    pub id: String,
    pub thread_id: String,
    pub kind: BrowserApprovalKind,
    /// The site in question (`github.com`), when there is one.
    pub site: Option<String>,
    pub url: Option<String>,
    pub tab_title: Option<String>,
    /// One line on what the agent is trying to do.
    pub detail: String,
}

/// A new question for the user.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BrowserApprovalRequested {
    pub approval: BrowserApproval,
}

/// A question went away: answered here or in another window, or its call
/// was cancelled.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BrowserApprovalResolved {
    pub id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) enum BrowserDecision {
    Deny,
    /// For this thread.
    Allow,
    /// For this thread and every later one in this profile (sites only).
    AlwaysAllow,
}

struct Pending {
    approval: BrowserApproval,
    reply: oneshot::Sender<BrowserDecision>,
}

#[derive(Default)]
pub(crate) struct Approvals {
    pending: Mutex<HashMap<String, Pending>>,
}

/// Removes the question if the waiting call is dropped (cancelled turn), so
/// a stale card does not linger.
struct Cleanup<'a> {
    approvals: &'a Approvals,
    app: &'a AppHandle,
    id: String,
}

impl Drop for Cleanup<'_> {
    fn drop(&mut self) {
        if self
            .approvals
            .pending
            .lock()
            .unwrap()
            .remove(&self.id)
            .is_some()
        {
            let _ = BrowserApprovalResolved {
                id: self.id.clone(),
            }
            .emit(self.app);
        }
    }
}

impl Approvals {
    pub(crate) async fn ask(
        &self,
        app: &AppHandle,
        mut approval: BrowserApproval,
    ) -> BrowserDecision {
        approval.id = uuid::Uuid::new_v4().to_string();
        let (reply, answer) = oneshot::channel();
        let id = approval.id.clone();
        self.pending.lock().unwrap().insert(
            id.clone(),
            Pending {
                approval: approval.clone(),
                reply,
            },
        );
        let _cleanup = Cleanup {
            approvals: self,
            app,
            id,
        };
        let _ = BrowserApprovalRequested { approval }.emit(app);
        answer.await.unwrap_or(BrowserDecision::Deny)
    }

    pub(crate) fn resolve(&self, app: &AppHandle, id: &str, decision: BrowserDecision) -> bool {
        let Some(pending) = self.pending.lock().unwrap().remove(id) else {
            return false;
        };
        let _ = pending.reply.send(decision);
        let _ = BrowserApprovalResolved { id: id.to_string() }.emit(app);
        true
    }

    pub(crate) fn list(&self) -> Vec<BrowserApproval> {
        self.pending
            .lock()
            .unwrap()
            .values()
            .map(|pending| pending.approval.clone())
            .collect()
    }
}
