//! One owned tab: its two CDP sessions, its event buffers and the actions
//! the tools are built from.
//!
//! Each tab gets two sessions. The *work* session carries every tool call
//! and every raw `cdp` call. The *gate* session does one thing: it holds
//! `Fetch` interception on main-frame documents so `mod.rs` can stop a
//! navigation to a site the thread has no grant for. Keeping the gate on its
//! own session means the agent's own `Fetch.enable` (raw CDP) cannot replace
//! or switch off Pingex's.

use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::cdp::{CdpError, CdpEvent};
use super::snapshot::{self, RefMap};
use super::Conn;

const LOG_LIMIT: usize = 500;
const RAW_LIMIT: usize = 2000;

#[derive(Default)]
pub(crate) struct Log {
    seq: u64,
    pub console: VecDeque<(u64, String)>,
    pub network: VecDeque<(u64, String)>,
    pub raw: VecDeque<(u64, CdpEvent)>,
    pub console_read: u64,
    pub network_read: u64,
    pub raw_read: u64,
    requests: HashMap<String, (String, String, String)>,
    /// The main frame committed a new document since the last snapshot.
    pub navigated: bool,
}

impl Log {
    fn push(queue: &mut VecDeque<(u64, String)>, seq: u64, line: String) {
        queue.push_back((seq, line));
        if queue.len() > LOG_LIMIT {
            queue.pop_front();
        }
    }
}

/// What actions on the tab share, one action at a time.
#[derive(Default)]
pub(crate) struct Doc {
    pub refs: RefMap,
    /// The last full-page snapshot, for diffs.
    pub last: Vec<String>,
}

pub(crate) struct Tab {
    pub target_id: String,
    pub thread_id: String,
    pub work: String,
    pub gate: String,
    pub doc: tokio::sync::Mutex<Doc>,
    pub log: Mutex<Log>,
    pub raw_arrived: tokio::sync::Notify,
}

fn err(error: CdpError) -> String {
    error.to_string()
}

impl Tab {
    pub(crate) async fn attach(
        conn: &Conn,
        thread_id: &str,
        target_id: &str,
    ) -> Result<std::sync::Arc<Tab>, String> {
        let attach = || async {
            conn.cdp
                .call(
                    None,
                    "Target.attachToTarget",
                    json!({"targetId": target_id, "flatten": true}),
                )
                .await
                .map_err(err)
                .and_then(|result| {
                    result["sessionId"]
                        .as_str()
                        .map(str::to_string)
                        .ok_or_else(|| "no session id".to_string())
                })
        };
        let work = attach().await?;
        let gate = attach().await?;
        let tab = std::sync::Arc::new(Tab {
            target_id: target_id.to_string(),
            thread_id: thread_id.to_string(),
            work: work.clone(),
            gate: gate.clone(),
            doc: tokio::sync::Mutex::default(),
            log: Mutex::default(),
            raw_arrived: tokio::sync::Notify::new(),
        });
        {
            let mut sessions = conn.sessions.lock().unwrap();
            sessions.insert(work.clone(), (target_id.to_string(), false));
            sessions.insert(gate.clone(), (target_id.to_string(), true));
        }
        // Registered before enabling, so the tab is findable when the first
        // paused request or console message arrives.
        conn.tabs
            .lock()
            .unwrap()
            .insert(target_id.to_string(), tab.clone());
        conn.cdp
            .call(
                Some(&gate),
                "Fetch.enable",
                json!({"patterns": [{"urlPattern": "*", "resourceType": "Document", "requestStage": "Request"}]}),
            )
            .await
            .map_err(err)?;
        for (method, params) in [
            ("Page.enable", json!({})),
            ("Runtime.enable", json!({})),
            ("Network.enable", json!({})),
            ("Log.enable", json!({})),
            // A background tab still behaves as focused for the agent.
            (
                "Emulation.setFocusEmulationEnabled",
                json!({"enabled": true}),
            ),
        ] {
            conn.cdp
                .call(Some(&work), method, params)
                .await
                .map_err(err)?;
        }
        Ok(tab)
    }

    pub(crate) async fn detach(&self, conn: &Conn) {
        for session in [&self.work, &self.gate] {
            let _ = conn
                .cdp
                .call(
                    None,
                    "Target.detachFromTarget",
                    json!({"sessionId": session}),
                )
                .await;
        }
    }

