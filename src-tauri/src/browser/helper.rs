//! The Pingex Browser Helper extension (`extensions/pingex-chrome`).
//!
//! CDP cannot create or read tab groups, so the helper does that part.
//! Pingex reaches it over the browser connection it already has: it attaches
//! to the helper's service worker as a CDP target and calls `pingex.*` with
//! `Runtime.evaluate`. Staying attached keeps the worker alive; the helper
//! reports the user's own group changes through the [`BINDING`] binding.
//!
//! Only the helper's fixed extension id (from the manifest `key`) is
//! trusted, so another extension cannot pose as it.

use serde_json::{json, Value};
use std::time::Duration;

use super::Conn;

/// Derived from the public `key` in `extensions/pingex-chrome/manifest.json`.
pub(crate) const HELPER_EXTENSION_ID: &str = "moebbciiianafkbmljnjplaggnhgpken";
/// The binding the helper calls to report changes.
pub(crate) const BINDING: &str = "pingexNotify";

/// Chrome's tab group colours, assigned per thread.
const COLORS: [&str; 8] = [
    "purple", "blue", "cyan", "green", "orange", "pink", "red", "yellow",
];

pub(crate) fn color_for(thread_id: &str) -> &'static str {
    let hash = thread_id.bytes().fold(0u32, |hash, byte| {
        hash.wrapping_mul(31).wrapping_add(byte as u32)
    });
    COLORS[hash as usize % COLORS.len()]
}

pub(crate) fn is_helper_target(info: &Value) -> bool {
    info["type"].as_str() == Some("service_worker")
        && info["url"].as_str().is_some_and(|url| {
            url.starts_with(&format!("chrome-extension://{HELPER_EXTENSION_ID}/"))
        })
}

/// Attach to the helper's worker and install the notification binding.
pub(crate) async fn attach(conn: &Conn, target_id: &str) -> Result<String, String> {
    let attached = conn
        .cdp
        .call(
            None,
            "Target.attachToTarget",
            json!({"targetId": target_id, "flatten": true}),
        )
        .await
        .map_err(|error| error.to_string())?;
    let session = attached["sessionId"]
        .as_str()
        .ok_or("no session id")?
        .to_string();
    // `Runtime.bindingCalled` is only delivered with the domain enabled.
    for (method, params) in [
        ("Runtime.enable", json!({})),
        ("Runtime.addBinding", json!({"name": BINDING})),
    ] {
        conn.cdp
            .call(Some(&session), method, params)
            .await
            .map_err(|error| error.to_string())?;
    }
    // The worker may be paused waiting for a debugger when it just started.
    let _ = conn
        .cdp
        .call(Some(&session), "Runtime.runIfWaitingForDebugger", json!({}))
        .await;
    evaluate(conn, &session, "pingex.hello()").await?;
    Ok(session)
}

fn session(conn: &Conn) -> Option<String> {
    conn.helper.lock().unwrap().clone()
}

/// Wait up to `timeout` for the helper to be attached; `None` when there is
/// no helper (not installed, or asleep and not woken in time).
pub(crate) async fn wait_for(conn: &Conn, timeout: Duration) -> Option<String> {
    if let Some(session) = session(conn) {
        return Some(session);
    }
    let appeared = conn.helper_appeared.notified();
    let _ = tokio::time::timeout(timeout, appeared).await;
    session(conn)
}

async fn evaluate(conn: &Conn, session: &str, expression: &str) -> Result<Value, String> {
    let result = conn
        .cdp
        .call(
            Some(session),
            "Runtime.evaluate",
            json!({"expression": expression, "awaitPromise": true, "returnByValue": true}),
        )
        .await
        .map_err(|error| error.to_string())?;
    if let Some(exception) = result.get("exceptionDetails") {
        let text = exception["exception"]["description"]
            .as_str()
            .or(exception["text"].as_str())
            .unwrap_or("helper call failed");
        return Err(text.to_string());
    }
    Ok(result["result"]["value"].clone())
}

async fn call(conn: &Conn, function: &str, args: &[Value]) -> Result<Value, String> {
    let session = session(conn).ok_or("the Pingex Browser Helper is not connected")?;
    let args = args
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    evaluate(conn, &session, &format!("pingex.{function}({args})")).await
}

pub(crate) async fn tab_id_for(conn: &Conn, target_id: &str) -> Option<i64> {
    let targets = call(conn, "targets", &[]).await.ok()?;
    targets
        .as_array()?
        .iter()
        .find(|target| target["targetId"].as_str() == Some(target_id))?["tabId"]
        .as_i64()
}

pub(crate) async fn target_for(conn: &Conn, tab_id: i64) -> Option<String> {
    let targets = call(conn, "targets", &[]).await.ok()?;
    targets
        .as_array()?
        .iter()
        .find(|target| target["tabId"].as_i64() == Some(tab_id))?["targetId"]
        .as_str()
        .map(str::to_string)
}

