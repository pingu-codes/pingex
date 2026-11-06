//! The tools against a real headless Chrome for Testing, with the Pingex
//! Browser Helper loaded. Auto-connect needs a human click, so the browser
//! is started with a plain debug port on a throwaway profile instead; the
//! CDP traffic from there on is the same.
//!
//! Skipped (with a note) when no Chrome for Testing is found: set
//! `PINGEX_TEST_CHROME`, or `bunx playwright install chromium`.

use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::approvals::{BrowserApproval, BrowserApprovalKind, BrowserDecision};
use super::{Browser, BrowserStatus, Ui};

#[derive(Default)]
struct FakeUi {
    asked: Mutex<Vec<BrowserApproval>>,
    /// Kinds answered with Deny; everything else is allowed.
    deny: Mutex<Vec<BrowserApprovalKind>>,
}

#[async_trait::async_trait]
impl Ui for FakeUi {
    fn status_changed(&self, _status: &BrowserStatus) {}
    fn tabs_changed(&self, _thread_id: &str) {}
    async fn ask(&self, approval: BrowserApproval) -> BrowserDecision {
        let denied = self.deny.lock().unwrap().contains(&approval.kind);
        self.asked.lock().unwrap().push(approval);
        if denied {
            BrowserDecision::Deny
        } else {
            BrowserDecision::Allow
        }
    }
}

fn chrome_for_testing() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("PINGEX_TEST_CHROME") {
        return Some(PathBuf::from(path));
    }
    let cache = if cfg!(target_os = "macos") {
        dirs::home_dir()?.join("Library/Caches/ms-playwright")
    } else {
        dirs::cache_dir()?.join("ms-playwright")
    };
    let mut builds: Vec<PathBuf> = std::fs::read_dir(&cache)
        .ok()?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("chromium-"))
        })
        .collect();
    builds.sort();
    builds.into_iter().rev().find_map(|build| {
        [
            "chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing",
            "chrome-mac/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing",
            "chrome-linux64/chrome",
            "chrome-linux/chrome",
        ]
        .iter()
        .map(|relative| build.join(relative))
        .find(|path| path.exists())
    })
}

struct Chrome {
    process: std::process::Child,
    ws_url: String,
    _profile: tempfile::TempDir,
}

