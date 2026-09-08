//! Discovering and classifying Codex-managed worktrees.
//!
//! Codex creates worktrees under `<codex_home>/worktrees/<hash>/<name>` (kept)
//! and `<codex_home>/worktrees-tmp/<hash>/<name>` (discardable). Both appear in
//! the sidebar as projects in their own right, alongside the repository they
//! were cut from. A linked worktree living anywhere else can be adopted by the
//! user; it is then flagged in the store rather than recognised by its path.

use std::fs;
use std::path::{Path, PathBuf};

use crate::util::host::Host;
use crate::RuntimeConfig;

/// Every worktree Codex has created under this home, permanent and temporary.
pub(crate) fn discover_worktrees(runtime: &RuntimeConfig) -> Vec<String> {
    let host = &runtime.host;
    let home = runtime.codex_home_str();
    let mut found = discover_worktree_root(host, &host.join_str(&home, "worktrees"));
    found.extend(discover_worktree_root(
        host,
        &host.join_str(&home, "worktrees-tmp"),
    ));
    found.sort();
    found
}

/// Scan one `<root>/<group>/<name>` worktree tree (the Codex-home layout for
/// both permanent `worktrees/` and temporary `worktrees-tmp/`). `root` is a
/// host path; the result is host paths too.
fn discover_worktree_root(host: &Host, root: &str) -> Vec<String> {
    let Ok(hashes) = fs::read_dir(host.to_local(root)) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for hash in hashes.flatten() {
        let group = host.join_str(root, &hash.file_name().to_string_lossy());
        let Ok(names) = fs::read_dir(hash.path()) else {
            continue;
        };
        for entry in names.flatten() {
            if entry.path().is_dir() {
                found.push(host.join_str(&group, &entry.file_name().to_string_lossy()));
            }
        }
    }
    found
}

