//! Handing a path or URL to the desktop environment.
//!
//! Each of these shells out to the platform's opener. The URL command is
//! deliberately restrictive about schemes: it is reachable from the renderer, so
//! an unchecked scheme would turn a link into arbitrary command invocation.

use std::path::PathBuf;
use std::process::Command;

#[tauri::command]
#[specta::specta]
pub(crate) fn reveal_in_finder(path: String) -> Result<(), String> {
    let target = PathBuf::from(&path);
    if !target.exists() {
        return Err(format!("{path} no longer exists"));
    }
    let mut command = Command::new("open");
    if target.is_dir() {
        command.arg(&target);
    } else {
        command.arg("-R").arg(&target);
    }
    let status = command
        .status()
        .map_err(|error| format!("Could not open Finder: {error}"))?;
    if !status.success() {
        return Err(format!("Finder could not open {path}"));
    }
    Ok(())
}

/// Open a URL in the user's default browser (not inside the app webview).
#[tauri::command]
#[specta::specta]
pub(crate) fn open_external_url(url: String) -> Result<(), String> {
    // Only hand off web/mail links to the OS; refuse anything else so a
    // renderer bug can't turn this into arbitrary shell invocation.
    let allowed = ["http://", "https://", "mailto:"];
    if !allowed.iter().any(|scheme| url.starts_with(scheme)) {
        return Err(format!("Refusing to open unsupported URL: {url}"));
    }

    #[cfg(target_os = "macos")]
    let mut command = {
        let mut c = Command::new("open");
        c.arg(&url);
        c
    };
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut c = Command::new("cmd");
        c.args(["/C", "start", "", &url]);
        c
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut c = Command::new("xdg-open");
        c.arg(&url);
        c
    };

    let status = command
        .status()
        .map_err(|error| format!("Could not open link: {error}"))?;
    if !status.success() {
        return Err(format!("Could not open {url}"));
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub(crate) fn open_in_zed(path: String) -> Result<(), String> {
    let target = PathBuf::from(&path);
    if !target.exists() {
        return Err(format!("{path} no longer exists"));
    }
    let opened = Command::new("open")
        .args(["-a", "Zed"])
        .arg(&target)
        .status()
        .map(|status| status.success())
        .unwrap_or(false);
    if opened {
        return Ok(());
    }
    let fallback = Command::new("zed")
        .arg(&target)
        .status()
        .map_err(|error| format!("Could not open Zed: {error}"))?;
    if !fallback.success() {
        return Err(format!("Zed could not open {path}"));
    }
    Ok(())
}
