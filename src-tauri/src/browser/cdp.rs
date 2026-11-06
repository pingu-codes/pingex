//! One DevTools Protocol connection to a browser, multiplexing every
//! flattened session over a single WebSocket.
//!
//! Calls are matched to replies by id; everything without an id is an event
//! and goes to a broadcast channel that the browser's dispatcher drains.

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::tungstenite::Message;

/// How long a single CDP call may take before the caller gives up. Long
/// enough for a full-page screenshot or a slow navigation commit.
const CALL_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CdpError {
    pub code: Option<i64>,
    pub message: String,
}

impl std::fmt::Display for CdpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.code {
            Some(code) => write!(f, "{} ({code})", self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl CdpError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            code: None,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CdpEvent {
    pub method: String,
    pub params: Value,
    pub session_id: Option<String>,
}

type Pending = Mutex<HashMap<u64, oneshot::Sender<Result<Value, CdpError>>>>;

pub(crate) struct Cdp {
    outgoing: mpsc::UnboundedSender<String>,
    pending: Arc<Pending>,
    next_id: AtomicU64,
    events: broadcast::Sender<CdpEvent>,
    closed: Arc<AtomicBool>,
}

impl Cdp {
    /// Open `ws_url`. Against a user's Chrome in auto-connect mode the
    /// handshake does not finish until the user allows the connection in
    /// Chrome's own dialog, so callers put their own timeout around this.
    pub(crate) async fn connect(ws_url: &str) -> Result<Arc<Self>, String> {
        let mut config = WebSocketConfig::default();
        // Full-page screenshots and heap snapshot chunks are large.
        config.max_message_size = Some(512 << 20);
        config.max_frame_size = None;
        let (socket, _) = tokio_tungstenite::connect_async_with_config(ws_url, Some(config), false)
            .await
            .map_err(|error| error.to_string())?;
        let (mut write, mut read) = socket.split();
        let (outgoing, mut queue) = mpsc::unbounded_channel::<String>();
        let pending: Arc<Pending> = Arc::default();
        let (events, _) = broadcast::channel(8192);
        let closed = Arc::new(AtomicBool::new(false));

        tokio::spawn(async move {
            while let Some(text) = queue.recv().await {
                if write.send(Message::Text(text.into())).await.is_err() {
                    break;
                }
            }
        });
        {
            let pending = pending.clone();
            let events = events.clone();
            let closed = closed.clone();
            tokio::spawn(async move {
                while let Some(Ok(message)) = read.next().await {
                    let text = match message {
                        Message::Text(text) => text.to_string(),
                        Message::Binary(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
                        Message::Close(_) => break,
                        _ => continue,
                    };
                    let Ok(value) = serde_json::from_str::<Value>(&text) else {
                        continue;
                    };
                    dispatch(&pending, &events, value);
                }
                closed.store(true, Ordering::SeqCst);
                let waiting: Vec<_> = pending.lock().unwrap().drain().collect();
                for (_, reply) in waiting {
                    let _ = reply.send(Err(CdpError::new("the browser connection closed")));
                }
                // Wake every subscriber so it notices the close.
                let _ = events.send(CdpEvent {
                    method: CLOSED_EVENT.into(),
                    params: Value::Null,
                    session_id: None,
                });
            });
        }
        Ok(Arc::new(Self {
            outgoing,
            pending,
            next_id: AtomicU64::new(1),
            events,
            closed,
        }))
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    pub(crate) fn subscribe(&self) -> broadcast::Receiver<CdpEvent> {
        self.events.subscribe()
    }

    /// Send `method` on `session` (the browser target when `None`) and wait
    /// for its result.
    pub(crate) async fn call(
        &self,
        session: Option<&str>,
        method: &str,
        params: Value,
    ) -> Result<Value, CdpError> {
        if self.is_closed() {
            return Err(CdpError::new("the browser connection closed"));
        }
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let mut message = json!({"id": id, "method": method, "params": params});
        if let Some(session) = session {
            message["sessionId"] = json!(session);
        }
        let (reply, receive) = oneshot::channel();
        self.pending.lock().unwrap().insert(id, reply);
        if self.outgoing.send(message.to_string()).is_err() {
            self.pending.lock().unwrap().remove(&id);
            return Err(CdpError::new("the browser connection closed"));
        }
        match tokio::time::timeout(CALL_TIMEOUT, receive).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(CdpError::new("the browser connection closed")),
            Err(_) => {
                self.pending.lock().unwrap().remove(&id);
                Err(CdpError::new(format!(
                    "{method} timed out after {}s",
                    CALL_TIMEOUT.as_secs()
                )))
            }
        }
    }
}

/// Synthetic event sent once when the socket closes.
pub(crate) const CLOSED_EVENT: &str = "pingex.closed";

fn dispatch(pending: &Pending, events: &broadcast::Sender<CdpEvent>, value: Value) {
    if let Some(id) = value.get("id").and_then(Value::as_u64) {
        let Some(reply) = pending.lock().unwrap().remove(&id) else {
            return;
        };
        let result = match value.get("error") {
            Some(error) => Err(CdpError {
                code: error.get("code").and_then(Value::as_i64),
                message: error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("CDP error")
                    .to_string(),
            }),
            None => Ok(value.get("result").cloned().unwrap_or(Value::Null)),
        };
        let _ = reply.send(result);
        return;
    }
    let Some(method) = value.get("method").and_then(Value::as_str) else {
        return;
    };
    let _ = events.send(CdpEvent {
        method: method.to_string(),
        params: value.get("params").cloned().unwrap_or(Value::Null),
        session_id: value
            .get("sessionId")
            .and_then(Value::as_str)
            .map(str::to_string),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replies_resolve_their_caller_and_events_broadcast() {
        let pending: Pending = Mutex::default();
        let (events, mut receive) = broadcast::channel(4);
        let (reply, mut result) = oneshot::channel();
        pending.lock().unwrap().insert(7, reply);
        dispatch(&pending, &events, json!({"id": 7, "result": {"ok": 1}}));
        assert_eq!(result.try_recv().unwrap(), Ok(json!({"ok": 1})));

        let (reply, mut result) = oneshot::channel();
        pending.lock().unwrap().insert(8, reply);
        dispatch(
            &pending,
            &events,
            json!({"id": 8, "error": {"code": -32000, "message": "No node"}}),
        );
        assert_eq!(
            result.try_recv().unwrap(),
            Err(CdpError {
                code: Some(-32000),
                message: "No node".into()
            })
        );

        dispatch(
            &pending,
            &events,
            json!({"method": "Page.loadEventFired", "params": {}, "sessionId": "S"}),
        );
        let event = receive.try_recv().unwrap();
        assert_eq!(event.method, "Page.loadEventFired");
        assert_eq!(event.session_id.as_deref(), Some("S"));
    }
}
