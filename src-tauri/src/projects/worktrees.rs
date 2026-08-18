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
    canonicalize_lenient(Path::new(path)).starts_with(&canonical_root)
}

/// `fs::canonicalize` for paths that may no longer exist (a removed temporary
/// worktree): the longest existing ancestor is canonicalised and the missing
/// tail re-appended, so a path spelled through a symlinked home still
/// classifies after its directory is gone.
fn canonicalize_lenient(path: &Path) -> PathBuf {
    if let Ok(canonical) = fs::canonicalize(path) {
        return canonical;
    }
    let mut missing = Vec::new();
    let mut current = path;
    while let Some(parent) = current.parent() {
        if let Some(name) = current.file_name() {
            missing.push(name.to_owned());
        }
        if let Ok(canonical) = fs::canonicalize(parent) {
            let mut rebuilt = canonical;
            for part in missing.into_iter().rev() {
                rebuilt.push(part);
            }
            return rebuilt;
        }
        current = parent;
    }
    path.to_path_buf()
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
    is_temp_worktree_path_under(&runtime.codex_home, path)
}

/// Where temporary worktrees for this Codex home live.
pub fn temp_worktrees_root(codex_home: &Path) -> PathBuf {
    codex_home.join("worktrees-tmp")
}

/// [`is_temp_worktree_path`] against an explicit Codex home.
pub fn is_temp_worktree_path_under(codex_home: &Path, path: &str) -> bool {
    path_under(temp_worktrees_root(codex_home), path)
}

/// The main working tree a linked worktree belongs to, so discovering a
/// worktree also surfaces its repository in the sidebar.
pub fn worktree_parent_project(worktree: &str) -> Option<String> {
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
    fn a_removed_temp_worktree_is_still_classified_under_a_symlinked_home() {
        // macOS temp dirs are symlinks (/var → /private/var): a path that no
        // longer exists cannot be canonicalised, but must still count when it
        // is spelled through the non-canonical home.
        let directory = tempfile::tempdir().unwrap();
        let real_home = directory.path().join("real-home");
        fs::create_dir_all(real_home.join("worktrees-tmp")).unwrap();
        let linked_home = directory.path().join("linked-home");
        std::os::unix::fs::symlink(&real_home, &linked_home).unwrap();
        let gone = linked_home.join("worktrees-tmp/abc/removed");
        assert!(!gone.exists());
        assert!(is_temp_worktree_path_under(
            &linked_home,
            &gone.display().to_string()
        ));
        assert!(is_temp_worktree_path_under(
            &real_home,
            &gone.display().to_string()
        ));
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
