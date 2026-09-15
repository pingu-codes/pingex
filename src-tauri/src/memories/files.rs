//! File mechanics shared by both harnesses: walking a store, reading
//! frontmatter, guarding paths, writing atomically.

use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

use super::MemoryEntry;
use crate::util::host::Host;

/// How deep a store is walked; Codex skills nest a few levels.
const MAX_DEPTH: usize = 6;

/// A memory file found in a store, before the harness labels it.
pub(super) struct Found {
    pub local: PathBuf,
    pub rel: String,
    pub modified_at: Option<f64>,
    pub bytes: f64,
}

/// Every regular file below `root`, skipping dot entries (`.git`, editor
/// swap files), sorted by relative path.
pub(super) fn walk(root: &Path, skip: &[&str]) -> Vec<Found> {
    let mut found = Vec::new();
    walk_into(root, "", 0, skip, &mut found);
    found.sort_by(|a, b| a.rel.cmp(&b.rel));
    found
}

fn walk_into(dir: &Path, prefix: &str, depth: usize, skip: &[&str], found: &mut Vec<Found>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || skip.contains(&name.as_str()) {
            continue;
        }
        let rel = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if meta.is_dir() {
            if depth < MAX_DEPTH {
                walk_into(&entry.path(), &rel, depth + 1, skip, found);
            }
        } else if meta.is_file() {
            found.push(Found {
                local: entry.path(),
                rel,
                modified_at: meta
                    .modified()
                    .ok()
                    .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                    .map(|elapsed| elapsed.as_secs() as f64),
                bytes: meta.len() as f64,
            });
        }
    }
}

/// The fields a memory file's YAML frontmatter declares.
#[derive(Default, Debug, PartialEq)]
pub(super) struct Frontmatter {
    pub name: Option<String>,
    pub description: Option<String>,
    pub kind: Option<String>,
}

/// Read `name`, `description` and `type` (top level or under `metadata`)
/// from a leading `---` block. Line-based: memory frontmatter is flat.
pub(super) fn frontmatter(text: &str) -> Frontmatter {
    let mut result = Frontmatter::default();
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some("---") {
        return result;
    }
    for line in lines {
        if line.trim() == "---" {
            break;
        }
        let Some((key, value)) = line.trim().split_once(':') else {
            continue;
        };
        let value = value.trim().trim_matches('"').trim_matches('\'').trim();
        if value.is_empty() {
            continue;
        }
        let slot = match key.trim() {
            "name" => &mut result.name,
            "description" => &mut result.description,
            "type" => &mut result.kind,
            _ => continue,
        };
        slot.get_or_insert_with(|| value.to_string());
    }
    result
}

/// The first line of prose after any frontmatter, for files that do not
/// describe themselves.
pub(super) fn first_line(text: &str) -> Option<String> {
    let body = match text.strip_prefix("---") {
        Some(rest) => rest.split_once("\n---").map_or("", |(_, body)| body),
        None => text,
    };
    body.lines()
        .map(|line| line.trim().trim_start_matches('#').trim())
        .find(|line| !line.is_empty() && *line != "---")
        .map(|line| line.chars().take(160).collect())
}

/// Read the head of a file, enough for its frontmatter and first line.
pub(super) fn head(path: &Path) -> String {
    use std::io::Read;
    let mut buffer = Vec::new();
    if let Ok(file) = fs::File::open(path) {
        let _ = file.take(4096).read_to_end(&mut buffer);
    }
    String::from_utf8_lossy(&buffer).into_owned()
}

pub(super) fn entry(found: &Found, host: &Host, root: &str) -> MemoryEntry {
    let text = head(&found.local);
    let meta = frontmatter(&text);
    let stem = Path::new(&found.rel)
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| found.rel.clone());
    MemoryEntry {
        path: host.join_str(root, &found.rel),
        rel_path: found.rel.clone(),
        title: meta.name.unwrap_or(stem),
        description: meta.description.or_else(|| first_line(&text)),
        kind: meta.kind,
        modified_at: found.modified_at,
        bytes: found.bytes,
        unindexed: false,
    }
}

