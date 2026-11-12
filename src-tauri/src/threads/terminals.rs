//! Background terminals: commands Codex started that outlive the step (or the
//! whole turn) that ran them — a dev server, a watcher. Codex's unified exec
//! keeps them alive across turn end and Stop; only `clean`, `terminate` or the
//! process exiting ends one, and its `item/completed` arrives only then.
//!
//! The API is experimental (`thread/backgroundTerminals/*`) and only answers for
//! a thread loaded in this app-server, so these commands never resume a thread
//! just to ask: one that is not loaded has nothing running.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;
use tauri::{AppHandle, State};

use crate::codex::requests;
use crate::harness::driver::unsupported;
use crate::util::json::str_at;
use crate::AppState;

/// One running background terminal. Codex also defines `osPid`, `cpuPercent`
/// and `rssKb`, but no release fills them in yet, so they are not carried.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BackgroundTerminal {
    /// The `commandExecution` item that started it.
    pub item_id: String,
    /// What `terminate` takes.
    pub process_id: String,
    pub command: String,
    pub cwd: String,
}

/// Codex answers `thread not found` for a thread it has not loaded.
fn not_loaded(error: &str) -> bool {
    error.contains("thread not found")
}

/// One page of a list response: its terminals and the cursor for the next.
fn parse_page(response: &Value) -> (Vec<BackgroundTerminal>, Option<String>) {
    let terminals = response
        .get("data")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(|row| serde_json::from_value(row.clone()).ok())
                .collect()
        })
        .unwrap_or_default();
    let next = str_at(response, "nextCursor").map(str::to_string);
    (terminals, next)
}

/// Upper bound on pages followed, against a cursor that never ends.
const MAX_PAGES: usize = 20;

#[tauri::command]
#[specta::specta]
pub(crate) async fn background_terminals_list(
    thread_id: String,
    app: AppHandle,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<Vec<BackgroundTerminal>, String> {
    let ctx = state.ctx(&window);
    if ctx.harness_thread(&thread_id).await?.is_some() {
        return Ok(Vec::new());
    }
    let mut all = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..MAX_PAGES {
        let request = requests::background_terminals_list(&thread_id, cursor.as_deref());
        let response = match ctx.session.send(&app, request).await {
            Ok(response) => response,
            Err(error) if not_loaded(&error) => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };
        let (page, next) = parse_page(&response);
        all.extend(page);
        match next {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }
    Ok(all)
}

/// Stop one background terminal. `false` means it had already gone.
#[tauri::command]
#[specta::specta]
pub(crate) async fn background_terminal_terminate(
    thread_id: String,
    process_id: String,
    app: AppHandle,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    let ctx = state.ctx(&window);
    if let Some((_, driver)) = ctx.harness_thread(&thread_id).await? {
        return Err(unsupported(driver.kind(), "background terminals"));
    }
    let request = requests::background_terminal_terminate(&thread_id, &process_id);
    match ctx.session.send(&app, request).await {
        Ok(response) => Ok(response
            .get("terminated")
            .and_then(Value::as_bool)
            .unwrap_or(false)),
        Err(error) if not_loaded(&error) => Ok(false),
        Err(error) => Err(error),
    }
}

/// Stop every background terminal of a thread.
#[tauri::command]
#[specta::specta]
pub(crate) async fn background_terminals_clean(
    thread_id: String,
    app: AppHandle,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let ctx = state.ctx(&window);
    if let Some((_, driver)) = ctx.harness_thread(&thread_id).await? {
        return Err(unsupported(driver.kind(), "background terminals"));
    }
    let request = requests::background_terminals_clean(&thread_id);
    match ctx.session.send(&app, request).await {
        Ok(_) => Ok(()),
        Err(error) if not_loaded(&error) => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_page_keeps_the_fields_the_app_shows() {
        let response = json!({
            "data": [{
                "itemId": "call_1",
                "processId": "1000",
                "command": "bun run dev",
                "cwd": "/repo",
                "osPid": null,
                "cpuPercent": null,
                "rssKb": null
            }],
            "nextCursor": "1"
        });
        let (page, next) = parse_page(&response);
        assert_eq!(
            page,
            vec![BackgroundTerminal {
                item_id: "call_1".into(),
                process_id: "1000".into(),
                command: "bun run dev".into(),
                cwd: "/repo".into(),
            }]
        );
        assert_eq!(next.as_deref(), Some("1"));
    }

    #[test]
    fn an_empty_or_last_page_has_no_cursor() {
        let (page, next) = parse_page(&json!({"data": [], "nextCursor": null}));
        assert!(page.is_empty());
        assert!(next.is_none());
    }

    #[test]
    fn a_thread_codex_has_not_loaded_has_nothing_running() {
        assert!(not_loaded(
            r#"Codex request failed: {"code":-32600,"message":"thread not found: 019a"}"#
        ));
        assert!(!not_loaded("Codex is not running"));
    }
}
