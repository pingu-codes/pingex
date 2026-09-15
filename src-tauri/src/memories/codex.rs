//! Codex memories: consolidated by a background agent into
//! `$CODEX_HOME/memories` (and `memories_v2` once that version is on),
//! switched by `[features] memories` and the `[memories]` table.

use std::path::{Path, PathBuf};

use super::{files, MemoryStore, MemoryToggle};
use crate::settings::codex_config::{read_doc, write_doc};
use crate::util::host::Host;

/// Store directories below the Codex home, with the title each is shown as.
const ROOTS: [(&str, &str); 2] = [("memories", "Memories"), ("memories_v2", "Memories (v2)")];

/// Scratch the consolidation agent deletes before it commits.
const SKIP: [&str; 1] = ["phase2_workspace_diff.md"];

/// The switches, as `(table, key, label, description, Codex's default)`.
const TOGGLES: [(&str, &str, &str, &str, bool); 3] = [
    (
        "features",
        "memories",
        "Memories",
        "Let Codex remember things between threads. Off by default in Codex.",
        false,
    ),
    (
        "memories",
        "generate_memories",
        "Generate memories",
        "Extract memories from finished threads.",
        true,
    ),
    (
        "memories",
        "use_memories",
        "Use memories",
        "Give new threads the memories Codex has consolidated.",
        true,
    ),
];

/// Every store directory the Codex home has, as `(host root, local root)`.
pub(super) fn roots(host: &Host, home: &str) -> Vec<(String, PathBuf, &'static str)> {
    ROOTS
        .iter()
        .map(|(dir, title)| {
            let root = host.join_str(home, dir);
            let local = host.to_local(&root);
            (root, local, *title)
        })
        .filter(|(_, local, _)| local.is_dir())
        .collect()
}

pub(super) fn stores(host: &Host, home: &str) -> Vec<MemoryStore> {
    roots(host, home)
        .into_iter()
        .map(|(root, local, title)| MemoryStore {
            entries: files::walk(&local, &SKIP)
                .iter()
                .map(|found| {
                    let mut entry = files::entry(found, host, &root);
                    entry.kind = entry.kind.or_else(|| kind(&found.rel).map(str::to_string));
                    entry
                })
                .collect(),
            root,
            title: title.to_string(),
            project_path: None,
        })
        .collect()
}

fn kind(rel: &str) -> Option<&'static str> {
    match rel {
        "MEMORY.md" => Some("index"),
        "memory_summary.md" => Some("summary"),
        "raw_memories.md" => Some("raw"),
        _ if rel.starts_with("rollout_summaries/") => Some("rollout"),
        _ if rel.starts_with("skills/") => Some("skill"),
        _ => None,
    }
}

pub(super) fn toggles(local_home: &Path) -> Result<Vec<MemoryToggle>, String> {
    let document = read_doc(local_home)?;
    Ok(TOGGLES
        .iter()
        .map(|(table, key, label, description, default)| MemoryToggle {
            key: format!("{table}.{key}"),
            label: label.to_string(),
            description: description.to_string(),
            value: document
                .get(table)
                .and_then(|table| table.get(key))
                .and_then(|value| value.as_bool())
                .unwrap_or(*default),
        })
        .collect())
}

pub(super) fn set_toggle(local_home: &Path, key: &str, value: bool) -> Result<(), String> {
    let (table, key) = TOGGLES
        .iter()
        .map(|(table, key, ..)| (*table, *key))
        .find(|(table, name)| format!("{table}.{name}") == key)
        .ok_or_else(|| format!("{key} is not a Codex memory setting"))?;
    let mut document = read_doc(local_home)?;
    if document
        .get(table)
        .is_some_and(|item| !item.is_table_like())
    {
        return Err(format!("config.toml has a `{table}` that is not a table"));
    }
    document[table][key] = toml_edit::value(value);
    write_doc(local_home, &document)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn lists_both_roots_with_kinds() {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join("memories");
        fs::create_dir_all(root.join("rollout_summaries")).unwrap();
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(root.join(".git/HEAD"), "ref").unwrap();
        fs::write(root.join("MEMORY.md"), "# Memory\nfacts").unwrap();
        fs::write(root.join("phase2_workspace_diff.md"), "diff").unwrap();
        fs::write(root.join("rollout_summaries/fix-login.md"), "Fixed login").unwrap();

        let home_str = home.path().to_string_lossy().into_owned();
        let stores = stores(&Host::Native, &home_str);
        assert_eq!(stores.len(), 1);
        let entries: Vec<_> = stores[0]
            .entries
            .iter()
            .map(|entry| {
                (
                    entry.rel_path.as_str(),
                    entry.kind.as_deref(),
                    entry.title.as_str(),
                )
            })
            .collect();
        assert_eq!(
            entries,
            [
                ("MEMORY.md", Some("index"), "MEMORY"),
                (
                    "rollout_summaries/fix-login.md",
                    Some("rollout"),
                    "fix-login"
                )
            ]
        );
        assert_eq!(stores[0].entries[0].description.as_deref(), Some("Memory"));
    }

    #[test]
    fn toggles_default_then_round_trip_keeping_comments() {
        let home = tempfile::tempdir().unwrap();
        fs::write(home.path().join("config.toml"), "# mine\nmodel = \"x\"\n").unwrap();

        let values = |home: &Path| -> Vec<bool> {
            toggles(home)
                .unwrap()
                .into_iter()
                .map(|toggle| toggle.value)
                .collect()
        };
        assert_eq!(values(home.path()), [false, true, true]);

        set_toggle(home.path(), "features.memories", true).unwrap();
        set_toggle(home.path(), "memories.use_memories", false).unwrap();
        assert_eq!(values(home.path()), [true, true, false]);

        let text = fs::read_to_string(home.path().join("config.toml")).unwrap();
        assert!(text.starts_with("# mine\nmodel = \"x\"\n"), "{text}");
        assert!(set_toggle(home.path(), "features.other", true).is_err());
    }
}
