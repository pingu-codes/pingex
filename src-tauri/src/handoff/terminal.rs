//! Handing a command to the OS: the clipboard and a terminal window.
//!
//! The frontend never shells out; both of these run here so the quoting is done
//! once, in one place, and can be tested. Each platform has its own way in:
//! `pbcopy`/`osascript` on macOS, `clip.exe`/Windows Terminal on Windows, and
//! the usual clipboard tools plus `x-terminal-emulator` on Linux.

use std::io::Write;
use std::process::{Command, Stdio};

use crate::util::host::Host;

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

/// Pipe `text` into a clipboard tool's stdin.
fn pipe_to(program: &str, args: &[&str], text: &str) -> Result<(), String> {
    let mut child = Command::new(program)
        .args(args)
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

/// Copy text to the system clipboard.
pub(crate) fn copy_to_clipboard(text: &str) -> Result<(), String> {
    if cfg!(target_os = "macos") {
        return pipe_to("pbcopy", &[], text);
    }
    if cfg!(windows) {
        return pipe_to("clip.exe", &[], text);
    }
    // Linux: whichever clipboard tool the session has.
    let mut last = String::new();
    for (program, args) in [
        ("wl-copy", &[][..]),
        ("xclip", &["-selection", "clipboard"][..]),
        ("xsel", &["--clipboard", "--input"][..]),
    ] {
        match pipe_to(program, args, text) {
            Ok(()) => return Ok(()),
            Err(error) => last = error,
        }
    }
    Err(last)
}

/// The argv that opens a terminal running `command` for a home on `host`,
/// program first. Public for the unit tests; the platform decides the shape.
pub(crate) fn terminal_argv(host: &Host, command: &str, os: &str) -> Vec<String> {
    match (os, host) {
        ("macos", _) => vec![
            "osascript".into(),
            "-e".into(),
            format!(
                "tell application \"Terminal\"\nactivate\ndo script {}\nend tell",
                applescript_quote(command)
            ),
        ],
        // A WSL home's command already starts with `wsl.exe ...`; Windows
        // Terminal runs it as is. A native command runs in cmd.
        ("windows", Host::Wsl { .. }) => vec![
            "cmd".into(),
            "/C".into(),
            "start".into(),
            String::new(),
            "wt.exe".into(),
            "cmd".into(),
            "/K".into(),
            command.into(),
        ],
        ("windows", Host::Native) => vec![
            "cmd".into(),
            "/C".into(),
            "start".into(),
            String::new(),
            "cmd".into(),
            "/K".into(),
            command.into(),
        ],
        _ => vec![
            "x-terminal-emulator".into(),
            "-e".into(),
            "sh".into(),
            "-lc".into(),
            format!("{command}; exec $SHELL"),
        ],
    }
}

/// Open a terminal and run a command. Non-destructive: `codex resume` only
/// re-opens the thread.
pub(crate) fn launch_terminal(host: &Host, command: &str) -> Result<(), String> {
    let argv = terminal_argv(host, command, std::env::consts::OS);
    let status = Command::new(&argv[0])
        .args(&argv[1..])
        .status()
        .map_err(|error| format!("Could not open a terminal: {error}"))?;
    if !status.success() {
        return Err("The terminal could not run the command".to_string());
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

    #[test]
    fn terminal_argv_per_platform_and_host() {
        let mac = terminal_argv(&Host::Native, "codex resume x", "macos");
        assert_eq!(mac[0], "osascript");
        assert!(mac[2].contains("do script \"codex resume x\""));

        let windows = terminal_argv(
            &Host::wsl("Ubuntu"),
            "wsl.exe -d Ubuntu --exec sh -lc \"codex\"",
            "windows",
        );
        assert_eq!(&windows[..3], ["cmd", "/C", "start"]);
        assert!(windows.contains(&"wt.exe".to_string()));

        let linux = terminal_argv(&Host::Native, "codex resume x", "linux");
        assert_eq!(linux[0], "x-terminal-emulator");
        assert!(linux[4].starts_with("codex resume x"));
    }
}