/// Resolve `path` to a local file inside one of `roots`, refusing traversal,
/// dot entries (a store's `.git`), the roots themselves and anything that
/// does not exist yet — memories are edited, not created, here.
pub(super) fn guard(path: &Path, roots: &[PathBuf]) -> Result<(PathBuf, PathBuf), String> {
    let refused = || format!("{} is not a memory file.", path.display());
    if path
        .components()
        .any(|part| matches!(part, Component::ParentDir))
    {
        return Err(refused());
    }
    let file = path.canonicalize().map_err(|_| refused())?;
    if !file.is_file() {
        return Err(refused());
    }
    for root in roots {
        let Ok(root) = root.canonicalize() else {
            continue;
        };
        let Ok(rel) = file.strip_prefix(&root) else {
            continue;
        };
        let hidden = rel.components().any(|part| {
            matches!(part, Component::Normal(name) if name.to_string_lossy().starts_with('.'))
        });
        if hidden || rel.as_os_str().is_empty() {
            return Err(refused());
        }
        return Ok((file, root));
    }
    Err(refused())
}

/// Replace a file's contents without leaving it half-written.
pub(super) fn write_atomic(path: &Path, contents: &str) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", path.display()))?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| format!("Could not write {}: {error}", path.display()))?;
    temp.write_all(contents.as_bytes())
        .map_err(|error| format!("Could not write {}: {error}", path.display()))?;
    temp.persist(path)
        .map_err(|error| format!("Could not write {}: {error}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_flat_and_nested_frontmatter() {
        let text = "---\nname: roadmap\ndescription: \"what is done\"\nmetadata:\n  type: project\n---\n\nBody";
        assert_eq!(
            frontmatter(text),
            Frontmatter {
                name: Some("roadmap".into()),
                description: Some("what is done".into()),
                kind: Some("project".into()),
            }
        );
        assert_eq!(frontmatter("# Title\nname: no"), Frontmatter::default());
    }

    #[test]
    fn first_line_skips_frontmatter_and_headings() {
        assert_eq!(
            first_line("---\nname: x\n---\n\n# Notes\nmore").as_deref(),
            Some("Notes")
        );
        assert_eq!(first_line("\n\nplain").as_deref(), Some("plain"));
        assert_eq!(first_line(""), None);
    }

    #[test]
    fn walk_skips_dot_entries_and_named_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(root.join(".git/HEAD"), "x").unwrap();
        fs::create_dir_all(root.join("rollout_summaries")).unwrap();
        fs::write(root.join("rollout_summaries/a.md"), "a").unwrap();
        fs::write(root.join("MEMORY.md"), "m").unwrap();
        fs::write(root.join("scratch.md"), "s").unwrap();
        let rels: Vec<_> = walk(root, &["scratch.md"])
            .into_iter()
            .map(|f| f.rel)
            .collect();
        assert_eq!(rels, ["MEMORY.md", "rollout_summaries/a.md"]);
    }

    #[test]
    fn guard_keeps_paths_inside_a_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("memory");
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(root.join("a.md"), "a").unwrap();
        fs::write(root.join(".git/config"), "x").unwrap();
        fs::write(dir.path().join("outside.md"), "o").unwrap();
        let roots = [root.clone()];

        assert!(guard(&root.join("a.md"), &roots).is_ok());
        assert!(guard(&root.join("../outside.md"), &roots).is_err());
        assert!(guard(&dir.path().join("outside.md"), &roots).is_err());
        assert!(guard(&root.join(".git/config"), &roots).is_err());
        assert!(guard(&root, &roots).is_err());
        assert!(guard(&root.join("missing.md"), &roots).is_err());
    }

    #[test]
    fn write_atomic_replaces_contents() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.md");
        fs::write(&file, "old").unwrap();
        write_atomic(&file, "new").unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "new");
    }
}
