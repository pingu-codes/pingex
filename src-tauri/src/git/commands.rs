//! The Git commands the frontend calls.
//!
//! Every one runs on a blocking task, since the work is a subprocess rather than
//! async I/O. Mutations take the per-common-directory lock first, so two
//! concurrent operations against the same repository cannot interleave.

use std::path::{Path, PathBuf};
use tauri::State;

use super::branches::read_branches;
use super::commits::read_recent_commits;
use super::run::{common_dir_of, lock_for_common_dir, redact_git_error, run_git, WRITE_TIMEOUT};
use super::status::{read_repo_info, read_status};
use super::types::{
    BranchRef, CommitInfo, GitRepoInfo, GitStatus, WorktreeAddRequest, WorktreeBranch,
    WorktreeEntry,
};
use super::worktrees::read_worktrees;
use crate::projects::worktrees::{is_temp_worktree_path, worktree_parent_project};
use crate::storage;
use crate::AppState;

#[tauri::command]
pub(crate) async fn git_repo_info(dir: String) -> Result<GitRepoInfo, String> {
    tauri::async_runtime::spawn_blocking(move || read_repo_info(Path::new(&dir)))
        .await
        .map_err(|_| "Git inspection failed".to_string())
}

#[tauri::command]
pub(crate) async fn git_status(dir: String) -> Result<GitStatus, String> {
    tauri::async_runtime::spawn_blocking(move || read_status(Path::new(&dir)))
        .await
        .map_err(|_| "Git inspection failed".to_string())?
}

#[tauri::command]
pub(crate) async fn git_worktrees(
    repo_dir: String,
    state: State<'_, AppState>,
) -> Result<Vec<WorktreeEntry>, String> {
    let codex_home = state.runtime().codex_home;
    tauri::async_runtime::spawn_blocking(move || read_worktrees(Path::new(&repo_dir), &codex_home))
        .await
        .map_err(|_| "Git inspection failed".to_string())?
}

#[tauri::command]
pub(crate) async fn git_recent_commits(
    dir: String,
    limit: Option<usize>,
) -> Result<Vec<CommitInfo>, String> {
    let limit = limit.unwrap_or(20);
    tauri::async_runtime::spawn_blocking(move || read_recent_commits(Path::new(&dir), limit))
        .await
        .map_err(|_| "Git inspection failed".to_string())?
}

#[tauri::command]
pub(crate) async fn git_branches(
    dir: String,
    limit: Option<usize>,
) -> Result<Vec<BranchRef>, String> {
    let limit = limit.unwrap_or(200);
    tauri::async_runtime::spawn_blocking(move || read_branches(Path::new(&dir), limit))
        .await
        .map_err(|_| "Git inspection failed".to_string())?
}

#[tauri::command]
pub(crate) async fn git_worktree_add(
    repo_dir: String,
    request: WorktreeAddRequest,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let runtime = state.runtime();
    let database = state.database();
    let created = request.path.clone();
    let repo_dir_for_git = repo_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let repo = PathBuf::from(&repo_dir_for_git);
        let common = common_dir_of(&repo)?;
        let guard = lock_for_common_dir(&common);
        let _lock = guard.lock().expect("git common-dir lock poisoned");

        // The Codex-home layouts nest worktrees one level deep
        // (`worktrees/<group>/<name>`); git does not create missing parents.
        if let Some(parent) = Path::new(&request.path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let mut args: Vec<String> = vec!["worktree".into(), "add".into()];
        match &request.branch {
            WorktreeBranch::Existing { name } => {
                args.push(request.path.clone());
                args.push(name.clone());
            }
            WorktreeBranch::New { name, base } => {
                args.push("-b".into());
                args.push(name.clone());
                args.push(request.path.clone());
                if let Some(base) = base.as_deref().filter(|b| !b.trim().is_empty()) {
                    args.push(base.to_string());
                }
            }
        }
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let output = run_git(&repo, &arg_refs, WRITE_TIMEOUT)?;
        if !output.ok {
            return Err(redact_git_error("Could not create the worktree", &output));
        }
        Ok(())
    })
    .await
    .map_err(|_| "Git operation failed".to_string())??;

    // A temporary worktree is scaffolding for a thread, not a project: record
    // the repository it came from so its threads stay listed there once the
    // worktree is discarded.
    if is_temp_worktree_path(&runtime, &created) {
        let parent = worktree_parent_project(&created).unwrap_or(repo_dir);
        storage::record_temp_worktree(&database, &created, &parent).await?;
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn git_worktree_remove(
    repo_dir: String,
    path: String,
    force: bool,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let repo = PathBuf::from(&repo_dir);
        let common = common_dir_of(&repo)?;
        let guard = lock_for_common_dir(&common);
        let _lock = guard.lock().expect("git common-dir lock poisoned");

        // Refuse to remove a dirty worktree unless force is explicitly given.
        if !force {
            if let Ok(status) = read_status(Path::new(&path)) {
                if status.counts.is_dirty() {
                    return Err("This worktree has uncommitted changes".to_string());
                }
            }
        }
        let mut args = vec!["worktree", "remove"];
        if force {
            args.push("--force");
        }
        args.push(&path);
        let output = run_git(&repo, &args, WRITE_TIMEOUT)?;
        if !output.ok {
            return Err(redact_git_error("Could not remove the worktree", &output));
        }
        Ok(())
    })
    .await
    .map_err(|_| "Git operation failed".to_string())?
}

