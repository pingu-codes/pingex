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

/// What the reader, the writer and callers share. Whichever side notices the
/// socket is gone first shuts it down, once.
struct Link {
    pending: Pending,
    events: broadcast::Sender<CdpEvent>,
    closed: AtomicBool,
    /// Why the socket closed, for the Browser panel and the log.
    reason: Mutex<Option<String>>,
}

impl Link {
    fn shut(&self, reason: String) {
        if self.closed.swap(true, Ordering::SeqCst) {
            return;
        }
        *self.reason.lock().unwrap() = Some(reason);
        let waiting: Vec<_> = self.pending.lock().unwrap().drain().collect();
        for (_, reply) in waiting {
            let _ = reply.send(Err(CdpError::new("the browser connection closed")));
        }
        // Wake every subscriber so it notices the close.
        let _ = self.events.send(CdpEvent {
            method: CLOSED_EVENT.into(),
            params: Value::Null,
            session_id: None,
        });
    }

    /// One message off the socket. A reply that will not parse still answers
    /// its caller (with an error) instead of leaving it waiting.
    fn receive(&self, text: &str) {
        let value = serde_json::from_str::<Value>(text).or_else(|error| {
            serde_json::from_str::<Value>(&replace_lone_surrogates(text)).map_err(|_| error)
        });
        match value {
            Ok(value) => dispatch(&self.pending, &self.events, value),
            Err(error) => {
                let Some(id) = leading_id(text) else {
                    return;
                };
                if let Some(reply) = self.pending.lock().unwrap().remove(&id) {
                    let _ = reply.send(Err(CdpError::new(format!(
                        "the browser's reply did not parse: {error}"
                    ))));
                }
            }
        }
    }
}

pub(crate) struct Cdp {
    outgoing: mpsc::UnboundedSender<String>,
    link: Arc<Link>,
    next_id: AtomicU64,
}

impl Cdp {
    /// Open `ws_url`. Against a user's Chrome in auto-connect mode the
    /// handshake does not finish until the user allows the connection in
    /// Chrome's own dialog, so callers put their own timeout around this.
    ///
    /// Chrome asks again for every new socket, so this one has to last: only
    /// the browser closing it, or a read or write failing, ends it. Any read
    /// error fuses a tokio-tungstenite stream, so there is no reading on.
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
        let (events, _) = broadcast::channel(8192);
        let link = Arc::new(Link {
            pending: Mutex::default(),
            events,
            closed: AtomicBool::new(false),
            reason: Mutex::new(None),
        });

        {
            let link = link.clone();
            tauri::async_runtime::spawn(async move {
                while let Some(text) = queue.recv().await {
                    if let Err(error) = write.send(Message::Text(text.into())).await {
                        link.shut(format!("writing to the browser failed: {error}"));
                        break;
                    }
                }
            });
        }
        {
            let link = link.clone();
            tauri::async_runtime::spawn(async move {
                let reason = loop {
                    match read.next().await {
                        Some(Ok(Message::Text(text))) => link.receive(&text),
                        Some(Ok(Message::Binary(bytes))) => {
                            link.receive(&String::from_utf8_lossy(&bytes))
                        }
                        Some(Ok(Message::Close(frame))) => {
                            break match frame {
                                Some(frame) => format!(
                                    "the browser closed it ({} {})",
                                    u16::from(frame.code),
                                    frame.reason
                                ),
                                None => "the browser closed it".to_string(),
                            }
                        }
                        Some(Ok(_)) => {}
                        Some(Err(error)) => break format!("reading from the browser failed: {error}"),
                        None => break "the socket ended".to_string(),
                    }
                };
                link.shut(reason);
            });
        }
        Ok(Arc::new(Self {
            outgoing,
            link,
            next_id: AtomicU64::new(1),
        }))
    }

    /// Why the socket closed, once it has.
    pub(crate) fn closed_reason(&self) -> Option<String> {
        self.link.reason.lock().unwrap().clone()
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.link.closed.load(Ordering::SeqCst)
    }

    pub(crate) fn subscribe(&self) -> broadcast::Receiver<CdpEvent> {
        self.link.events.subscribe()
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
        self.link.pending.lock().unwrap().insert(id, reply);
        if self.outgoing.send(message.to_string()).is_err() {
            self.link.pending.lock().unwrap().remove(&id);
            return Err(CdpError::new("the browser connection closed"));
        }
        match tokio::time::timeout(CALL_TIMEOUT, receive).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(CdpError::new("the browser connection closed")),
            Err(_) => {
                self.link.pending.lock().unwrap().remove(&id);
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

/// Swap every unpaired UTF-16 surrogate escape (`\ud83d` with no low half)
/// for U+FFFD. Chrome escapes JS strings code unit by code unit, so a string
/// cut mid-emoji arrives that way, and serde_json refuses the whole message.
fn replace_lone_surrogates(text: &str) -> String {
    fn unit_at(bytes: &[u8], at: usize) -> Option<u16> {
        if bytes.get(at) != Some(&b'\\') || bytes.get(at + 1) != Some(&b'u') {
            return None;
        }
        let hex = std::str::from_utf8(bytes.get(at + 2..at + 6)?).ok()?;
        u16::from_str_radix(hex, 16).ok()
    }
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut copied = 0;
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] != b'\\' {
            at += 1;
            continue;
        }
        let Some(unit) = unit_at(bytes, at) else {
            // Some other escape (`\\`, `\"`, ...): keep it whole.
            at += 2;
            continue;
        };
        let lone = match unit {
            0xD800..=0xDBFF => {
                let paired = unit_at(bytes, at + 6).is_some_and(|low| (0xDC00..=0xDFFF).contains(&low));
                if paired {
                    at += 12;
                    continue;
                }
                true
            }
            0xDC00..=0xDFFF => true,
            _ => false,
        };
        if lone {
            out.push_str(&text[copied..at]);
            out.push_str("\\ufffd");
            copied = at + 6;
        }
        at += 6;
    }
    out.push_str(&text[copied..]);
    out
}

