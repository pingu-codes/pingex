use std::fs;
use std::path::{Path, PathBuf};
use tauri::State;

use crate::AppState;

/// Every project gets its own nested folder under `<codex_home>/drafts/`,
/// holding that project's in-progress composer message as `draft.json`.
/// The folder name is a readable slug of the project path plus an FNV-1a
/// hash so distinct paths can never collide after slugging.
fn draft_folder(codex_home: &Path, project: &str) -> PathBuf {
    let slug: String = project
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect();
    let slug = slug.trim_matches('-');
    let tail_start = slug.len().saturating_sub(60);
    let tail = &slug[tail_start..];
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in project.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    codex_home
        .join("drafts")
        .join(format!("{tail}-{hash:016x}"))
}

fn draft_path(codex_home: &Path, project: &str) -> PathBuf {
    draft_folder(codex_home, project).join("draft.json")
}

fn write_draft(codex_home: &Path, project: &str, content: &str) -> Result<(), String> {
    let folder = draft_folder(codex_home, project);
    fs::create_dir_all(&folder)
        .map_err(|error| format!("Could not create draft folder: {error}"))?;
    fs::write(folder.join("draft.json"), content)
        .map_err(|error| format!("Could not save draft: {error}"))
}

fn read_draft(codex_home: &Path, project: &str) -> Result<Option<String>, String> {
    match fs::read_to_string(draft_path(codex_home, project)) {
        Ok(content) => Ok(Some(content)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("Could not read draft: {error}")),
    }
}

fn remove_draft(codex_home: &Path, project: &str) -> Result<(), String> {
    let path = draft_path(codex_home, project);
    match fs::remove_file(&path) {
        Ok(()) => {
            // Tidy the now-empty per-project folder; a failure is harmless.
            if let Some(folder) = path.parent() {
                let _ = fs::remove_dir(folder);
            }
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("Could not delete draft: {error}")),
    }
}

#[tauri::command]
pub(crate) fn save_draft(
    project: String,
    content: String,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let ctx = state.ctx(&window);
    write_draft(&ctx.runtime().codex_home, &project, &content)
}

#[tauri::command]
pub(crate) fn load_draft(
    project: String,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    let ctx = state.ctx(&window);
    read_draft(&ctx.runtime().codex_home, &project)
}

#[tauri::command]
pub(crate) fn delete_draft(
    project: String,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let ctx = state.ctx(&window);
    remove_draft(&ctx.runtime().codex_home, &project)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distinct_projects_get_distinct_nested_folders() {
        let home = Path::new("/home");
        let one = draft_folder(home, "/tmp/project one");
        let two = draft_folder(home, "/tmp/project-one");
        assert_ne!(one, two);
        assert!(one.starts_with("/home/drafts"));
        assert!(one
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("tmp-project-one-"));
    }

    #[test]
    fn drafts_round_trip_and_delete() {
        let directory = tempfile::tempdir().unwrap();
        let home = directory.path();
        assert_eq!(read_draft(home, "/tmp/project").unwrap(), None);

        write_draft(home, "/tmp/project", r#"[{"type":"text","text":"hi"}]"#).unwrap();
        assert_eq!(
            read_draft(home, "/tmp/project").unwrap().as_deref(),
            Some(r#"[{"type":"text","text":"hi"}]"#)
        );
        assert!(draft_folder(home, "/tmp/project").is_dir());

        remove_draft(home, "/tmp/project").unwrap();
        assert_eq!(read_draft(home, "/tmp/project").unwrap(), None);
        assert!(!draft_folder(home, "/tmp/project").exists());
        // Deleting an absent draft is a no-op, not an error.
        remove_draft(home, "/tmp/project").unwrap();
    }
}
