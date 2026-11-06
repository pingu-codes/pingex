//! The `pingex_browser` dynamic tools Codex calls.
//!
//! The high-level tools come first and are built to be cheap to read:
//! snapshots are outlines with short refs, actions answer with only what
//! changed, screenshots are downscaled JPEGs. Raw CDP (`cdp`, `cdp_events`)
//! is the fallback for anything they do not cover; it and the rarely used
//! log tools are `deferLoading`, so their schemas cost nothing per turn
//! until the model looks them up.

use base64::Engine;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

use super::approvals::{BrowserApproval, BrowserApprovalKind, BrowserDecision};
use super::policy::{self, CallTarget, Verdict};
use super::snapshot;
use super::tab::{clip, Buffer, Tab};
use super::{helper, Browser, Conn, OwnedTab};

pub(crate) const NAMESPACE: &str = "pingex_browser";

/// Results longer than this are cut (or, for raw CDP, written to a file).
const TEXT_LIMIT: usize = 16_000;
/// Snapshot lines beyond this are left out with a note to narrow `ref`.
const SNAPSHOT_LINES: usize = 600;
/// Screenshots are scaled down to at most this many CSS pixels wide.
const SCREENSHOT_WIDTH: f64 = 1280.0;

/// What a tool call answers with.
#[derive(Debug, Default)]
pub(crate) struct ToolOutput {
    pub text: String,
    /// `data:` URLs (Codex refuses remote image URLs).
    pub images: Vec<String>,
    pub success: bool,
}

impl ToolOutput {
    fn ok(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            images: Vec::new(),
            success: true,
        }
    }

    pub(crate) fn fail(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            images: Vec::new(),
            success: false,
        }
    }

    /// The `item/tool/call` response body.
    pub(crate) fn to_response(&self) -> Value {
        let mut items = vec![json!({"type": "inputText", "text": self.text})];
        items.extend(
            self.images
                .iter()
                .map(|url| json!({"type": "inputImage", "imageUrl": url})),
        );
        json!({"contentItems": items, "success": self.success})
    }
}

fn tool(
    name: &str,
    description: &str,
    properties: Value,
    required: &[&str],
    deferred: bool,
) -> Value {
    let mut spec = json!({
        "type": "function",
        "name": name,
        "description": description,
        "inputSchema": {
            "type": "object",
            "properties": properties,
            "required": required,
            "additionalProperties": false,
        },
    });
    if deferred {
        spec["deferLoading"] = json!(true);
    }
    spec
}

