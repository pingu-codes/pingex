//! The handoff commands the frontend calls.

use tauri::State;

use super::terminal::{copy_to_clipboard, launch_terminal};
use super::url::{build_resume_command, build_thread_link};
use crate::AppState;

/// Build the reproducible `codex resume` command for the running home.
#[tauri::command]
pub(crate) fn handoff_command(
    thread_id: String,
    cwd: String,
    state: State<'_, AppState>,
) -> String {
    let runtime = state.runtime();
    build_resume_command(
        &runtime.codex_home.display().to_string(),
        &runtime.codex_binary.display().to_string(),
        &thread_id,
        &cwd,
    )
}

/// Build the shareable `codex://` link for the running home.
#[tauri::command]
pub(crate) fn handoff_thread_link(
    thread_id: String,
    cwd: String,
    label: Option<String>,
    state: State<'_, AppState>,
) -> String {
    let runtime = state.runtime();
    build_thread_link(
        &thread_id,
        &cwd,
        &runtime.codex_home.display().to_string(),
        label.as_deref(),
    )
}

/// Copy text to the system clipboard.
#[tauri::command]
pub(crate) fn handoff_copy(text: String) -> Result<(), String> {
    copy_to_clipboard(&text)
}

/// Open Terminal.app and run the handoff command.
#[tauri::command]
pub(crate) fn handoff_launch_terminal(command: String) -> Result<(), String> {
    launch_terminal(&command)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_resume_command_with_quoting() {
        let command = build_resume_command("/home/.codex", "codex", "abc-123", "/repo/wt");
        assert_eq!(
            command,
            "CODEX_HOME='/home/.codex' codex resume 'abc-123' --cd '/repo/wt'"
        );
    }
    #[test]
    fn resume_command_quotes_spaces_and_binary_path() {
        let command =
            build_resume_command("/home/my codex", "/usr/local/bin/codex", "id1", "/repo/a b");
        assert_eq!(
            command,
            "CODEX_HOME='/home/my codex' '/usr/local/bin/codex' resume 'id1' --cd '/repo/a b'"
        );
    }
    #[test]
    fn resume_command_escapes_single_quotes() {
        let command = build_resume_command("/h", "codex", "id", "/repo/o'brien");
        assert_eq!(
            command,
            "CODEX_HOME='/h' codex resume 'id' --cd '/repo/o'\\''brien'"
        );
    }
    #[test]
    fn thread_link_omits_empty_label() {
        let url = build_thread_link("id", "/repo", "/home", None);
        assert!(!url.contains("label="));
    }
}