impl Drop for Chrome {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

async fn launch(binary: &Path) -> Chrome {
    let profile = tempfile::tempdir().unwrap();
    // The copy the app installs, not the source folder: that is what users load.
    let extension = profile.path().join("pingex-browser-helper");
    super::helper::install_extension(&extension).unwrap();
    let process = std::process::Command::new(binary)
        .args([
            "--headless=new".to_string(),
            format!("--user-data-dir={}", profile.path().display()),
            "--remote-debugging-port=0".to_string(),
            format!("--load-extension={}", extension.display()),
            format!("--disable-extensions-except={}", extension.display()),
            "--no-first-run".to_string(),
            // Never touch the macOS Keychain ("Chrome Safe Storage") from a
            // throwaway profile: it prompts the user on every launch.
            "--use-mock-keychain".to_string(),
            "--password-store=basic".to_string(),
            "--no-default-browser-check".to_string(),
            "--window-size=1280,900".to_string(),
            "about:blank".to_string(),
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("start chrome");
    let port_file = profile.path().join("DevToolsActivePort");
    for _ in 0..100 {
        if port_file.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let endpoint =
        super::attach::read_endpoint(super::attach::BrowserChannel::Chromium, profile.path())
            .expect("DevToolsActivePort");
    Chrome {
        process,
        ws_url: endpoint.ws_url(),
        _profile: profile,
    }
}

const FIXTURE: &str = r#"<!doctype html>
<title>Pingex fixture</title>
<h1>Greeter</h1>
<label>Name <input id="name"></label>
<select id="size" aria-label="Size"><option value="s">Small</option><option value="l">Large</option></select>
<button onclick="greet()">Greet</button>
<p id="out"></p>
<script>
function greet() {
  const text = 'Hello, ' + document.getElementById('name').value + ' (' + document.getElementById('size').value + ')';
  document.getElementById('out').textContent = text;
  console.log('greeted', text);
}
</script>"#;

fn text(output: &super::tools::ToolOutput) -> &str {
    assert!(output.success, "tool failed: {}", output.text);
    &output.text
}

/// The ref on the first snapshot line that contains `needle`.
fn ref_of(snapshot: &str, needle: &str) -> String {
    let line = snapshot
        .lines()
        .find(|line| line.contains(needle))
        .unwrap_or_else(|| panic!("no line with {needle:?} in:\n{snapshot}"));
    line.split_whitespace()
        .rev()
        .find(|word| word.starts_with('e') && word[1..].parse::<u32>().is_ok())
        .unwrap_or_else(|| panic!("no ref on {line:?}"))
        .to_string()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tools_drive_a_real_browser() {
    let Some(binary) = chrome_for_testing() else {
        eprintln!("skipping: no Chrome for Testing (set PINGEX_TEST_CHROME)");
        return;
    };
    let chrome = launch(&binary).await;
    let site = tempfile::tempdir().unwrap();
    let page = site.path().join("fixture.html");
    std::fs::write(&page, FIXTURE).unwrap();
    let url = format!("file://{}", page.display());

    let ui = Arc::new(FakeUi::default());
    let browser = Browser::with_endpoint(ui.clone(), chrome.ws_url.clone());
    let thread = "thread-1";
    let call = |tool: &'static str, args: Value| {
        let browser = browser.clone();
        async move { browser.call_tool(thread, tool, &args).await }
    };

    // Opening asks for the site once, then lands in the thread's group.
    let opened = call("open", json!({"url": url})).await;
    let snapshot = text(&opened).to_string();
    assert!(snapshot.starts_with("Opened t1."), "{snapshot}");
    assert!(snapshot.contains("heading \"Greeter\""), "{snapshot}");
    let asked: Vec<_> = ui
        .asked
        .lock()
        .unwrap()
        .iter()
        .map(|a| (a.kind.clone(), a.site.clone()))
        .collect();
    assert_eq!(
        asked,
        vec![(BrowserApprovalKind::Site, Some("file://".to_string()))]
    );
    let tab = browser.thread_view(thread).tabs[0].clone();
    assert_eq!(tab.id, "t1");
    assert!(
        tab.chrome_tab_id.is_some(),
        "the helper should have grouped the tab"
    );

    // A snapshot is far smaller than the raw accessibility tree it is drawn from.
    let raw = call(
        "cdp",
        json!({"tab": "t1", "method": "Accessibility.getFullAXTree"}),
    )
    .await;
    let raw_len = text(&raw).len();
    assert!(
        snapshot.len() * 5 < raw_len,
        "snapshot {} vs raw {raw_len}",
        snapshot.len()
    );

    // Type, choose, click: each answers with only what changed.
    let typed = call(
        "type",
        json!({"tab": "t1", "ref": ref_of(&snapshot, "textbox \"Name\""), "text": "Ada"}),
    )
    .await;
    assert!(text(&typed).contains("Ada"), "{}", typed.text);
    let chosen = call(
        "select",
        json!({"tab": "t1", "ref": ref_of(&snapshot, "combobox \"Size\""), "value": "Large"}),
    )
    .await;
    assert!(
        text(&chosen).starts_with("Selected \"Large\""),
        "{}",
        chosen.text
    );
    let clicked = call(
        "click",
        json!({"tab": "t1", "ref": ref_of(&snapshot, "button \"Greet\"")}),
    )
    .await;
    let clicked = text(&clicked);
    assert!(
        clicked.contains("Changed:") && clicked.contains("Hello, Ada (l)"),
        "{clicked}"
    );
    assert!(
        clicked.len() < snapshot.len(),
        "a diff should be smaller than the snapshot"
    );
    eprintln!(
        "result sizes: snapshot {} bytes, raw AX tree {raw_len} bytes, click diff {} bytes",
        snapshot.len(),
        clicked.len()
    );

    let waited = call("wait_for", json!({"tab": "t1", "text": "Hello, Ada"})).await;
    assert!(text(&waited).starts_with("Matched"));
    let console = call("console", json!({"tab": "t1"})).await;
    assert!(
        text(&console).contains("[log] greeted Hello, Ada (l)"),
        "{}",
        console.text
    );
    let read = call("read", json!({"tab": "t1"})).await;
    assert!(text(&read).contains("Hello, Ada (l)"));
    let evaluated = call(
        "evaluate",
        json!({"tab": "t1", "expression": "document.title"}),
    )
    .await;
    assert_eq!(text(&evaluated), "Pingex fixture");

    let shot = call("screenshot", json!({"tab": "t1"})).await;
    assert!(shot.success, "{}", shot.text);
    assert!(shot.images[0].starts_with("data:image/jpeg;base64,"));

    // Raw CDP passes through on an owned tab...
    let emulated = call(
        "cdp",
        json!({"tab": "t1", "method": "Emulation.setDeviceMetricsOverride", "params": {"width": 390, "height": 844, "deviceScaleFactor": 3, "mobile": false}}),
    )
    .await;
    assert_eq!(text(&emulated), "{}");
    let width = call("evaluate", json!({"tab": "t1", "expression": "innerWidth"})).await;
    assert_eq!(text(&width), "390");
    // ...and its events can be drained.
    call("cdp", json!({"tab": "t1", "method": "Page.reload"})).await;
    let events = call(
        "cdp_events",
        json!({"tab": "t1", "methods": ["Page.loadEventFired"], "waitMs": 5000}),
    )
    .await;
    assert!(
        text(&events).contains("Page.loadEventFired"),
        "{}",
        events.text
    );

    // Reaching beyond owned tabs needs full access, which this user denies.
    ui.deny
        .lock()
        .unwrap()
        .push(BrowserApprovalKind::FullAccess);
    let targets = call("cdp", json!({"tab": "t1", "method": "Target.getTargets"})).await;
    assert!(
        !targets.success && targets.text.contains("full browser access"),
        "{}",
        targets.text
    );
    let browser_wide = call(
        "cdp",
        json!({"tab": "browser", "method": "Browser.getVersion"}),
    )
    .await;
    assert!(!browser_wide.success);

    // Page JavaScript cannot leave the granted sites either: the gate holds
    // the navigation and the user is asked.
    ui.deny.lock().unwrap().push(BrowserApprovalKind::Site);
    call(
        "evaluate",
        json!({"tab": "t1", "expression": "location.href = 'http://127.0.0.1:9/blocked'"}),
    )
    .await;
    tokio::time::sleep(Duration::from_millis(800)).await;
    let still = call(
        "evaluate",
        json!({"tab": "t1", "expression": "location.href"}),
    )
    .await;
    assert!(
        text(&still).starts_with("file://"),
        "navigated away: {}",
        still.text
    );
    assert!(ui
        .asked
        .lock()
        .unwrap()
        .iter()
        .any(|a| a.kind == BrowserApprovalKind::Site && a.site.as_deref() == Some("127.0.0.1:9")));

    // A tab the thread does not own is invisible to it.
    let missing = call("snapshot", json!({"tab": "t9"})).await;
    assert!(!missing.success);

    // Releasing hands it back: still open, no longer the thread's.
    let released = call("release", json!({"tab": "t1"})).await;
    assert!(text(&released).starts_with("Released t1"));
    assert!(browser.thread_view(thread).tabs.is_empty());
    let listed = call("tabs", json!({})).await;
    assert!(text(&listed).starts_with("This thread owns no tabs"));

    // Taking it back by title asks the user, and refills the group.
    let taken = call("take", json!({"match": "Pingex fixture"})).await;
    assert!(text(&taken).starts_with("Took t2"), "{}", taken.text);
    assert!(ui
        .asked
        .lock()
        .unwrap()
        .iter()
        .any(|a| a.kind == BrowserApprovalKind::TakeTab));
    let closed = call("close", json!({"tab": "t2"})).await;
    assert!(text(&closed).starts_with("Closed t2"));
    drop(chrome);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn dragging_a_tab_out_of_the_group_releases_it() {
    let Some(binary) = chrome_for_testing() else {
        eprintln!("skipping: no Chrome for Testing (set PINGEX_TEST_CHROME)");
        return;
    };
    let chrome = launch(&binary).await;
    let ui = Arc::new(FakeUi::default());
    let browser = Browser::with_endpoint(ui.clone(), chrome.ws_url.clone());
    let opened = browser
        .call_tool(
            "thread-2",
            "open",
            &json!({"url": "data:text/html,<title>Drag me</title><p>hi"}),
        )
        .await;
    assert!(opened.success, "{}", opened.text);
    let tab = browser.thread_view("thread-2").tabs[0].clone();
    let chrome_tab = tab.chrome_tab_id.expect("grouped");
    // What the user does by hand: take the tab out of the group.
    let conn = browser.connection().await.unwrap();
    super::helper::ungroup(&conn, chrome_tab).await.unwrap();
    for _ in 0..40 {
        if browser.thread_view("thread-2").tabs.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(
        browser.thread_view("thread-2").tabs.is_empty(),
        "the tab should be released"
    );
    drop(chrome);
}

/// The same flow against the user's own running Chrome, found the way the
/// app finds it (`DevToolsActivePort`). Chrome shows its Allow dialog on
/// connect. Run by hand: `cargo test --lib real_chrome -- --ignored --nocapture`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "drives the user's real Chrome"]
async fn real_chrome_smoke() {
    let ui = Arc::new(FakeUi::default());
    let browser = Browser::new(ui.clone(), std::collections::BTreeSet::new());
    let started = std::time::Instant::now();
    let conn = browser
        .connection()
        .await
        .expect("connect to the user's Chrome");
    eprintln!(
        "connected in {:?}: {:?}",
        started.elapsed(),
        browser.status()
    );
    let helper = super::helper::wait_for(&conn, Duration::from_secs(3))
        .await
        .is_some();
    eprintln!("helper extension attached: {helper}");

    let site = tempfile::tempdir().unwrap();
    let page = site.path().join("pingex-smoke.html");
    std::fs::write(&page, FIXTURE).unwrap();
    let url = format!("file://{}", page.display());
    let thread = "real-chrome-smoke";
    browser.set_label(thread, "smoke test");
    let call = |tool: &'static str, args: Value| {
        let browser = browser.clone();
        async move { browser.call_tool(thread, tool, &args).await }
    };

    let opened = call("open", json!({"url": url})).await;
    eprintln!("--- open ({} bytes)\n{}", opened.text.len(), opened.text);
    let snapshot = text(&opened).to_string();
    let typed = call(
        "type",
        json!({"tab": "t1", "ref": ref_of(&snapshot, "textbox \"Name\""), "text": "Ciaran"}),
    )
    .await;
    eprintln!("--- type\n{}", text(&typed));
    let clicked = call(
        "click",
        json!({"tab": "t1", "ref": ref_of(&snapshot, "button \"Greet\"")}),
    )
    .await;
    eprintln!(
        "--- click ({} bytes)\n{}",
        clicked.text.len(),
        text(&clicked)
    );
    assert!(clicked.text.contains("Hello, Ciaran"));
    let shot = call("screenshot", json!({"tab": "t1"})).await;
    eprintln!(
        "--- screenshot: {} ({} KB)",
        shot.text,
        shot.images.first().map(|i| i.len() / 1024).unwrap_or(0)
    );
    assert!(shot.success, "{}", shot.text);
    std::fs::write(
        std::env::temp_dir().join("pingex-smoke.jpg"),
        base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            shot.images[0].trim_start_matches("data:image/jpeg;base64,"),
        )
        .unwrap(),
    )
    .unwrap();
    let tabs = browser.thread_view(thread).tabs;
    eprintln!("--- owned tabs: {tabs:?}");
    if helper {
        assert!(
            tabs[0].chrome_tab_id.is_some(),
            "the helper should have grouped the tab"
        );
    }
    // Leave it on screen long enough to see the group.
    tokio::time::sleep(Duration::from_secs(6)).await;
    let closed = call("close", json!({"tab": "t1"})).await;
    eprintln!("--- {}", text(&closed));
}
