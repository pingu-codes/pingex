use serde_json::json;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use crate::settings::prefs as settings;
use crate::AppState;

/// Window label for the lightweight quick-chat composer.
const QUICK_LABEL: &str = "quick";
const MAIN_LABEL: &str = "main";

/// Create the quick window on demand. It points at the same frontend bundle
/// with `?window=quick`, which `main.ts` branches on to mount `QuickChat`
/// instead of the full app. Hidden until the shortcut shows it.
fn ensure_quick_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    if let Some(window) = app.get_webview_window(QUICK_LABEL) {
        return Ok(window);
    }
    let window = WebviewWindowBuilder::new(
        app,
        QUICK_LABEL,
        WebviewUrl::App("index.html?window=quick".into()),
    )
    .title("Quick Chat")
    .inner_size(560.0, 260.0)
    .min_inner_size(420.0, 180.0)
    .resizable(true)
    .decorations(false)
    .always_on_top(true)
    .center()
    .skip_taskbar(true)
    .visible(false)
    .build()
    .map_err(|error| format!("Could not create quick window: {error}"))?;

    // Spotlight-style: dismiss when the panel loses focus so it never lingers
    // over the app the user tabbed back to.
    let dismiss = window.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::Focused(false) = event {
            let _ = dismiss.hide();
        }
    });
    Ok(window)
}

/// Toggle the quick window: show+focus when hidden, hide when visible.
pub(crate) fn toggle_quick_window(app: &AppHandle) -> Result<(), String> {
    let window = ensure_quick_window(app)?;
    if window.is_visible().unwrap_or(false) {
        window.hide().map_err(|error| error.to_string())?;
    } else {
        let _ = window.center();
        window.show().map_err(|error| error.to_string())?;
        window.set_focus().map_err(|error| error.to_string())?;
        // Let the frontend prefetch/refocus when it becomes visible.
        let _ = app.emit_to(QUICK_LABEL, "quickchat://shown", ());
    }
    Ok(())
}

/// Register (or re-register) the global shortcut that toggles the quick window.
/// Any previously registered app shortcut is cleared first so changing the
/// accelerator never leaves a stale binding behind.
pub(crate) fn register_quick_shortcut(app: &AppHandle, accelerator: &str) -> Result<(), String> {
    let shortcut: Shortcut = accelerator
        .parse()
        .map_err(|error| format!("Invalid shortcut '{accelerator}': {error}"))?;
    let global = app.global_shortcut();
    let _ = global.unregister_all();
    global
        .on_shortcut(shortcut, move |app, _shortcut, event| {
            if event.state() == ShortcutState::Pressed {
                let _ = toggle_quick_window(app);
            }
        })
        .map_err(|error| format!("Could not register '{accelerator}': {error}"))
}

/// Register the persisted (or default) shortcut at startup. Best-effort: a
/// boot-time conflict must not prevent the app from launching.
pub(crate) fn register_saved_shortcut(app: &AppHandle) {
    let accelerator = settings::read_quick_shortcut(&settings::settings_path());
    if let Err(error) = register_quick_shortcut(app, &accelerator) {
        eprintln!("quick-chat shortcut not registered: {error}");
    }
}

#[tauri::command]
#[specta::specta]
pub(crate) fn get_quick_shortcut() -> String {
    settings::read_quick_shortcut(&settings::settings_path())
}

#[tauri::command]
#[specta::specta]
pub(crate) fn set_quick_shortcut(app: AppHandle, accelerator: String) -> Result<String, String> {
    let trimmed = accelerator.trim().to_string();
    if trimmed.is_empty() {
        return Err("Shortcut cannot be empty".to_string());
    }
    // Try the new binding; on failure (invalid or already taken) restore the
    // previously working shortcut so the user is never left with none.
    register_quick_shortcut(&app, &trimmed).map_err(|error| {
        let previous = settings::read_quick_shortcut(&settings::settings_path());
        let _ = register_quick_shortcut(&app, &previous);
        error
    })?;
    settings::write_quick_shortcut(&settings::settings_path(), Some(trimmed.clone()))?;
    Ok(trimmed)
}

/// Hand the quick-window thread back to the full app: focus a window bound to
/// the quick chat's home (the default context) and emit a navigation event.
#[tauri::command]
#[specta::specta]
pub(crate) fn quick_open_full_thread(app: AppHandle, thread_id: String) -> Result<(), String> {
    let state = app.state::<AppState>();
    let home_key = state.default_home();
    // The main window drives the default home, so it is normally the target;
    // fall back to any other window bound to the quick chat's home.
    let label = if app.get_webview_window(MAIN_LABEL).is_some() {
        MAIN_LABEL.to_string()
    } else {
        state
            .window_bindings()
            .into_iter()
            .find(|(label, key)| key == &home_key && label != QUICK_LABEL)
            .map(|(label, _)| label)
            .unwrap_or_else(|| MAIN_LABEL.to_string())
    };
    if let Some(target) = app.get_webview_window(&label) {
        let _ = target.unminimize();
        target.show().map_err(|error| error.to_string())?;
        target.set_focus().map_err(|error| error.to_string())?;
    }
    app.emit(
        "quickchat://open-thread",
        json!({ "threadId": thread_id, "codexHome": home_key }),
    )
    .map_err(|error| error.to_string())?;
    if let Some(quick) = app.get_webview_window(QUICK_LABEL) {
        let _ = quick.hide();
    }
    Ok(())
}