/// The `id` of a reply, read without parsing it: Chrome writes `{"id":N,`
/// first.
fn leading_id(text: &str) -> Option<u64> {
    let rest = text.trim_start().strip_prefix('{')?.trim_start();
    let rest = rest.strip_prefix("\"id\"")?.trim_start().strip_prefix(':')?.trim_start();
    let digits = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
    rest[..digits].parse().ok()
}

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
    #[test]
    fn lone_surrogate_escapes_become_replacement_characters() {
        let cut = r#"{"id":1,"result":{"title":"ab\ud83d","pair":"😀","low":"\udc00x","esc":"\\ud83d"}}"#;
        assert!(serde_json::from_str::<Value>(cut).is_err());
        let value: Value = serde_json::from_str(&replace_lone_surrogates(cut)).unwrap();
        assert_eq!(value["result"]["title"], "ab\u{fffd}");
        assert_eq!(value["result"]["pair"], "😀");
        assert_eq!(value["result"]["low"], "\u{fffd}x");
        // An escaped backslash before `u` is not an escape.
        assert_eq!(value["result"]["esc"], "\\ud83d");
        assert_eq!(replace_lone_surrogates("plain"), "plain");
    }

    #[test]
    fn replies_are_identified_without_parsing() {
        assert_eq!(leading_id(r#"{"id":42,"result":{}}"#), Some(42));
        assert_eq!(leading_id(r#" { "id" : 7 , "x":1}"#), Some(7));
        assert_eq!(leading_id(r#"{"method":"Page.x","params":{}}"#), None);
    }

    fn link() -> Link {
        Link {
            pending: Mutex::default(),
            events: broadcast::channel(4).0,
            closed: AtomicBool::new(false),
            reason: Mutex::new(None),
        }
    }

    #[test]
    fn an_unparseable_reply_still_answers_its_caller() {
        let link = link();
        let (reply, mut result) = oneshot::channel();
        link.pending.lock().unwrap().insert(3, reply);
        link.receive(r#"{"id":3,"result":{"broken": }"#);
        let error = result.try_recv().unwrap().unwrap_err();
        assert!(error.message.contains("did not parse"), "{error}");
        assert!(!link.closed.load(Ordering::SeqCst));
    }

    #[test]
    fn shutting_down_happens_once_and_keeps_the_first_reason() {
        let link = link();
        let mut events = link.events.subscribe();
        let (reply, mut result) = oneshot::channel();
        link.pending.lock().unwrap().insert(1, reply);
        link.shut("first".into());
        link.shut("second".into());
        assert_eq!(link.reason.lock().unwrap().as_deref(), Some("first"));
        assert!(result.try_recv().unwrap().is_err());
        assert_eq!(events.try_recv().unwrap().method, CLOSED_EVENT);
        assert!(events.try_recv().is_err());
    }

    /// A one-connection WebSocket server on localhost; `serve` runs on the
    /// accepted socket.
    async fn server<F, Fut>(serve: F) -> String
    where
        F: FnOnce(tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>) -> Fut
            + Send
            + 'static,
        Fut: std::future::Future<Output = ()> + Send,
    {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let socket = tokio_tungstenite::accept_async(stream).await.unwrap();
            serve(socket).await;
        });
        format!("ws://127.0.0.1:{port}/devtools/browser/test")
    }

    #[tokio::test]
    async fn a_reply_with_a_cut_emoji_resolves_and_the_socket_stays_open() {
        let url = server(|mut socket| async move {
            let Some(Ok(Message::Text(request))) = socket.next().await else {
                return;
            };
            let id = serde_json::from_str::<Value>(&request).unwrap()["id"].clone();
            let reply = format!(r#"{{"id":{id},"result":{{"value":"hi\ud83d"}}}}"#);
            socket.send(Message::Text(reply.into())).await.unwrap();
            // Hold the socket open until the client goes.
            while socket.next().await.is_some() {}
        })
        .await;
        let cdp = Cdp::connect(&url).await.unwrap();
        let result = cdp.call(None, "Runtime.evaluate", json!({})).await.unwrap();
        assert_eq!(result["value"], "hi\u{fffd}");
        assert!(!cdp.is_closed());
    }

    #[tokio::test]
    async fn a_close_from_the_browser_records_why() {
        use tokio_tungstenite::tungstenite::protocol::{frame::coding::CloseCode, CloseFrame};
        let url = server(|mut socket| async move {
            let _ = socket
                .close(Some(CloseFrame {
                    code: CloseCode::Policy,
                    reason: "revoked".into(),
                }))
                .await;
        })
        .await;
        let cdp = Cdp::connect(&url).await.unwrap();
        let mut events = cdp.subscribe();
        if !cdp.is_closed() {
            while events.recv().await.unwrap().method != CLOSED_EVENT {}
        }
        let reason = cdp.closed_reason().unwrap();
        assert!(reason.contains("1008") && reason.contains("revoked"), "{reason}");
        assert!(cdp.call(None, "Browser.getVersion", json!({})).await.is_err());
    }
}