/// The `dynamicTools` entry for `thread/start`.
pub(crate) fn specs() -> Value {
    let tab = json!({"type": "string", "description": "Tab id from `tabs`/`open`, e.g. t1."});
    let reference =
        json!({"type": "string", "description": "Element ref from a snapshot, e.g. e12."});
    let tools = vec![
        tool("tabs", "List the tabs this thread owns in the user's browser.", json!({}), &[], false),
        tool(
            "open",
            "Open a URL in a new tab in the user's browser (added to this thread's tab group). Returns the tab id and a snapshot.",
            json!({"url": {"type": "string"}}),
            &["url"],
            false,
        ),
        tool(
            "navigate",
            "Go to a URL, or back/forward/reload, in an owned tab. Returns a snapshot.",
            json!({"tab": tab, "url": {"type": "string", "description": "A URL, or back, forward or reload."}}),
            &["tab", "url"],
            false,
        ),
        tool(
            "snapshot",
            "Accessibility outline of the page; actionable elements carry refs (e12). `ref` limits it to one element's subtree.",
            json!({"tab": tab, "ref": reference}),
            &["tab"],
            false,
        ),
        tool(
            "click",
            "Click an element. Returns what changed in the snapshot.",
            json!({"tab": tab, "ref": reference, "double": {"type": "boolean"}}),
            &["tab", "ref"],
            false,
        ),
        tool(
            "type",
            "Replace the text in a field (clear=false to append), optionally pressing Enter after. Returns what changed.",
            json!({"tab": tab, "ref": reference, "text": {"type": "string"}, "submit": {"type": "boolean"}, "clear": {"type": "boolean"}}),
            &["tab", "ref", "text"],
            false,
        ),
        tool(
            "press",
            "Press a key or chord in the focused element: Enter, Escape, Tab, ArrowDown, Shift+Tab, Meta+a.",
            json!({"tab": tab, "key": {"type": "string"}}),
            &["tab", "key"],
            false,
        ),
        tool(
            "select",
            "Choose an option in a <select> by value or label.",
            json!({"tab": tab, "ref": reference, "value": {"type": "string"}}),
            &["tab", "ref", "value"],
            false,
        ),
        tool(
            "scroll",
            "Scroll an element into view, or the page by `pixels` (negative scrolls up).",
            json!({"tab": tab, "ref": reference, "pixels": {"type": "number"}}),
            &["tab"],
            false,
        ),
        tool(
            "wait_for",
            "Wait until text appears (or disappears with gone=true), a ref exists, or the URL contains a string.",
            json!({"tab": tab, "text": {"type": "string"}, "url": {"type": "string"}, "ref": reference, "gone": {"type": "boolean"}, "timeoutMs": {"type": "number"}}),
            &["tab"],
            false,
        ),
        tool(
            "read",
            "Visible text of the page or one element, from `offset` characters.",
            json!({"tab": tab, "ref": reference, "offset": {"type": "number"}}),
            &["tab"],
            false,
        ),
        tool(
            "evaluate",
            "Run a JavaScript expression in the page and return its JSON value (promises are awaited).",
            json!({"tab": tab, "expression": {"type": "string"}}),
            &["tab", "expression"],
            false,
        ),
        tool(
            "screenshot",
            "Screenshot the viewport, the full page, or one element.",
            json!({"tab": tab, "ref": reference, "fullPage": {"type": "boolean"}}),
            &["tab"],
            false,
        ),
        tool(
            "take",
            "Take over one of the user's own tabs (they approve it): the active tab, or the first whose title or URL contains `match`.",
            json!({"match": {"type": "string"}}),
            &[],
            false,
        ),
        tool(
            "release",
            "Hand an owned tab back to the user, leaving it open.",
            json!({"tab": tab}),
            &["tab"],
            false,
        ),
        tool("close", "Close an owned tab.", json!({"tab": tab}), &["tab"], false),
        tool(
            "console",
            "Console messages and exceptions logged in a tab since the last call.",
            json!({"tab": tab, "filter": {"type": "string"}}),
            &["tab"],
            true,
        ),
        tool(
            "network",
            "Finished network requests (status, method, URL, type) in a tab since the last call.",
            json!({"tab": tab, "filter": {"type": "string"}}),
            &["tab"],
            true,
        ),
        tool(
            "cdp",
            "Send any Chrome DevTools Protocol command, for what the other tools do not cover. `tab` is an owned tab, or \"browser\" for the browser target (needs full access). `session` is a child session from Target.attachedToTarget. Large results are written to a file.",
            json!({"tab": {"type": "string"}, "method": {"type": "string"}, "params": {"type": "object"}, "session": {"type": "string"}}),
            &["tab", "method"],
            true,
        ),
        tool(
            "cdp_events",
            "Drain CDP events received on an owned tab since the last call, filtered by method prefixes (e.g. Network.). Waits up to waitMs for the first one.",
            json!({"tab": tab, "methods": {"type": "array", "items": {"type": "string"}}, "waitMs": {"type": "number"}, "limit": {"type": "number"}}),
            &["tab"],
            true,
        ),
    ];
    json!([{
        "type": "namespace",
        "name": NAMESPACE,
        "description": "Drive the user's own Chrome (their profile and sign-ins). Work in tabs this thread owns: `open` one, or `take` the user's. Prefer snapshot refs and the high-level tools; `cdp`/`cdp_events` (raw DevTools Protocol), `console` and `network` are available through tool search. Visiting a new site asks the user first.",
        "tools": tools,
    }])
}

fn arg<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
}

fn required<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    arg(args, key).ok_or_else(|| format!("`{key}` is required"))
}

impl Browser {
    /// Run one tool call for `thread_id`.
    pub(crate) async fn call_tool(&self, thread_id: &str, tool: &str, args: &Value) -> ToolOutput {
        let conn = match self.connection().await {
            Ok(conn) => conn,
            Err(message) => return ToolOutput::fail(message),
        };
        match self.dispatch(&conn, thread_id, tool, args).await {
            Ok(output) => output,
            Err(message) => ToolOutput::fail(message),
        }
    }

    async fn dispatch(
        &self,
        conn: &std::sync::Arc<Conn>,
        thread: &str,
        tool: &str,
        args: &Value,
    ) -> Result<ToolOutput, String> {
        match tool {
            "tabs" => Ok(ToolOutput::ok(self.list_tabs(thread))),
            "open" => self.open(conn, thread, required(args, "url")?).await,
            "take" => self.take(conn, thread, arg(args, "match")).await,
            "cdp" => self.raw_cdp(conn, thread, args).await,
            _ => {
                let (owned, tab) = self.tab(conn, thread, required(args, "tab")?)?;
                self.on_tab(conn, thread, &owned, &tab, tool, args).await
            }
        }
    }

