//! Claude Code auto-memory: one directory per project at
//! `<config>/projects/<slug>/memory`, each with a `MEMORY.md` index linking
//! its files, switched by `autoMemoryEnabled` in `<config>/settings.json`.

use std::fs;
use std::path::{Path, PathBuf};

use super::{files, MemoryStore, MemoryToggle};
use crate::util::host::Host;

const INDEX: &str = "MEMORY.md";
const SETTING: &str = "autoMemoryEnabled";

/// Claude names a project's directory after its path with every character
/// that is not ASCII alphanumeric replaced by `-`.
pub(super) fn slug(path: &str) -> String {
    path.chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect()
}

/// Every project memory directory, as `(host root, local root, slug)`.
pub(super) fn roots(host: &Host, config_dir: &str) -> Vec<(String, PathBuf, String)> {
    let projects = host.join_str(config_dir, "projects");
    let Ok(entries) = fs::read_dir(host.to_local(&projects)) else {
        return Vec::new();
    };
    let mut roots: Vec<_> = entries
        .flatten()
        .filter_map(|entry| {
            let local = entry.path().join("memory");
            local.is_dir().then(|| {
                let slug = entry.file_name().to_string_lossy().into_owned();
                let root = host.join_str(&host.join_str(&projects, &slug), "memory");
                (root, local, slug)
            })
        })
        .collect();
    roots.sort_by(|a, b| a.2.cmp(&b.2));
    roots
}

pub(super) fn stores(host: &Host, config_dir: &str, projects: &[String]) -> Vec<MemoryStore> {
    roots(host, config_dir)
        .into_iter()
        .map(|(root, local, dir_slug)| {
            let project = projects.iter().find(|path| slug(path) == dir_slug);
            let index = fs::read_to_string(local.join(INDEX)).unwrap_or_default();
            MemoryStore {
                entries: files::walk(&local, &[])
                    .iter()
                    .map(|found| {
                        let mut entry = files::entry(found, host, &root);
                        if found.rel == INDEX {
                            entry.kind = Some("index".into());
                        } else {
                            entry.unindexed = !links_to(&index, &found.rel);
                        }
                        entry
                    })
                    .collect(),
                title: project
                    .and_then(|path| Path::new(path).file_name())
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or(dir_slug),
                project_path: project.cloned(),
                root,
            }
        })
        .collect()
}

fn links_to(index: &str, rel: &str) -> bool {
    index.contains(&format!("]({rel})"))
}

/// The index without the lines that link to `rel`, or `None` when no line does.
pub(super) fn unlink(index: &str, rel: &str) -> Option<String> {
    if !links_to(index, rel) {
        return None;
    }
    let kept: Vec<&str> = index.lines().filter(|line| !links_to(line, rel)).collect();
    let mut text = kept.join("\n");
    if index.ends_with('\n') {
        text.push('\n');
    }
    Some(text)
}

/// Delete a memory and drop its line from the store's index.
pub(super) fn delete(file: &Path, root: &Path) -> Result<(), String> {
    let rel = file
        .strip_prefix(root)
        .map_err(|_| format!("{} is not in {}", file.display(), root.display()))?
        .to_string_lossy()
        .replace('\\', "/");
    fs::remove_file(file)
        .map_err(|error| format!("Could not delete {}: {error}", file.display()))?;
    let index = root.join(INDEX);
    if rel != INDEX {
        if let Some(text) = fs::read_to_string(&index)
            .ok()
            .and_then(|text| unlink(&text, &rel))
        {
            files::write_atomic(&index, &text)?;
        }
    }
    Ok(())
}

fn settings_path(local_config: &Path) -> PathBuf {
    local_config.join("settings.json")
}

pub(super) fn toggles(local_config: &Path) -> Result<Vec<MemoryToggle>, String> {
    let value = match fs::read_to_string(settings_path(local_config)) {
        Ok(text) if !text.trim().is_empty() => serde_json::from_str::<serde_json::Value>(&text)
            .map_err(|error| format!("settings.json is not valid JSON: {error}"))?
            .get(SETTING)
            .and_then(|value| value.as_bool()),
        _ => None,
    };
    Ok(vec![MemoryToggle {
        key: SETTING.into(),
        label: "Auto memory".into(),
        description: "Let Claude Code save and recall memories for each project.".into(),
        value: value.unwrap_or(true),
    }])
}

pub(super) fn set_toggle(local_config: &Path, key: &str, value: bool) -> Result<(), String> {
    if key != SETTING {
        return Err(format!("{key} is not a Claude Code memory setting"));
    }
    let path = settings_path(local_config);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(format!("Could not read settings.json: {error}")),
    };
    let updated = set_bool(&text, SETTING, value)?;
    fs::create_dir_all(local_config)
        .map_err(|error| format!("Could not create {}: {error}", local_config.display()))?;
    files::write_atomic(&path, &updated)
}