    /// Buffer an event from the work session (or one of its children).
    pub(crate) fn record(&self, event: &CdpEvent) {
        let mut log = self.log.lock().unwrap();
        log.seq += 1;
        let seq = log.seq;
        let params = &event.params;
        match event.method.as_str() {
            "Runtime.consoleAPICalled" => {
                let text = params["args"]
                    .as_array()
                    .map(|args| {
                        args.iter()
                            .map(|arg| match &arg["value"] {
                                Value::String(text) => text.clone(),
                                Value::Null => arg["description"]
                                    .as_str()
                                    .unwrap_or(arg["type"].as_str().unwrap_or(""))
                                    .to_string(),
                                other => other.to_string(),
                            })
                            .collect::<Vec<_>>()
                            .join(" ")
                    })
                    .unwrap_or_default();
                let kind = params["type"].as_str().unwrap_or("log");
                Log::push(
                    &mut log.console,
                    seq,
                    format!("[{kind}] {}", clip(&text, 500)),
                );
            }
            "Runtime.exceptionThrown" => {
                let details = &params["exceptionDetails"];
                let text = details["exception"]["description"]
                    .as_str()
                    .or(details["text"].as_str())
                    .unwrap_or("exception");
                Log::push(
                    &mut log.console,
                    seq,
                    format!("[exception] {}", clip(text, 500)),
                );
            }
            "Log.entryAdded" => {
                let entry = &params["entry"];
                let line = format!(
                    "[{}] {}",
                    entry["level"].as_str().unwrap_or("log"),
                    clip(entry["text"].as_str().unwrap_or(""), 500)
                );
                Log::push(&mut log.console, seq, line);
            }
            "Network.requestWillBeSent" => {
                let id = params["requestId"].as_str().unwrap_or_default().to_string();
                let request = &params["request"];
                log.requests.insert(
                    id,
                    (
                        request["method"].as_str().unwrap_or("GET").to_string(),
                        request["url"].as_str().unwrap_or_default().to_string(),
                        params["type"].as_str().unwrap_or("").to_string(),
                    ),
                );
                if log.requests.len() > LOG_LIMIT * 2 {
                    log.requests.clear();
                }
            }
            "Network.responseReceived" | "Network.loadingFailed" => {
                let id = params["requestId"].as_str().unwrap_or_default();
                if let Some((method, url, kind)) = log.requests.remove(id) {
                    let outcome = if event.method == "Network.responseReceived" {
                        params["response"]["status"].to_string()
                    } else {
                        params["errorText"].as_str().unwrap_or("failed").to_string()
                    };
                    let line = format!("{outcome} {method} {} ({kind})", clip(&url, 200));
                    Log::push(&mut log.network, seq, line);
                }
            }
            "Page.frameNavigated" if params["frame"].get("parentId").is_none() => {
                log.navigated = true;
            }
            _ => {}
        }
        log.raw.push_back((seq, event.clone()));
        if log.raw.len() > RAW_LIMIT {
            log.raw.pop_front();
        }
        drop(log);
        self.raw_arrived.notify_waiters();
    }

    pub(crate) async fn call(
        &self,
        conn: &Conn,
        method: &str,
        params: Value,
    ) -> Result<Value, String> {
        conn.cdp
            .call(Some(&self.work), method, params)
            .await
            .map_err(err)
    }

    /// Evaluate `expression` in the page; the JSON value, or the exception.
    pub(crate) async fn evaluate(&self, conn: &Conn, expression: &str) -> Result<Value, String> {
        let result = self
            .call(
                conn,
                "Runtime.evaluate",
                json!({"expression": expression, "awaitPromise": true, "returnByValue": true, "userGesture": true}),
            )
            .await?;
        exception_or(result)
    }