/// Group `tab_id` into `group` (or a new group), returning the group id.
pub(crate) async fn add_to_group(
    conn: &Conn,
    tab_id: i64,
    group: Option<i64>,
    title: &str,
    color: &str,
) -> Result<i64, String> {
    call(
        conn,
        "addToGroup",
        &[json!(tab_id), json!(group), json!(title), json!(color)],
    )
    .await?
    .as_i64()
    .ok_or_else(|| "the helper returned no group".to_string())
}

pub(crate) async fn ungroup(conn: &Conn, tab_id: i64) -> Result<(), String> {
    call(conn, "ungroup", &[json!(tab_id)]).await.map(|_| ())
}

pub(crate) async fn update_group(
    conn: &Conn,
    group: i64,
    title: &str,
    color: &str,
) -> Result<(), String> {
    call(
        conn,
        "updateGroup",
        &[json!(group), json!(title), json!(color)],
    )
    .await
    .map(|_| ())
}

/// The tab the user is looking at, as `(tab id, target id, title, url)`.
pub(crate) async fn active_tab(conn: &Conn) -> Option<(i64, String, String, String)> {
    let tab = call(conn, "activeTab", &[]).await.ok()?;
    let tab_id = tab["tabId"].as_i64()?;
    let target = target_for(conn, tab_id).await?;
    Some((
        tab_id,
        target,
        tab["title"].as_str().unwrap_or_default().to_string(),
        tab["url"].as_str().unwrap_or_default().to_string(),
    ))
}

pub(crate) async fn focus(conn: &Conn, tab_id: i64) -> Result<(), String> {
    call(conn, "focus", &[json!(tab_id)]).await.map(|_| ())
}

/// The helper's files, built into the app so it never depends on where the
/// source checkout is.
const EXTENSION_FILES: &[(&str, &str)] = &[
    (
        "manifest.json",
        include_str!("../../../extensions/pingex-chrome/manifest.json"),
    ),
    (
        "service-worker.js",
        include_str!("../../../extensions/pingex-chrome/service-worker.js"),
    ),
    (
        "README.md",
        include_str!("../../../extensions/pingex-chrome/README.md"),
    ),
];

/// Where Pingex installs the helper for "Load unpacked": its own app-data
/// folder (`~/Library/Application Support/pingex/browser-helper` on macOS).
pub(crate) fn extension_dir() -> std::path::PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("pingex")
        .join("browser-helper")
}

/// Write (or refresh) the helper into `dir`. Files already up to date are
/// left alone, so Chrome does not see a change it would reload for.
pub(crate) fn install_extension(dir: &std::path::Path) -> Result<(), String> {
    std::fs::create_dir_all(dir)
        .map_err(|error| format!("Could not create {}: {error}", dir.display()))?;
    for (name, contents) in EXTENSION_FILES {
        let path = dir.join(name);
        if std::fs::read_to_string(&path).ok().as_deref() == Some(*contents) {
            continue;
        }
        crate::util::fs::write_atomic(&path, contents.as_bytes())
            .map_err(|error| format!("Could not write {}: {error}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_helpers_own_worker_is_trusted() {
        let ours = json!({"type": "service_worker", "url": format!("chrome-extension://{HELPER_EXTENSION_ID}/service-worker.js")});
        assert!(is_helper_target(&ours));
        let impostor = json!({"type": "service_worker", "url": "chrome-extension://aaaabbbbccccddddeeeeffffgggghhhh/service-worker.js"});
        assert!(!is_helper_target(&impostor));
        let page = json!({"type": "page", "url": format!("chrome-extension://{HELPER_EXTENSION_ID}/x.html")});
        assert!(!is_helper_target(&page));
    }

    #[test]
    fn the_installed_helper_is_the_built_in_one() {
        let dir = tempfile::tempdir().unwrap();
        install_extension(dir.path()).unwrap();
        let manifest: Value = serde_json::from_str(
            &std::fs::read_to_string(dir.path().join("manifest.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            manifest["background"]["service_worker"],
            "service-worker.js"
        );
        // The id Pingex trusts is the one the manifest key produces.
        let key = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            manifest["key"].as_str().unwrap(),
        )
        .unwrap();
        assert_eq!(extension_id(&key), HELPER_EXTENSION_ID);
        // Installing again is a no-op.
        install_extension(dir.path()).unwrap();
    }

    /// Chrome's extension id: the first 16 bytes of SHA-256(public key),
    /// each nibble mapped to a..p.
    fn extension_id(public_key: &[u8]) -> String {
        use sha2::Digest;
        sha2::Sha256::digest(public_key)[..16]
            .iter()
            .flat_map(|byte| [byte >> 4, byte & 0xf])
            .map(|nibble| (b'a' + nibble) as char)
            .collect()
    }

    #[test]
    fn threads_keep_their_colour() {
        assert_eq!(color_for("thread-a"), color_for("thread-a"));
        assert!(COLORS.contains(&color_for("anything")));
    }
}
