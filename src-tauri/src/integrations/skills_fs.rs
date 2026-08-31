//! Skill files on disk.
//!
//! Codex resolves skills (`skills/list`) but has no API to read, scaffold, or
//! remove a skill, so those are plain filesystem operations here. Writes are
//! confined to `<codex_home>/skills/` — the user scope — so a system or
//! plugin-provided skill can never be deleted from the UI. After any change we
//! ask Codex for a forced rescan so the returned list reflects disk.

use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, State};

use super::commands::build_list_with;
use super::IntegrationsList;
use crate::AppState;

const SKILL_FILE: &str = "SKILL.md";

/// Accept a skill directory or its `SKILL.md` and return the file path.
fn skill_file(path: &str) -> Result<PathBuf, String> {
    let target = PathBuf::from(path);
    let file = if target.is_dir() {
        target.join(SKILL_FILE)
    } else {
        target
    };
    if file.file_name().and_then(|n| n.to_str()) != Some(SKILL_FILE) {
        return Err(format!("{path} is not a SKILL.md"));
    }
    if !file.is_file() {
        return Err(format!("{} does not exist", file.display()));
    }
    Ok(file)
}

/// Skill names become directory names and are referenced from prompts, so keep
/// them to the safe, lowercase, hyphenated form Codex's own examples use.
pub fn validate_skill_name(name: &str) -> Result<(), String> {
    let ok = !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
        && !name.starts_with(['-', '_']);
    if ok {
        Ok(())
    } else {
        Err("Skill name must be lowercase letters, digits, '-' or '_' (not leading).".into())
    }
}

fn skills_root(codex_home: &Path) -> PathBuf {
    codex_home.join("skills")
}

/// Render the SKILL.md we scaffold. Frontmatter matches what Codex parses.
pub fn render_skill_md(name: &str, description: &str, body: Option<&str>) -> String {
    let description = description.trim().replace('\n', " ");
    let body = body
        .map(str::trim)
        .filter(|b| !b.is_empty())
        .unwrap_or("## Instructions\n\nDescribe what Codex should do when this skill is used.");
    format!("---\nname: {name}\ndescription: {description}\n---\n\n{body}\n")
}

/// Create `<codex_home>/skills/<name>/SKILL.md`. Errors if anything exists
/// there already — we never overwrite a skill the user wrote.
pub fn create_skill_on_disk(
    codex_home: &Path,
    name: &str,
    description: &str,
    body: Option<&str>,
) -> Result<PathBuf, String> {
    validate_skill_name(name)?;
    if description.trim().is_empty() {
        return Err("Description is required.".into());
    }
    let dir = skills_root(codex_home).join(name);
    if dir.exists() {
        return Err(format!("A skill named {name} already exists."));
    }
    fs::create_dir_all(&dir).map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
    let file = dir.join(SKILL_FILE);
    fs::write(&file, render_skill_md(name, description, body))
        .map_err(|e| format!("Could not write {}: {e}", file.display()))?;
    Ok(file)
}

/// Delete a skill directory, refusing anything outside the user skills root.
pub fn delete_skill_on_disk(codex_home: &Path, path: &str) -> Result<(), String> {
    let file = skill_file(path)?;
    let dir = file
        .parent()
        .ok_or_else(|| "Skill has no parent directory".to_string())?;
    let root = skills_root(codex_home);
    let canonical_dir = dir
        .canonicalize()
        .map_err(|e| format!("Could not resolve {}: {e}", dir.display()))?;
    let canonical_root = root
        .canonicalize()
        .map_err(|_| "Only skills under your Codex home can be deleted.".to_string())?;
    // Must be a direct child of the root: refuse the root itself and refuse
    // anything nested elsewhere (system skills, plugin skills, symlink escapes).
    if canonical_dir.parent() != Some(canonical_root.as_path()) {
        return Err("Only skills under your Codex home can be deleted.".into());
    }
    fs::remove_dir_all(&canonical_dir)
        .map_err(|e| format!("Could not delete {}: {e}", canonical_dir.display()))
}

#[tauri::command]
#[specta::specta]
pub(crate) fn read_skill(path: String) -> Result<String, String> {
    let file = skill_file(&path)?;
    fs::read_to_string(&file).map_err(|e| format!("Could not read {}: {e}", file.display()))
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn create_skill(
    name: String,
    description: String,
    body: Option<String>,
    cwds: Option<Vec<String>>,
    app: AppHandle,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<IntegrationsList, String> {
    let ctx = state.ctx(&window);
    let home = ctx.runtime().codex_home;
    create_skill_on_disk(&home, name.trim(), &description, body.as_deref())?;
    build_list_with(&app, &ctx, cwds.unwrap_or_default(), true).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn delete_skill(
    path: String,
    cwds: Option<Vec<String>>,
    app: AppHandle,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<IntegrationsList, String> {
    let ctx = state.ctx(&window);
    let home = ctx.runtime().codex_home;
    delete_skill_on_disk(&home, &path)?;
    build_list_with(&app, &ctx, cwds.unwrap_or_default(), true).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_names() {
        assert!(validate_skill_name("my-skill_2").is_ok());
        for bad in ["", "My", "-x", "a b", "a/b", "a:b"] {
            assert!(validate_skill_name(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn creates_reads_and_deletes_under_home() {
        let home = tempfile::tempdir().unwrap();
        let file = create_skill_on_disk(home.path(), "demo", "Does demo things", None).unwrap();
        let text = read_skill(file.display().to_string()).unwrap();
        assert!(text.starts_with("---\nname: demo\ndescription: Does demo things\n---"));
        // Reading the directory works too.
        assert_eq!(
            read_skill(file.parent().unwrap().display().to_string()).unwrap(),
            text
        );
        assert!(create_skill_on_disk(home.path(), "demo", "again", None).is_err());
        delete_skill_on_disk(home.path(), &file.display().to_string()).unwrap();
        assert!(!file.exists());
    }

    #[test]
    fn refuses_to_delete_outside_skills_root() {
        let home = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let dir = other.path().join("sys");
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join(SKILL_FILE);
        fs::write(&file, "---\nname: sys\n---\n").unwrap();
        assert!(delete_skill_on_disk(home.path(), &file.display().to_string()).is_err());
        assert!(file.exists());
        // Not a SKILL.md at all.
        let stray = other.path().join("notes.md");
        fs::write(&stray, "x").unwrap();
        assert!(delete_skill_on_disk(home.path(), &stray.display().to_string()).is_err());
    }
}