/// Canonical-path prefix check shared by the worktree classifiers.
fn path_under(host: &Host, root: &str, path: &str) -> bool {
    match host {
        Host::Native => {
            let root = PathBuf::from(root);
            let canonical_root = fs::canonicalize(&root).unwrap_or(root);
            canonicalize_lenient(Path::new(path)).starts_with(&canonical_root)
        }
        Host::Wsl { .. } => host.is_under(&host.canonical(root), &host.canonical(path)),
    }
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
/// folder unless the user adopted it (`StoredProject::worktree`), never
/// labelled Codex-managed by path resemblance alone.
pub(crate) fn is_worktree_path(runtime: &RuntimeConfig, path: &str) -> bool {
    let host = &runtime.host;
    path_under(
        host,
        &host.join_str(&runtime.codex_home_str(), "worktrees"),
        path,
    )
}

/// Temporary worktrees live under `<codex_home>/worktrees-tmp/` — persistent
/// across app restarts (never the OS temp dir) but intended to be discarded.
pub(crate) fn is_temp_worktree_path(runtime: &RuntimeConfig, path: &str) -> bool {
    is_temp_worktree_path_on(&runtime.host, &runtime.codex_home_str(), path)
}

/// Where temporary worktrees for this Codex home live.
pub fn temp_worktrees_root(codex_home: &Path) -> PathBuf {
    codex_home.join("worktrees-tmp")
}

/// [`is_temp_worktree_path`] against an explicit native Codex home.
pub fn is_temp_worktree_path_under(codex_home: &Path, path: &str) -> bool {
    is_temp_worktree_path_on(&Host::Native, &codex_home.to_string_lossy(), path)
}

/// [`is_temp_worktree_path`] against an explicit Codex home on `host`.
pub(crate) fn is_temp_worktree_path_on(host: &Host, codex_home: &str, path: &str) -> bool {
    path_under(host, &host.join_str(codex_home, "worktrees-tmp"), path)
}

/// The main working tree a linked worktree belongs to, so discovering a
/// worktree also surfaces its repository in the sidebar.
pub fn worktree_parent_project(worktree: &str) -> Option<String> {
    worktree_parent_project_on(&Host::Native, worktree)
}

/// [`worktree_parent_project`] on `host`.
pub(crate) fn worktree_parent_project_on(host: &Host, worktree: &str) -> Option<String> {
    let output = crate::git::run_git(
        host,
        Path::new(worktree),
        &["worktree", "list", "--porcelain"],
        crate::git::run::READ_TIMEOUT,
    )
    .ok()?;
    if !output.ok {
        return None;
    }
    let main = output
        .stdout
        .lines()
        .find_map(|line| line.strip_prefix("worktree "))?
        .trim();
    if main.is_empty() || main == worktree || !host.is_dir(main) {
        return None;
    }
    Some(main.to_string())
}

/// The main working tree of a *linked* worktree at `path`, for adopting it as
/// a sidebar project. Refuses folders that are not a repository and the main
/// working tree itself, since neither is a worktree to adopt.
pub(crate) fn linked_worktree_parent(host: &Host, path: &Path) -> Result<String, String> {
    let output = crate::git::run_git(
        host,
        path,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-dir",
            "--git-common-dir",
        ],
        crate::git::run::READ_TIMEOUT,
    )?;
    if !output.ok {
        return Err(format!("{} is not a Git repository", path.display()));
    }
    let mut lines = output.stdout.lines().map(str::trim);
    let git_dir = lines.next().unwrap_or("");
    let common_dir = lines.next().unwrap_or("");
    if git_dir.is_empty() || common_dir.is_empty() {
        return Err(format!("{} is not a Git repository", path.display()));
    }
    if git_dir == common_dir {
        return Err(format!(
            "{} is the main working tree, not a linked worktree",
            path.display()
        ));
    }
    worktree_parent_project_on(host, &host.path_string(path))
        .ok_or_else(|| "Could not find the repository this worktree belongs to".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

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
            host: crate::util::host::Host::Native,
        };
        let managed = home.join("worktrees/abc/feature").display().to_string();
        let temporary = home.join("worktrees-tmp/abc/scratch").display().to_string();

        assert!(is_worktree_path(&runtime, &managed));
        assert!(!is_temp_worktree_path(&runtime, &managed));
        assert!(is_temp_worktree_path(&runtime, &temporary));
        assert!(!is_worktree_path(&runtime, &impostor.display().to_string()));
    }

    #[cfg(unix)]
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
            host: crate::util::host::Host::Native,
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
            host: crate::util::host::Host::Native,
        })
        .is_empty());
    }

    fn git(dir: &Path, args: &[&str]) -> bool {
        Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.com")
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    #[test]
    fn only_a_linked_worktree_can_be_adopted() {
        let directory = tempfile::tempdir().unwrap();
        let repo = directory.path().join("repo");
        fs::create_dir_all(&repo).unwrap();
        assert!(git(&repo, &["init", "-q", "-b", "main"]));
        fs::write(repo.join("file.txt"), "hello").unwrap();
        assert!(git(&repo, &["add", "."]));
        assert!(git(&repo, &["commit", "-q", "-m", "init"]));
        let linked = directory.path().join("repo-feature");
        assert!(git(
            &repo,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "feature",
                linked.to_str().unwrap()
            ]
        ));
        let plain = directory.path().join("plain");
        fs::create_dir_all(&plain).unwrap();

        let parent = linked_worktree_parent(&Host::Native, &linked).unwrap();
        assert_eq!(
            fs::canonicalize(&parent).unwrap(),
            fs::canonicalize(&repo).unwrap()
        );
        let main_error = linked_worktree_parent(&Host::Native, &repo).unwrap_err();
        assert!(main_error.contains("main working tree"), "{main_error}");
        let plain_error = linked_worktree_parent(&Host::Native, &plain).unwrap_err();
        assert!(
            plain_error.contains("not a Git repository"),
            "{plain_error}"
        );
    }
}