/// Set a top-level boolean in a JSON object by editing its text, so the keys
/// the user ordered and formatted keep their order and formatting.
fn set_bool(text: &str, key: &str, value: bool) -> Result<String, String> {
    let invalid = |error: serde_json::Error| format!("settings.json is not valid JSON: {error}");
    if text.trim().is_empty() {
        return Ok(format!("{{\n  \"{key}\": {value}\n}}\n"));
    }
    let parsed: serde_json::Value = serde_json::from_str(text).map_err(invalid)?;
    let object = parsed
        .as_object()
        .ok_or("settings.json is not a JSON object")?;

    let updated = match object.get(key) {
        Some(current) if current.as_bool() == Some(value) => return Ok(text.to_string()),
        Some(current) => {
            let quoted = format!("\"{key}\"");
            let start = text
                .find(&quoted)
                .ok_or("settings.json could not be edited")?;
            let after = &text[start + quoted.len()..];
            let colon = after.find(':').ok_or("settings.json could not be edited")?;
            let rest = &after[colon + 1..];
            let literal = current.to_string();
            let offset = rest
                .find(&literal)
                .ok_or("settings.json could not be edited")?;
            let at = start + quoted.len() + colon + 1 + offset;
            format!("{}{value}{}", &text[..at], &text[at + literal.len()..])
        }
        None => {
            let brace = text.find('{').ok_or("settings.json is not a JSON object")?;
            let separator = if object.is_empty() { "" } else { "," };
            format!(
                "{}\n  \"{key}\": {value}{separator}{}",
                &text[..=brace],
                &text[brace + 1..]
            )
        }
    };
    // The edit is textual; prove it landed on the top-level key.
    let check: serde_json::Value = serde_json::from_str(&updated).map_err(invalid)?;
    if check.get(key).and_then(|v| v.as_bool()) != Some(value)
        || check.as_object().map(|o| o.len())
            != Some(object.len() + usize::from(!object.contains_key(key)))
    {
        return Err("settings.json could not be edited safely".into());
    }
    Ok(updated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_matches_claude_project_directories() {
        assert_eq!(
            slug("/Users/me/side-projects/codex-custom"),
            "-Users-me-side-projects-codex-custom"
        );
    }

    #[test]
    fn lists_projects_and_flags_unindexed_files() {
        let config = tempfile::tempdir().unwrap();
        let memory = config.path().join("projects/-work-app/memory");
        fs::create_dir_all(&memory).unwrap();
        fs::write(
            memory.join(INDEX),
            "# Memory index\n\n- [Roadmap](roadmap.md) — status\n",
        )
        .unwrap();
        fs::write(
            memory.join("roadmap.md"),
            "---\nname: roadmap\ndescription: what is done\nmetadata:\n  type: project\n---\nbody",
        )
        .unwrap();
        fs::write(memory.join("stray.md"), "no index line").unwrap();
        fs::create_dir_all(config.path().join("projects/-other/no-memory")).unwrap();

        let config_dir = config.path().to_string_lossy().into_owned();
        let stores = stores(&Host::Native, &config_dir, &["/work/app".to_string()]);
        assert_eq!(stores.len(), 1);
        assert_eq!(stores[0].title, "app");
        assert_eq!(stores[0].project_path.as_deref(), Some("/work/app"));
        let entries: Vec<_> = stores[0]
            .entries
            .iter()
            .map(|e| (e.rel_path.as_str(), e.kind.as_deref(), e.unindexed))
            .collect();
        assert_eq!(
            entries,
            [
                ("MEMORY.md", Some("index"), false),
                ("roadmap.md", Some("project"), false),
                ("stray.md", None, true),
            ]
        );
        assert_eq!(
            stores[0].entries[1].description.as_deref(),
            Some("what is done")
        );
    }

    #[test]
    fn delete_removes_the_index_line() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(
            root.join(INDEX),
            "# Index\n- [A](a.md) — a\n- [B](b.md) — b\n",
        )
        .unwrap();
        fs::write(root.join("a.md"), "a").unwrap();
        delete(&root.join("a.md"), root).unwrap();
        assert!(!root.join("a.md").exists());
        assert_eq!(
            fs::read_to_string(root.join(INDEX)).unwrap(),
            "# Index\n- [B](b.md) — b\n"
        );
        assert_eq!(unlink("- [B](b.md)", "a.md"), None);
    }

    #[test]
    fn toggle_edits_settings_in_place() {
        let config = tempfile::tempdir().unwrap();
        assert!(toggles(config.path()).unwrap()[0].value);

        set_toggle(config.path(), SETTING, false).unwrap();
        assert!(!toggles(config.path()).unwrap()[0].value);

        let settings = settings_path(config.path());
        fs::write(
            &settings,
            "{\n  \"model\": \"opus\",\n  \"autoMemoryEnabled\": false\n}\n",
        )
        .unwrap();
        set_toggle(config.path(), SETTING, true).unwrap();
        assert_eq!(
            fs::read_to_string(&settings).unwrap(),
            "{\n  \"model\": \"opus\",\n  \"autoMemoryEnabled\": true\n}\n"
        );

        fs::write(&settings, "{\n  \"model\": \"opus\"\n}\n").unwrap();
        set_toggle(config.path(), SETTING, false).unwrap();
        let text = fs::read_to_string(&settings).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed["model"], "opus");
        assert_eq!(parsed[SETTING], false);
        assert!(set_toggle(config.path(), "other", true).is_err());
    }

    #[test]
    fn set_bool_refuses_a_nested_match() {
        let text = "{\"env\": {\"autoMemoryEnabled\": true}, \"autoMemoryEnabled\": true}";
        assert!(set_bool(text, SETTING, false).is_err());
    }
}
