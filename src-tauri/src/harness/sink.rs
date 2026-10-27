//! Where a driver's neutral events go: out on `harness:event`, and projected
//! onto the journal and `codex:event`, which is what the transcript renders
//! today (see the module docs of [`super`]).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::AppHandle;
use tauri_specta::Event;

use super::project::Projector;
use super::{
    HarnessEvent, HarnessEventEnvelope, HarnessKind, HarnessRequest, HarnessRequestEnvelope,
};
use crate::codex::events::{CodexEvent, CodexNotification};
use crate::codex::journal::TurnJournal;

/// One thread's event stream, for the life of one process.
pub(crate) struct EventSink {
    harness: HarnessKind,
    home_key: String,
    thread_id: String,
    app: AppHandle,
    seq: AtomicU64,
    projector: Mutex<Projector>,
    journal: TurnJournal,
}

impl EventSink {
    pub(crate) fn new(
        harness: HarnessKind,
        home_key: &str,
        thread_id: &str,
        app: &AppHandle,
    ) -> Self {
        Self {
            harness,
            home_key: home_key.to_string(),
            thread_id: thread_id.to_string(),
            app: app.clone(),
            seq: AtomicU64::new(0),
            projector: Mutex::new(Projector::default()),
            journal: TurnJournal::new(app.clone(), home_key.to_string(), harness),
        }
    }

    pub(crate) fn app(&self) -> &AppHandle {
        &self.app
    }

    pub(crate) fn home_key(&self) -> &str {
        &self.home_key
    }

    /// Push one event under `turn_id` (a turn's closing events are produced
    /// after the driver has let go of its id, so it is passed explicitly).
    pub(crate) fn emit(&self, turn_id: Option<&str>, event: HarnessEvent) {
        let seq = self.seq.fetch_add(1, Ordering::SeqCst);
        let _ = HarnessEventEnvelope {
            codex_home: self.home_key.clone(),
            harness: self.harness,
            thread_id: self.thread_id.clone(),
            turn_id: turn_id.map(str::to_string),
            seq,
            event: event.clone(),
        }
        .emit(&self.app);
        let notifications = match self.projector.lock() {
            Ok(mut projector) => {
                projector.project(&self.thread_id, turn_id.unwrap_or_default(), &event)
            }
            Err(_) => Vec::new(),
        };
        for (method, params) in notifications {
            self.journal.observe(method, &params);
            let _ = CodexEvent {
                codex_home: self.home_key.clone(),
                event: CodexNotification::decode(method, &params),
            }
            .emit(&self.app);
        }
    }

    pub(crate) fn emit_all(&self, turn_id: Option<&str>, events: Vec<HarnessEvent>) {
        for event in events {
            self.emit(turn_id, event);
        }
    }

    /// Put a request in front of the user.
    pub(crate) fn request(
        &self,
        request_id: i64,
        turn_id: &str,
        item_id: &str,
        request: HarnessRequest,
    ) {
        let _ = HarnessRequestEnvelope {
            codex_home: self.home_key.clone(),
            harness: self.harness,
            request_id,
            thread_id: self.thread_id.clone(),
            turn_id: turn_id.to_string(),
            item_id: item_id.to_string(),
            request,
        }
        .emit(&self.app);
    }
}