    /// Run `function` with `this` bound to the element behind a backend id.
    pub(crate) async fn call_on(
        &self,
        conn: &Conn,
        backend: i64,
        function: &str,
        args: Vec<Value>,
    ) -> Result<Value, String> {
        let resolved = self
            .call(conn, "DOM.resolveNode", json!({"backendNodeId": backend}))
            .await
            .map_err(|_| "That element is gone; take a new snapshot.".to_string())?;
        let object = resolved["object"]["objectId"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let args: Vec<Value> = args
            .into_iter()
            .map(|value| json!({"value": value}))
            .collect();
        let result = self
            .call(
                conn,
                "Runtime.callFunctionOn",
                json!({"objectId": object, "functionDeclaration": function, "arguments": args, "awaitPromise": true, "returnByValue": true, "userGesture": true}),
            )
            .await?;
        let _ = self
            .call(conn, "Runtime.releaseObject", json!({"objectId": object}))
            .await;
        exception_or(result)
    }

    pub(crate) async fn url_and_title(&self, conn: &Conn) -> (String, String) {
        let value = self
            .evaluate(conn, "({url: location.href, title: document.title})")
            .await
            .unwrap_or(Value::Null);
        (
            value["url"].as_str().unwrap_or_default().to_string(),
            value["title"].as_str().unwrap_or_default().to_string(),
        )
    }

    /// Wait for the document to finish loading (and a beat for scripts that
    /// render after load). Errors while a navigation swaps the document are
    /// expected and retried.
    pub(crate) async fn settle(&self, conn: &Conn, timeout: Duration) {
        let start = Instant::now();
        tokio::time::sleep(Duration::from_millis(150)).await;
        while start.elapsed() < timeout {
            if let Ok(Value::String(state)) = self.evaluate(conn, "document.readyState").await {
                if state == "complete" {
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    return;
                }
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    /// The page as outline lines. `scope` limits it to one element's subtree.
    pub(crate) async fn snapshot(
        &self,
        conn: &Conn,
        doc: &mut Doc,
        scope: Option<i64>,
    ) -> Result<Vec<String>, String> {
        if std::mem::take(&mut self.log.lock().unwrap().navigated) {
            doc.refs.reset();
            doc.last.clear();
        }
        let tree = self
            .call(conn, "Accessibility.getFullAXTree", json!({}))
            .await?;
        let nodes = tree["nodes"].as_array().cloned().unwrap_or_default();
        Ok(snapshot::render(&nodes, &mut doc.refs, scope))
    }

    pub(crate) fn backend_for(&self, doc: &Doc, reference: &str) -> Result<i64, String> {
        snapshot::parse_ref(reference)
            .and_then(|number| doc.refs.backend(number))
            .ok_or_else(|| {
                format!("Unknown ref {reference:?}; take a new snapshot to get current refs.")
            })
    }

    /// The element's centre in viewport coordinates, scrolled into view.
    pub(crate) async fn center_of(&self, conn: &Conn, backend: i64) -> Result<(f64, f64), String> {
        let _ = self
            .call(
                conn,
                "DOM.scrollIntoViewIfNeeded",
                json!({"backendNodeId": backend}),
            )
            .await;
        let model = self
            .call(conn, "DOM.getBoxModel", json!({"backendNodeId": backend}))
            .await
            .map_err(|_| "That element has no box (hidden or detached).".to_string())?;
        let quad: Vec<f64> = model["model"]["content"]
            .as_array()
            .map(|points| points.iter().filter_map(Value::as_f64).collect())
            .unwrap_or_default();
        if quad.len() < 8 {
            return Err("That element has no box (hidden or detached).".into());
        }
        let x = (quad[0] + quad[2] + quad[4] + quad[6]) / 4.0;
        let y = (quad[1] + quad[3] + quad[5] + quad[7]) / 4.0;
        Ok((x, y))
    }

    pub(crate) async fn click_at(
        &self,
        conn: &Conn,
        x: f64,
        y: f64,
        count: i64,
    ) -> Result<(), String> {
        self.call(
            conn,
            "Input.dispatchMouseEvent",
            json!({"type": "mouseMoved", "x": x, "y": y}),
        )
        .await?;
        for kind in ["mousePressed", "mouseReleased"] {
            self.call(
                conn,
                "Input.dispatchMouseEvent",
                json!({"type": kind, "x": x, "y": y, "button": "left", "clickCount": count}),
            )
            .await?;
        }
        Ok(())
    }

    pub(crate) async fn press(&self, conn: &Conn, chord: &str) -> Result<(), String> {
        for event in super::keys::key_events(chord)? {
            self.call(conn, "Input.dispatchKeyEvent", event).await?;
        }
        Ok(())
    }

    /// Lines buffered since the last read of `which`, and advance the cursor.
    pub(crate) fn drain(
        &self,
        which: Buffer,
        filter: Option<&str>,
        limit: usize,
    ) -> (Vec<String>, usize) {
        let mut log = self.log.lock().unwrap();
        let (queue, cursor) = match which {
            Buffer::Console => (log.console.clone(), log.console_read),
            Buffer::Network => (log.network.clone(), log.network_read),
        };
        let fresh: Vec<&(u64, String)> = queue
            .iter()
            .filter(|(seq, line)| {
                *seq > cursor
                    && filter
                        .is_none_or(|needle| line.to_lowercase().contains(&needle.to_lowercase()))
            })
            .collect();
        let skipped = fresh.len().saturating_sub(limit);
        let lines: Vec<String> = fresh[skipped..]
            .iter()
            .map(|(_, line)| line.clone())
            .collect();
        let last = queue.back().map(|(seq, _)| *seq).unwrap_or(cursor);
        match which {
            Buffer::Console => log.console_read = last,
            Buffer::Network => log.network_read = last,
        }
        (lines, skipped)
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Buffer {
    Console,
    Network,
}

fn exception_or(result: Value) -> Result<Value, String> {
    if let Some(exception) = result.get("exceptionDetails") {
        let text = exception["exception"]["description"]
            .as_str()
            .or(exception["text"].as_str())
            .unwrap_or("evaluation failed");
        return Err(clip(text, 1000));
    }
    let value = &result["result"];
    Ok(match value.get("value") {
        Some(value) => value.clone(),
        None => match value["type"].as_str() {
            Some("undefined") => Value::Null,
            _ => value["description"].clone(),
        },
    })
}

pub(crate) fn clip(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let mut cut: String = text.chars().take(limit).collect();
    cut.push('…');
    cut
}
