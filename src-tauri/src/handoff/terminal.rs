//! Handing a command to the OS: the clipboard and Terminal.app.
//!
//! The frontend never shells out; both of these run here so the quoting is done
//! once, in one place, and can be tested.

use std::io::Write;
use std::process::{Command, Stdio};

/// Quote a string as an AppleScript string literal.
pub(crate) fn applescript_quote(value: &str) -> String {
    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

/// Copy text to the system clipboard (macOS `pbcopy`).
pub(crate) fn copy_to_clipboard(text: &str) -> Result<(), String> {
    let mut child = Command::new("pbcopy")
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|error| format!("Could not copy to clipboard: {error}"))?;
    child
        .stdin
        .as_mut()
        .ok_or_else(|| "Clipboard is unavailable".to_string())?
        .write_all(text.as_bytes())
        .map_err(|error| format!("Could not copy to clipboard: {error}"))?;
    let status = child
        .wait()
        .map_err(|error| format!("Could not copy to clipboard: {error}"))?;
    if !status.success() {
        return Err("Clipboard write failed".to_string());
    }
    Ok(())
}

/// Open Terminal.app and run a command. Non-destructive: `codex resume` only
/// re-opens the thread.
pub(crate) fn launch_terminal(command: &str) -> Result<(), String> {
    let script = format!(
        "tell application \"Terminal\"\nactivate\ndo script {}\nend tell",
        applescript_quote(command)
    );
    let status = Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .status()
        .map_err(|error| format!("Could not open Terminal: {error}"))?;
    if !status.success() {
        return Err("Terminal could not run the command".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applescript_quote_escapes() {
        assert_eq!(applescript_quote("echo \"hi\""), "\"echo \\\"hi\\\"\"");
    }
}