#[tauri::command]
pub(crate) async fn git_worktree_prune(repo_dir: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let repo = PathBuf::from(&repo_dir);
        let common = common_dir_of(&repo)?;
        let guard = lock_for_common_dir(&common);
        let _lock = guard.lock().expect("git common-dir lock poisoned");

        let output = run_git(&repo, &["worktree", "prune"], WRITE_TIMEOUT)?;
        if !output.ok {
            return Err(redact_git_error("Could not prune worktrees", &output));
        }
        Ok(())
    })
    .await
    .map_err(|_| "Git operation failed".to_string())?
}

#[tauri::command]
pub(crate) async fn git_worktree_lock(
    repo_dir: String,
    path: String,
    reason: Option<String>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let repo = PathBuf::from(&repo_dir);
        let common = common_dir_of(&repo)?;
        let guard = lock_for_common_dir(&common);
        let _lock = guard.lock().expect("git common-dir lock poisoned");

        let mut args = vec!["worktree".to_string(), "lock".to_string()];
        if let Some(reason) = reason.as_deref().filter(|r| !r.trim().is_empty()) {
            args.push("--reason".to_string());
            args.push(reason.to_string());
        }
        args.push(path);
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let output = run_git(&repo, &arg_refs, WRITE_TIMEOUT)?;
        if !output.ok {
            return Err(redact_git_error("Could not lock the worktree", &output));
        }
        Ok(())
    })
    .await
    .map_err(|_| "Git operation failed".to_string())?
}

#[tauri::command]
pub(crate) async fn git_worktree_unlock(repo_dir: String, path: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let repo = PathBuf::from(&repo_dir);
        let common = common_dir_of(&repo)?;
        let guard = lock_for_common_dir(&common);
        let _lock = guard.lock().expect("git common-dir lock poisoned");

        let output = run_git(&repo, &["worktree", "unlock", &path], WRITE_TIMEOUT)?;
        if !output.ok {
            return Err(redact_git_error("Could not unlock the worktree", &output));
        }
        Ok(())
    })
    .await
    .map_err(|_| "Git operation failed".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    /// Run git against the fixture repository, isolated from the developer's own
    /// git configuration. Without `GIT_CONFIG_GLOBAL`/`SYSTEM` this test inherits
    /// whatever the machine has set — notably `commit.gpgsign = true`, which
    /// makes `git commit` block on a passphrase prompt it can never receive and
    /// then fail. Test fixtures must not depend on who is running them.
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
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    fn git_available() -> bool {
        Command::new("git")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Builds a real repository with a linked worktree and exercises the read
    /// paths against it — the parsers are unit-tested elsewhere, this checks
    /// they are wired to git correctly.
    #[test]
    fn integration_worktree_lifecycle() {
        if !git_available() {
            eprintln!("skipping: git not available");
            return;
        }
        let temp = tempfile::tempdir().expect("temp dir");
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        assert!(git(&repo, &["init", "-q", "-b", "main"]));
        std::fs::write(repo.join("file.txt"), "hello").unwrap();
        assert!(git(&repo, &["add", "."]));
        assert!(git(&repo, &["commit", "-q", "-m", "init"]));

        // repo_info sees a real repo on main.
        let info = read_repo_info(&repo);
        assert!(info.is_git_repo);
        assert_eq!(info.branch.as_deref(), Some("main"));
        assert!(info.common_dir.is_some());

        // A non-git folder reports is_git_repo=false, no error.
        let plain = temp.path().join("plain");
        std::fs::create_dir_all(&plain).unwrap();
        let plain_info = read_repo_info(&plain);
        assert!(!plain_info.is_git_repo);
        assert!(plain_info.error.is_none());

        // Add a linked worktree on a new branch.
        let wt = temp.path().join("wt-feature");
        let add = run_git(
            &repo,
            &["worktree", "add", "-b", "feature", wt.to_str().unwrap()],
            WRITE_TIMEOUT,
        )
        .unwrap();
        assert!(add.ok, "worktree add failed: {}", add.stderr);

        // Listing includes main + the linked worktree; neither is Codex-managed
        // because they are not under <codex_home>/worktrees.
        let fake_home = temp.path().join("codex-home");
        let entries = read_worktrees(&repo, &fake_home).unwrap();
        assert_eq!(entries.len(), 2);
        assert!(entries[0].is_main);
        assert!(!entries[0].is_codex_managed);
        let feature = entries
            .iter()
            .find(|e| e.branch.as_deref() == Some("feature"))
            .expect("feature worktree listed");
        assert!(!feature.is_main);
        assert!(!feature.is_codex_managed);
        assert!(!feature.missing_dir);

        // Dirty the worktree; a non-forced remove is refused.
        std::fs::write(wt.join("dirty.txt"), "x").unwrap();
        let refused = read_status(&wt).unwrap();
        assert!(refused.counts.is_dirty());

        // recent commits are readable.
        let commits = read_recent_commits(&repo, 10).unwrap();
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].subject, "init");
    }
}
