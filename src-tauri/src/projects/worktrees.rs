//! Discovering and classifying Codex-managed worktrees.
//!
//! Codex creates worktrees under `<codex_home>/worktrees/<hash>/<name>` (kept)
//! and `<codex_home>/worktrees-tmp/<hash>/<name>` (discardable). Both appear in
//! the sidebar as projects in their own right, alongside the repository they
//! were cut from.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::RuntimeConfig;

/// Every worktree Codex has created under this home, permanent and temporary.
pub(crate) fn discover_worktrees(runtime: &RuntimeConfig) -> Vec<String> {
    let mut found = discover_worktree_root(&runtime.codex_home.join("worktrees"));
    found.extend(discover_worktree_root(
        &runtime.codex_home.join("worktrees-tmp"),
    ));
    found.sort();
    found
}

/// Scan one `<root>/<group>/<name>` worktree tree (the Codex-home layout for
/// both permanent `worktrees/` and temporary `worktrees-tmp/`).
fn discover_worktree_root(root: &Path) -> Vec<String> {
    let Ok(hashes) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for hash in hashes.flatten() {
        let Ok(names) = fs::read_dir(hash.path()) else {
            continue;
        };
        for entry in names.flatten() {
            if entry.path().is_dir() {
                found.push(entry.path().display().to_string());
            }
        }
    }
    found
}

/// Canonical-path prefix check shared by the worktree classifiers.
fn path_under(root: PathBuf, path: &str) -> bool {
    let canonical_root = fs::canonicalize(&root).unwrap_or(root);
    match fs::canonicalize(path) {
        Ok(canonical) => canonical.starts_with(&canonical_root),
        Err(_) => Path::new(path).starts_with(&canonical_root),
    }
}

/// A project is a Codex-managed permanent worktree only when its *canonical*
/// path lives under `<codex_home>/worktrees/`. Identity is the canonical path,
/// not the display name — an arbitrary linked worktree elsewhere is a plain
/// folder, never labelled Codex-managed by path resemblance alone.
pub(crate) fn is_worktree_path(runtime: &RuntimeConfig, path: &str) -> bool {
    path_under(runtime.codex_home.join("worktrees"), path)
}

/// Temporary worktrees live under `<codex_home>/worktrees-tmp/` — persistent
/// across app restarts (never the OS temp dir) but intended to be discarded.
pub(crate) fn is_temp_worktree_path(runtime: &RuntimeConfig, path: &str) -> bool {
    path_under(runtime.codex_home.join("worktrees-tmp"), path)
}

/// The main working tree a linked worktree belongs to, so discovering a
/// worktree also surfaces its repository in the sidebar.
pub(crate) fn worktree_parent_project(worktree: &str) -> Option<String> {
    let output = Command::new("git")
        .args(["-C", worktree, "worktree", "list", "--porcelain"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    let main = stdout
        .lines()
        .find_map(|line| line.strip_prefix("worktree "))?
        .trim();
    if main.is_empty() || main == worktree || !Path::new(main).is_dir() {
        return None;
    }
    Some(main.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_worktrees_by_canonical_path_not_by_name() {
        let directory = tempfile::tempdir().unwrap();
        let home = directory.path().join("codex-home");
        fs::create_dir_all(home.join("worktrees/abc/feature")).unwrap();
        fs::create_dir_all(home.join("worktrees-tmp/abc/scratch")).unwrap();
        // A folder that merely *looks* like a managed worktree.
        let impostor = directory.path().join("worktrees/abc/feature");
        fs::create_dir_all(&impostor).unwrap();

        let runtime = RuntimeConfig {
            codex_home: home.clone(),
            codex_binary: PathBuf::from("codex"),
        };
        let managed = home.join("worktrees/abc/feature").display().to_string();
        let temporary = home.join("worktrees-tmp/abc/scratch").display().to_string();

        assert!(is_worktree_path(&runtime, &managed));
        assert!(!is_temp_worktree_path(&runtime, &managed));
        assert!(is_temp_worktree_path(&runtime, &temporary));
        assert!(!is_worktree_path(&runtime, &impostor.display().to_string()));
    }

    #[test]
    fn discovers_both_permanent_and_temporary_worktrees() {
        let directory = tempfile::tempdir().unwrap();
        let home = directory.path().join("codex-home");
        fs::create_dir_all(home.join("worktrees/abc/feature")).unwrap();
        fs::create_dir_all(home.join("worktrees-tmp/def/scratch")).unwrap();
        // Loose files at the group level are not worktrees.
        fs::write(home.join("worktrees/abc/notes.txt"), "").unwrap();

        let found = discover_worktrees(&RuntimeConfig {
            codex_home: home,
            codex_binary: PathBuf::from("codex"),
        });
        assert_eq!(found.len(), 2);
        assert!(found.iter().any(|path| path.ends_with("feature")));
        assert!(found.iter().any(|path| path.ends_with("scratch")));
    }

    #[test]
    fn a_missing_worktree_root_discovers_nothing() {
        let directory = tempfile::tempdir().unwrap();
        assert!(discover_worktrees(&RuntimeConfig {
            codex_home: directory.path().join("never-created"),
            codex_binary: PathBuf::from("codex"),
        })
        .is_empty());
    }
}