    async fn on_tab(
        &self,
        conn: &std::sync::Arc<Conn>,
        thread: &str,
        owned: &OwnedTab,
        tab: &Tab,
        tool: &str,
        args: &Value,
    ) -> Result<ToolOutput, String> {
        match tool {
            "navigate" => {
                self.navigate(conn, thread, tab, required(args, "url")?)
                    .await
            }
            "snapshot" => {
                let mut doc = tab.doc.lock().await;
                let scope = match arg(args, "ref") {
                    Some(reference) => Some(tab.backend_for(&doc, reference)?),
                    None => None,
                };
                let lines = tab.snapshot(conn, &mut doc, scope).await?;
                if scope.is_none() {
                    doc.last = lines.clone();
                }
                let (url, title) = tab.url_and_title(conn).await;
                Ok(ToolOutput::ok(format!(
                    "{title} — {url}\n{}",
                    outline(&lines)
                )))
            }
            "click" | "type" | "select" => {
                let mut doc = tab.doc.lock().await;
                let backend = tab.backend_for(&doc, required(args, "ref")?)?;
                let note = match tool {
                    "click" => {
                        let count = if args["double"].as_bool() == Some(true) {
                            2
                        } else {
                            1
                        };
                        match tab.center_of(conn, backend).await {
                            Ok((x, y)) => tab.click_at(conn, x, y, count).await?,
                            // No box (an <option>, an off-layout control):
                            // fall back to a DOM click.
                            Err(_) => {
                                tab.call_on(conn, backend, "function() { this.click(); }", vec![])
                                    .await?;
                            }
                        }
                        String::new()
                    }
                    "type" => {
                        let text = args["text"].as_str().unwrap_or_default();
                        tab.call(conn, "DOM.focus", json!({"backendNodeId": backend}))
                            .await?;
                        if args["clear"].as_bool() != Some(false) {
                            tab.call_on(conn, backend, SELECT_CONTENTS, vec![]).await?;
                        }
                        tab.call(conn, "Input.insertText", json!({"text": text}))
                            .await?;
                        if args["submit"].as_bool() == Some(true) {
                            tab.press(conn, "Enter").await?;
                        }
                        String::new()
                    }
                    _ => {
                        let value = required(args, "value")?;
                        let chosen = tab
                            .call_on(conn, backend, SELECT_OPTION, vec![json!(value)])
                            .await?;
                        match chosen.as_str() {
                            Some(label) => format!("Selected {label:?}.\n"),
                            None => return Err(format!("No option matches {value:?}.")),
                        }
                    }
                };
                tab.settle(conn, Duration::from_secs(5)).await;
                Ok(ToolOutput::ok(format!(
                    "{note}{}",
                    self.changes(conn, tab, &mut doc).await?
                )))
            }
            "press" => {
                let mut doc = tab.doc.lock().await;
                tab.press(conn, required(args, "key")?).await?;
                tab.settle(conn, Duration::from_secs(5)).await;
                Ok(ToolOutput::ok(self.changes(conn, tab, &mut doc).await?))
            }
            "scroll" => {
                let mut doc = tab.doc.lock().await;
                match arg(args, "ref") {
                    Some(reference) => {
                        let backend = tab.backend_for(&doc, reference)?;
                        tab.call(
                            conn,
                            "DOM.scrollIntoViewIfNeeded",
                            json!({"backendNodeId": backend}),
                        )
                        .await?;
                    }
                    None => {
                        let pixels = args["pixels"].as_f64().unwrap_or(600.0);
                        tab.evaluate(conn, &format!("window.scrollBy(0, {pixels})"))
                            .await?;
                    }
                }
                tokio::time::sleep(Duration::from_millis(300)).await;
                Ok(ToolOutput::ok(self.changes(conn, tab, &mut doc).await?))
            }
            "wait_for" => self.wait_for(conn, tab, args).await,
            "read" => {
                let doc = tab.doc.lock().await;
                let text = match arg(args, "ref") {
                    Some(reference) => {
                        let backend = tab.backend_for(&doc, reference)?;
                        tab.call_on(
                            conn,
                            backend,
                            "function() { return this.innerText ?? this.textContent ?? ''; }",
                            vec![],
                        )
                        .await?
                    }
                    None => {
                        tab.evaluate(conn, "document.body ? document.body.innerText : ''")
                            .await?
                    }
                };
                let text = text.as_str().unwrap_or_default();
                let offset = args["offset"].as_u64().unwrap_or(0) as usize;
                Ok(ToolOutput::ok(page_of(text, offset)))
            }
            "evaluate" => {
                let _doc = tab.doc.lock().await;
                let value = tab.evaluate(conn, required(args, "expression")?).await?;
                let text = match value {
                    Value::Null => "undefined".to_string(),
                    Value::String(text) => text,
                    other => other.to_string(),
                };
                Ok(ToolOutput::ok(clip(&text, TEXT_LIMIT)))
            }
            "screenshot" => {
                let doc = tab.doc.lock().await;
                let scope = match arg(args, "ref") {
                    Some(reference) => Some(tab.backend_for(&doc, reference)?),
                    None => None,
                };
                drop(doc);
                let (image, note) =
                    screenshot(conn, tab, scope, args["fullPage"].as_bool() == Some(true)).await?;
                Ok(ToolOutput {
                    text: format!("Screenshot of {}{note}", owned.id),
                    images: vec![image],
                    success: true,
                })
            }
            "release" => {
                self.release(conn, thread, &owned.target_id).await;
                Ok(ToolOutput::ok(format!(
                    "Released {}; it stays open for the user.",
                    owned.id
                )))
            }
            "close" => {
                conn.cdp
                    .call(
                        None,
                        "Target.closeTarget",
                        json!({"targetId": owned.target_id}),
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                self.forget_target(conn, &owned.target_id);
                Ok(ToolOutput::ok(format!("Closed {}.", owned.id)))
            }
            "console" | "network" => {
                let which = if tool == "console" {
                    Buffer::Console
                } else {
                    Buffer::Network
                };
                let (lines, skipped) = tab.drain(which, arg(args, "filter"), 150);
                let mut text = if lines.is_empty() {
                    format!("No new {tool} entries.")
                } else {
                    lines.join("\n")
                };
                if skipped > 0 {
                    text = format!("({skipped} older entries left out)\n{text}");
                }
                Ok(ToolOutput::ok(clip(&text, TEXT_LIMIT)))
            }
            "cdp_events" => Ok(cdp_events(tab, args).await),
            other => Err(format!("Unknown tool {other:?}.")),
        }
    }

    fn list_tabs(&self, thread: &str) -> String {
        let tabs = self.owned_tabs(thread);
        if tabs.is_empty() {
            return "This thread owns no tabs. `open` a URL, or `take` one of the user's tabs."
                .into();
        }
        tabs.iter()
            .map(|tab| {
                format!(
                    "{} {} — {}",
                    tab.id,
                    clip(&tab.title, 80),
                    clip(&tab.url, 160)
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    async fn open(
        &self,
        conn: &std::sync::Arc<Conn>,
        thread: &str,
        url: &str,
    ) -> Result<ToolOutput, String> {
        self.ensure_site(thread, url, "Codex wants to open this in a new tab.")
            .await?;
        let created = conn
            .cdp
            .call(
                None,
                "Target.createTarget",
                json!({"url": "about:blank", "background": true}),
            )
            .await
            .map_err(|error| error.to_string())?;
        let target_id = created["targetId"]
            .as_str()
            .ok_or("no target id")?
            .to_string();
        // Attach (and so start gating) before the first navigation.
        let owned = self.own(conn, thread, &target_id).await?;
        let (_, tab) = self.tab(conn, thread, &owned.id)?;
        let output = self.navigate(conn, thread, &tab, url).await?;
        Ok(ToolOutput {
            text: format!("Opened {}.\n{}", owned.id, output.text),
            ..output
        })
    }

    async fn navigate(
        &self,
        conn: &Conn,
        thread: &str,
        tab: &Tab,
        url: &str,
    ) -> Result<ToolOutput, String> {
        let mut doc = tab.doc.lock().await;
        match url.trim().to_ascii_lowercase().as_str() {
            "back" | "forward" => {
                let history = tab
                    .call(conn, "Page.getNavigationHistory", json!({}))
                    .await?;
                let index = history["currentIndex"].as_i64().unwrap_or(0)
                    + if url.trim().eq_ignore_ascii_case("back") {
                        -1
                    } else {
                        1
                    };
                let entry = history["entries"]
                    .as_array()
                    .and_then(|entries| entries.get(usize::try_from(index).ok()?))
                    .ok_or_else(|| format!("Nothing to go {url} to."))?;
                tab.call(
                    conn,
                    "Page.navigateToHistoryEntry",
                    json!({"entryId": entry["id"]}),
                )
                .await?;
            }
            "reload" => {
                tab.call(conn, "Page.reload", json!({})).await?;
            }
            _ => {
                self.ensure_site(thread, url, "Codex wants to navigate a tab here.")
                    .await?;
                let result = tab.call(conn, "Page.navigate", json!({"url": url})).await?;
                if let Some(error) = result["errorText"].as_str() {
                    return Err(
                        if error.contains("ABORTED") || error.contains("BLOCKED_BY_CLIENT") {
                            format!("Navigation to {url} was blocked: the user did not allow that site.")
                        } else {
                            format!("Navigation to {url} failed: {error}")
                        },
                    );
                }
            }
        }
        tab.settle(conn, Duration::from_secs(20)).await;
        let lines = tab.snapshot(conn, &mut doc, None).await?;
        doc.last = lines.clone();
        let (url, title) = tab.url_and_title(conn).await;
        Ok(ToolOutput::ok(format!(
            "{title} — {url}\n{}",
            outline(&lines)
        )))
    }

    /// After an action: the snapshot lines that changed, or the whole
    /// snapshot when most of it did (or the page navigated).
    async fn changes(
        &self,
        conn: &Conn,
        tab: &Tab,
        doc: &mut super::tab::Doc,
    ) -> Result<String, String> {
        let before = doc.last.clone();
        let lines = tab.snapshot(conn, doc, None).await?;
        doc.last = lines.clone();
        let (url, title) = tab.url_and_title(conn).await;
        let head = format!("{title} — {url}");
        if before.is_empty() {
            return Ok(format!("{head}\n{}", outline(&lines)));
        }
        let full = outline(&lines);
        Ok(match snapshot::diff(&before, &lines) {
            Some(changed) if changed.is_empty() => format!("{head}\nNo visible change."),
            // The diff only pays while it is clearly shorter than the page.
            Some(changed)
                if changed.iter().map(|line| line.len() + 1).sum::<usize>() * 4
                    < full.len() * 3 =>
            {
                format!("{head}\nChanged:\n{}", outline(&changed))
            }
            _ => format!("{head}\n{full}"),
        })
    }

    async fn wait_for(&self, conn: &Conn, tab: &Tab, args: &Value) -> Result<ToolOutput, String> {
        let timeout =
            Duration::from_millis(args["timeoutMs"].as_u64().unwrap_or(10_000).min(60_000));
        let gone = args["gone"].as_bool() == Some(true);
        let backend = match arg(args, "ref") {
            Some(reference) => Some(tab.backend_for(&*tab.doc.lock().await, reference)?),
            None => None,
        };
        let text = arg(args, "text");
        let url = arg(args, "url");
        if text.is_none() && url.is_none() && backend.is_none() {
            return Err("Give `text`, `url` or `ref` to wait for.".into());
        }
        let start = Instant::now();
        loop {
            let mut matched = true;
            if let Some(text) = text {
                let present = tab
                    .evaluate(
                        conn,
                        &format!(
                            "!!document.body && document.body.innerText.includes({})",
                            json!(text)
                        ),
                    )
                    .await
                    .ok()
                    == Some(Value::Bool(true));
                matched &= present != gone;
            }
            if let Some(url) = url {
                let (current, _) = tab.url_and_title(conn).await;
                matched &= current.contains(url) != gone;
            }
            if let Some(backend) = backend {
                let exists = tab
                    .call_on(
                        conn,
                        backend,
                        "function() { return this.isConnected; }",
                        vec![],
                    )
                    .await
                    .ok()
                    == Some(Value::Bool(true));
                matched &= exists != gone;
            }
            if matched {
                return Ok(ToolOutput::ok(format!(
                    "Matched after {} ms.",
                    start.elapsed().as_millis()
                )));
            }
            if start.elapsed() >= timeout {
                return Err(format!(
                    "Still not matched after {} ms.",
                    timeout.as_millis()
                ));
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }

    async fn take(
        &self,
        conn: &std::sync::Arc<Conn>,
        thread: &str,
        matching: Option<&str>,
    ) -> Result<ToolOutput, String> {
        let (target_id, title, url) = match matching {
            Some(needle) => {
                let needle = needle.to_lowercase();
                let targets = conn
                    .cdp
                    .call(None, "Target.getTargets", json!({}))
                    .await
                    .map_err(|error| error.to_string())?;
                let found = targets["targetInfos"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|info| info["type"].as_str() == Some("page"))
                    .filter(|info| {
                        self.owner_of(info["targetId"].as_str().unwrap_or_default())
                            .is_none()
                    })
                    .find(|info| {
                        let title = info["title"].as_str().unwrap_or_default().to_lowercase();
                        let url = info["url"].as_str().unwrap_or_default().to_lowercase();
                        title.contains(&needle) || url.contains(&needle)
                    })
                    .ok_or_else(|| format!("None of the user's tabs matches {matching:?}."))?;
                (
                    found["targetId"].as_str().unwrap_or_default().to_string(),
                    found["title"].as_str().unwrap_or_default().to_string(),
                    found["url"].as_str().unwrap_or_default().to_string(),
                )
            }
            None => {
                let (_, target, title, url) = helper::active_tab(conn).await.ok_or(
                    "Finding the user's active tab needs the Pingex Browser Helper extension; pass `match` (part of the tab's title or URL) instead.",
                )?;
                (target, title, url)
            }
        };
        if let Some(owner) = self.owner_of(&target_id) {
            if owner == thread {
                return Ok(ToolOutput::ok(self.list_tabs(thread)));
            }
            return Err("That tab already belongs to another Pingex thread.".into());
        }
        let decision = self
            .ask(BrowserApproval {
                id: String::new(),
                thread_id: thread.to_string(),
                kind: BrowserApprovalKind::TakeTab,
                site: policy::site_of(&url),
                url: Some(url.clone()),
                tab_title: Some(title.clone()),
                detail: "Codex wants to take over this tab.".into(),
            })
            .await;
        if decision == BrowserDecision::Deny {
            return Err("The user did not hand over that tab.".into());
        }
        let owned = self.share(conn, thread, &target_id).await?;
        let (_, tab) = self.tab(conn, thread, &owned.id)?;
        let mut doc = tab.doc.lock().await;
        let lines = tab.snapshot(conn, &mut doc, None).await?;
        doc.last = lines.clone();
        Ok(ToolOutput::ok(format!(
            "Took {}: {} — {}\n{}",
            owned.id,
            title,
            url,
            outline(&lines)
        )))
    }

    async fn raw_cdp(&self, conn: &Conn, thread: &str, args: &Value) -> Result<ToolOutput, String> {
        let method = required(args, "method")?;
        let params = args
            .get("params")
            .cloned()
            .filter(Value::is_object)
            .unwrap_or(json!({}));
        let tab_arg = required(args, "tab")?;
        let (target, session) = if tab_arg.eq_ignore_ascii_case("browser") {
            (CallTarget::Browser, None)
        } else {
            let (owned, tab) = self.tab(conn, thread, tab_arg)?;
            let session = match arg(args, "session") {
                Some(child) => {
                    let parent = conn.sessions.lock().unwrap().get(child).cloned();
                    match parent {
                        Some((target_id, false)) if target_id == owned.target_id => {
                            child.to_string()
                        }
                        _ => {
                            return Err(format!(
                                "Session {child} is not a child session of {}.",
                                owned.id
                            ))
                        }
                    }
                }
                None => tab.work.clone(),
            };
            (CallTarget::Tab, Some(session))
        };
        let full_access = self.full_access(thread);
        let granted = |site: &str| self.site_allowed(thread, site);
        match policy::check_raw(target, method, &params, full_access, &granted) {
            Verdict::Allow => {}
            Verdict::NeedsFullAccess(why) => self.ensure_full_access(thread, &why).await?,
            Verdict::NeedsApproval(what) => {
                let decision = self
                    .ask(BrowserApproval {
                        id: String::new(),
                        thread_id: thread.to_string(),
                        kind: BrowserApprovalKind::Upload,
                        site: None,
                        url: None,
                        tab_title: None,
                        detail: format!("Codex wants to {what}."),
                    })
                    .await;
                if decision == BrowserDecision::Deny {
                    return Err(format!("The user did not allow: {what}."));
                }
            }
        }
        let result = conn
            .cdp
            .call(session.as_deref(), method, params)
            .await
            .map_err(|error| format!("{method} failed: {error}"))?;
        // Screenshots come back as images rather than kilobytes of base64.
        if let (Some(data), true) = (
            result["data"].as_str(),
            matches!(method, "Page.captureScreenshot" | "Page.captureSnapshot"),
        ) {
            if method == "Page.captureScreenshot" {
                let format = args["params"]["format"].as_str().unwrap_or("png");
                return Ok(ToolOutput {
                    text: format!("{method} result:"),
                    images: vec![format!("data:image/{format};base64,{data}")],
                    success: true,
                });
            }
        }
        let text = serde_json::to_string(&result).unwrap_or_default();
        if text.len() <= TEXT_LIMIT {
            return Ok(ToolOutput::ok(text));
        }
        let path = spill(method, &text)?;
        Ok(ToolOutput::ok(format!(
            "{method} returned {} bytes, written to {path}. Start of it:\n{}",
            text.len(),
            clip(&text, 2000)
        )))
    }
}

async fn cdp_events(tab: &Tab, args: &Value) -> ToolOutput {
    let prefixes: Vec<String> = args["methods"]
        .as_array()
        .map(|methods| {
            methods
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let limit = args["limit"].as_u64().unwrap_or(100).clamp(1, 500) as usize;
    let wait = Duration::from_millis(args["waitMs"].as_u64().unwrap_or(0).min(30_000));
    let start = Instant::now();
    loop {
        let arrived = tab.raw_arrived.notified();
        let (events, last) = {
            let log = tab.log.lock().unwrap();
            let events: Vec<_> = log
                .raw
                .iter()
                .filter(|(seq, event)| {
                    *seq > log.raw_read
                        && (prefixes.is_empty()
                            || prefixes
                                .iter()
                                .any(|prefix| event.method.starts_with(prefix.as_str())))
                })
                .take(limit)
                .cloned()
                .collect();
            let last = events.last().map(|(seq, _)| *seq);
            (events, last)
        };
        if !events.is_empty() || start.elapsed() >= wait {
            if let Some(last) = last {
                tab.log.lock().unwrap().raw_read = last;
            }
            if events.is_empty() {
                return ToolOutput::ok("No new events.");
            }
            let lines: Vec<String> = events
                .iter()
                .map(|(_, event)| {
                    let mut line = json!({"method": event.method, "params": event.params});
                    if let Some(session) = &event.session_id {
                        line["sessionId"] = json!(session);
                    }
                    clip(&line.to_string(), 2000)
                })
                .collect();
            return ToolOutput::ok(clip(&lines.join("\n"), TEXT_LIMIT));
        }
        let remaining = wait.saturating_sub(start.elapsed());
        let _ = tokio::time::timeout(remaining, arrived).await;
    }
}

async fn screenshot(
    conn: &Conn,
    tab: &Tab,
    scope: Option<i64>,
    full_page: bool,
) -> Result<(String, String), String> {
    let metrics = tab.call(conn, "Page.getLayoutMetrics", json!({})).await?;
    let viewport = &metrics["cssVisualViewport"];
    let (x, y, width, height) = match scope {
        Some(backend) => {
            let _ = tab
                .call(
                    conn,
                    "DOM.scrollIntoViewIfNeeded",
                    json!({"backendNodeId": backend}),
                )
                .await;
            let model = tab
                .call(conn, "DOM.getBoxModel", json!({"backendNodeId": backend}))
                .await
                .map_err(|_| "That element has no box to capture.".to_string())?;
            let border: Vec<f64> = model["model"]["border"]
                .as_array()
                .map(|points| points.iter().filter_map(Value::as_f64).collect())
                .unwrap_or_default();
            if border.len() < 8 {
                return Err("That element has no box to capture.".into());
            }
            let xs = [border[0], border[2], border[4], border[6]];
            let ys = [border[1], border[3], border[5], border[7]];
            let min_x = xs.iter().cloned().fold(f64::MAX, f64::min);
            let min_y = ys.iter().cloned().fold(f64::MAX, f64::min);
            let page_x = viewport["pageX"].as_f64().unwrap_or(0.0);
            let page_y = viewport["pageY"].as_f64().unwrap_or(0.0);
            (
                min_x + page_x,
                min_y + page_y,
                xs.iter().cloned().fold(f64::MIN, f64::max) - min_x,
                ys.iter().cloned().fold(f64::MIN, f64::max) - min_y,
            )
        }
        None if full_page => {
            let content = &metrics["cssContentSize"];
            (
                0.0,
                0.0,
                content["width"].as_f64().unwrap_or(1280.0),
                content["height"].as_f64().unwrap_or(800.0).min(10_000.0),
            )
        }
        None => (
            viewport["pageX"].as_f64().unwrap_or(0.0),
            viewport["pageY"].as_f64().unwrap_or(0.0),
            viewport["clientWidth"].as_f64().unwrap_or(1280.0),
            viewport["clientHeight"].as_f64().unwrap_or(800.0),
        ),
    };
    // `clip.scale` multiplies CSS pixels, and the image comes out at the
    // device pixel ratio on top; divide it out so a Retina screen does not
    // send four times the pixels (and tokens).
    let ratio = tab
        .evaluate(conn, "window.devicePixelRatio")
        .await
        .ok()
        .and_then(|value| value.as_f64())
        .filter(|ratio| *ratio > 0.0)
        .unwrap_or(1.0);
    let scale = (SCREENSHOT_WIDTH / width.max(1.0)).min(1.0) / ratio;
    let params = json!({
        "format": "jpeg", "quality": 70,
        "clip": {"x": x, "y": y, "width": width.max(1.0), "height": height.max(1.0), "scale": scale},
        "captureBeyondViewport": full_page || scope.is_some(),
    });
    // A tab in the background may not paint until it is shown; bring it to
    // the front once rather than wait out the full CDP timeout.
    let attempt = tokio::time::timeout(
        Duration::from_secs(8),
        tab.call(conn, "Page.captureScreenshot", params.clone()),
    )
    .await;
    let (shot, note) = match attempt {
        Ok(result) => (result?, String::new()),
        Err(_) => {
            tab.call(conn, "Page.bringToFront", json!({})).await?;
            (
                tab.call(conn, "Page.captureScreenshot", params).await?,
                " (brought the tab to the front to capture it)".to_string(),
            )
        }
    };
    let data = shot["data"].as_str().ok_or("no screenshot data")?;
    // Validate it decodes, so a truncated payload fails here rather than at
    // the model.
    base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|error| format!("bad screenshot data: {error}"))?;
    Ok((format!("data:image/jpeg;base64,{data}"), note))
}

fn outline(lines: &[String]) -> String {
    if lines.is_empty() {
        return "(empty page)".into();
    }
    let mut text = String::new();
    for (index, line) in lines.iter().enumerate() {
        if index >= SNAPSHOT_LINES || text.len() + line.len() > TEXT_LIMIT {
            text.push_str(&format!(
                "… {} more lines; pass `ref` to snapshot one part, or `read` for text.",
                lines.len() - index
            ));
            break;
        }
        text.push_str(line);
        text.push('\n');
    }
    text.trim_end().to_string()
}

/// One page of `text` from `offset`, with a pointer to the next page.
fn page_of(text: &str, offset: usize) -> String {
    let total = text.chars().count();
    if offset >= total {
        return format!("(no text past offset {offset}; {total} characters in all)");
    }
    let page: String = text.chars().skip(offset).take(TEXT_LIMIT / 2).collect();
    let end = offset + page.chars().count();
    if end < total {
        format!(
            "{page}\n… {} more characters; read again with offset {end}.",
            total - end
        )
    } else {
        page
    }
}

fn spill(method: &str, text: &str) -> Result<String, String> {
    let dir = std::env::temp_dir().join("pingex-browser");
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let name = format!("{}-{}.json", method.replace('.', "-"), uuid::Uuid::new_v4());
    let path = dir.join(name);
    std::fs::write(&path, text).map_err(|error| error.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}

/// Select a field's contents so the inserted text replaces them.
const SELECT_CONTENTS: &str = "function() {
  if (typeof this.select === 'function') { this.select(); return; }
  const range = document.createRange(); range.selectNodeContents(this);
  const selection = getSelection(); selection.removeAllRanges(); selection.addRange(range);
}";

/// Choose a <select> option by value, then by visible label.
const SELECT_OPTION: &str = "function(wanted) {
  const options = Array.from(this.options || []);
  const option = options.find(o => o.value === wanted) || options.find(o => o.label.trim() === wanted.trim())
    || options.find(o => o.label.toLowerCase().includes(wanted.toLowerCase()));
  if (!option) return null;
  this.value = option.value;
  this.dispatchEvent(new Event('input', { bubbles: true }));
  this.dispatchEvent(new Event('change', { bubbles: true }));
  return option.label;
}";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specs_are_one_namespace_with_the_fallbacks_deferred() {
        let specs = specs();
        let namespace = &specs[0];
        assert_eq!(namespace["type"], "namespace");
        assert_eq!(namespace["name"], NAMESPACE);
        let tools = namespace["tools"].as_array().unwrap();
        let deferred: Vec<&str> = tools
            .iter()
            .filter(|tool| tool["deferLoading"] == json!(true))
            .map(|tool| tool["name"].as_str().unwrap())
            .collect();
        assert_eq!(deferred, ["console", "network", "cdp", "cdp_events"]);
        for tool in tools {
            assert_eq!(tool["type"], "function");
            assert_eq!(tool["inputSchema"]["type"], "object");
        }
        // The definitions are sent on every turn; keep the direct ones lean.
        let direct: usize = tools
            .iter()
            .filter(|tool| tool.get("deferLoading").is_none())
            .map(|tool| tool.to_string().len())
            .sum();
        assert!(direct < 9000, "direct tool specs grew to {direct} bytes");
    }

    #[test]
    fn responses_carry_text_then_images() {
        let output = ToolOutput {
            text: "hi".into(),
            images: vec!["data:image/jpeg;base64,AA==".into()],
            success: true,
        };
        assert_eq!(
            output.to_response(),
            json!({"contentItems": [
                {"type": "inputText", "text": "hi"},
                {"type": "inputImage", "imageUrl": "data:image/jpeg;base64,AA=="},
            ], "success": true})
        );
    }

    #[test]
    fn long_text_pages_with_a_cursor() {
        let text = "x".repeat(TEXT_LIMIT);
        let first = page_of(&text, 0);
        assert!(first.ends_with(&format!("read again with offset {}.", TEXT_LIMIT / 2)));
        assert_eq!(page_of(&text, TEXT_LIMIT / 2).len(), TEXT_LIMIT / 2);
        assert!(page_of("abc", 10).starts_with("(no text past offset 10"));
    }

    #[test]
    fn outlines_stop_at_the_line_budget() {
        let lines: Vec<String> = (0..SNAPSHOT_LINES + 5)
            .map(|i| format!("- text: {i}"))
            .collect();
        let text = outline(&lines);
        assert!(
            text.ends_with("… 5 more lines; pass `ref` to snapshot one part, or `read` for text.")
        );
    }
}
