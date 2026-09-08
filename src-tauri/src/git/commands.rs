//! The Git commands the frontend calls.
//!
//! Every one runs on a blocking task, since the work is a subprocess rather than
//! async I/O. Mutations take the per-common-directory lock first, so two
//! concurrent operations against the same repository cannot interleave.

use std::path::{Path, PathBuf};
use tauri::State;

use super::actions;
use super::branches::read_branches;
use super::changes::{
    handoff, handoff_preflight, read_changes_summary, read_file_diff, ChangesSummary, FileDiff,
    HandoffPreflight, DEFAULT_DIFF_BYTES,
};
use super::commits::read_recent_commits;
use super::run::{common_dir_of, lock_for_common_dir, redact_git_error, run_git, WRITE_TIMEOUT};
use super::status::{read_repo_info, read_status};
use super::types::{
    BranchRef, CommitInfo, CommitResult, GitContext, GitRepoInfo, GitStatus, SyncResult,
    WorktreeAddRequest, WorktreeBranch, WorktreeEntry,
};
use super::worktrees::read_worktrees;
use crate::projects::worktrees::{is_temp_worktree_path, worktree_parent_project_on};
use crate::storage;
use crate::util::host::Host;
use crate::AppState;

#[tauri::command]
#[specta::specta]
pub(crate) async fn git_repo_info(
    dir: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<GitRepoInfo, String> {
    let host = state.ctx(&window).host();
    tauri::async_runtime::spawn_blocking(move || read_repo_info(&host, Path::new(&dir)))
        .await
        .map_err(|_| "Git inspection failed".to_string())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn git_status(
    dir: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<GitStatus, String> {
    let host = state.ctx(&window).host();
    tauri::async_runtime::spawn_blocking(move || read_status(&host, Path::new(&dir)))
        .await
        .map_err(|_| "Git inspection failed".to_string())?
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn git_worktrees(
    repo_dir: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<Vec<WorktreeEntry>, String> {
    let ctx = state.ctx(&window);
    let runtime = ctx.runtime();
    tauri::async_runtime::spawn_blocking(move || {
        read_worktrees(&runtime.host, Path::new(&repo_dir), &runtime.codex_home)
    })
    .await
    .map_err(|_| "Git inspection failed".to_string())?
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn git_recent_commits(
    dir: String,
    limit: Option<usize>,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<Vec<CommitInfo>, String> {
    let limit = limit.unwrap_or(20);
    let host = state.ctx(&window).host();
    tauri::async_runtime::spawn_blocking(move || read_recent_commits(&host, Path::new(&dir), limit))
        .await
        .map_err(|_| "Git inspection failed".to_string())?
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn git_branches(
    dir: String,
    limit: Option<usize>,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<Vec<BranchRef>, String> {
    let limit = limit.unwrap_or(200);
    let host = state.ctx(&window).host();
    tauri::async_runtime::spawn_blocking(move || read_branches(&host, Path::new(&dir), limit))
        .await
        .map_err(|_| "Git inspection failed".to_string())?
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn git_worktree_add(
    repo_dir: String,
    request: WorktreeAddRequest,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let ctx = state.ctx(&window);
    let runtime = ctx.runtime();
    let database = ctx.database();
    let created = request.path.clone();
    let repo_dir_for_git = repo_dir.clone();
    let host = runtime.host.clone();
    let host_for_git = host.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let host = host_for_git;
        let repo = PathBuf::from(&repo_dir_for_git);
        let common = common_dir_of(&host, &repo)?;
        let guard = lock_for_common_dir(&common);
        let _lock = guard.lock().expect("git common-dir lock poisoned");

        // The Codex-home layouts nest worktrees one level deep
        // (`worktrees/<group>/<name>`); git does not create missing parents.
        if let Some(parent) = host.parent_str(&request.path) {
            let _ = std::fs::create_dir_all(host.to_local(&parent));
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
            WorktreeBranch::Tracking { name, remote_ref } => {
                args.push("--track".into());
                args.push("-b".into());
                args.push(name.clone());
                args.push(request.path.clone());
                args.push(remote_ref.clone());
            }
        }
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let output = run_git(&host, &repo, &arg_refs, WRITE_TIMEOUT)?;
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
        let parent = worktree_parent_project_on(&host, &created).unwrap_or(repo_dir);
        storage::record_temp_worktree(&database, &created, &parent).await?;
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn git_worktree_remove(
    repo_dir: String,
    path: String,
    force: bool,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let host = state.ctx(&window).host();
    tauri::async_runtime::spawn_blocking(move || {
        let repo = PathBuf::from(&repo_dir);
        let common = common_dir_of(&host, &repo)?;
        let guard = lock_for_common_dir(&common);
        let _lock = guard.lock().expect("git common-dir lock poisoned");

        // Refuse to remove a dirty worktree unless force is explicitly given.
        if !force {
            if let Ok(status) = read_status(&host, Path::new(&path)) {
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
        let output = run_git(&host, &repo, &args, WRITE_TIMEOUT)?;
        if !output.ok {
            return Err(redact_git_error("Could not remove the worktree", &output));
        }
        Ok(())
    })
    .await
    .map_err(|_| "Git operation failed".to_string())?
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn git_worktree_prune(
    repo_dir: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let host = state.ctx(&window).host();
    tauri::async_runtime::spawn_blocking(move || {
        let repo = PathBuf::from(&repo_dir);
        let common = common_dir_of(&host, &repo)?;
        let guard = lock_for_common_dir(&common);
        let _lock = guard.lock().expect("git common-dir lock poisoned");

        let output = run_git(&host, &repo, &["worktree", "prune"], WRITE_TIMEOUT)?;
        if !output.ok {
            return Err(redact_git_error("Could not prune worktrees", &output));
        }
        Ok(())
    })
    .await
    .map_err(|_| "Git operation failed".to_string())?
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn git_worktree_lock(
    repo_dir: String,
    path: String,
    reason: Option<String>,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let host = state.ctx(&window).host();
    tauri::async_runtime::spawn_blocking(move || {
        let repo = PathBuf::from(&repo_dir);
        let common = common_dir_of(&host, &repo)?;
        let guard = lock_for_common_dir(&common);
        let _lock = guard.lock().expect("git common-dir lock poisoned");

        let mut args = vec!["worktree".to_string(), "lock".to_string()];
        if let Some(reason) = reason.as_deref().filter(|r| !r.trim().is_empty()) {
            args.push("--reason".to_string());
            args.push(reason.to_string());
        }
        args.push(path);
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let output = run_git(&host, &repo, &arg_refs, WRITE_TIMEOUT)?;
        if !output.ok {
            return Err(redact_git_error("Could not lock the worktree", &output));
        }
        Ok(())
    })
    .await
    .map_err(|_| "Git operation failed".to_string())?
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn git_worktree_unlock(
    repo_dir: String,
    path: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let host = state.ctx(&window).host();
    tauri::async_runtime::spawn_blocking(move || {
        let repo = PathBuf::from(&repo_dir);
        let common = common_dir_of(&host, &repo)?;
        let guard = lock_for_common_dir(&common);
        let _lock = guard.lock().expect("git common-dir lock poisoned");

        let output = run_git(&host, &repo, &["worktree", "unlock", &path], WRITE_TIMEOUT)?;
        if !output.ok {
            return Err(redact_git_error("Could not unlock the worktree", &output));
        }
        Ok(())
    })
    .await
    .map_err(|_| "Git operation failed".to_string())?
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn git_changes_summary(
    dir: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<ChangesSummary, String> {
    let ctx = state.ctx(&window);
    let runtime = ctx.runtime();
    tauri::async_runtime::spawn_blocking(move || {
        read_changes_summary(&runtime.host, Path::new(&dir), &runtime.codex_home)
    })
    .await
    .map_err(|_| "Git inspection failed".to_string())?
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn git_file_diff(
    dir: String,
    base: String,
    path: String,
    untracked: bool,
    max_bytes: Option<usize>,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<FileDiff, String> {
    let host = state.ctx(&window).host();
    tauri::async_runtime::spawn_blocking(move || {
        read_file_diff(
            &host,
            Path::new(&dir),
            &base,
            &path,
            untracked,
            false,
            max_bytes.unwrap_or(DEFAULT_DIFF_BYTES),
        )
    })
    .await
    .map_err(|_| "Git inspection failed".to_string())?
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn git_worktree_handoff_preflight(
    worktree_path: String,
    target_dir: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<HandoffPreflight, String> {
    let ctx = state.ctx(&window);
    let runtime = ctx.runtime();
    tauri::async_runtime::spawn_blocking(move || {
        handoff_preflight(
            &runtime.host,
            Path::new(&worktree_path),
            Path::new(&target_dir),
            &runtime.codex_home,
        )
    })
    .await
    .map_err(|_| "Git inspection failed".to_string())?
}

/// Check the temporary worktree's branch out in `target_dir` and remove the
/// worktree, so the thread can continue in the local checkout.
#[tauri::command]
#[specta::specta]
pub(crate) async fn git_worktree_handoff(
    worktree_path: String,
    target_dir: String,
    commit_uncommitted: bool,
    branch_name: Option<String>,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let ctx = state.ctx(&window);
    let runtime = ctx.runtime();
    let database = ctx.database();
    let worktree_for_db = worktree_path.clone();
    let branch = tauri::async_runtime::spawn_blocking(move || {
        let target = PathBuf::from(&target_dir);
        let common = common_dir_of(&runtime.host, &target)?;
        let guard = lock_for_common_dir(&common);
        let _lock = guard.lock().expect("git common-dir lock poisoned");
        handoff(
            &runtime.host,
            Path::new(&worktree_path),
            &target,
            &runtime.codex_home,
            commit_uncommitted,
            branch_name.as_deref(),
        )
    })
    .await
    .map_err(|_| "Git operation failed".to_string())??;
    let _ = storage::remove_temp_worktree(&database, &worktree_for_db).await;
    Ok(branch)
}

// --- Project-level git actions (stage, commit, sync, branch) ---

/// Run a mutation under the repository's common-dir lock on a blocking task.
async fn locked<T, F>(host: Host, dir: String, work: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(&Host, &Path) -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        let dir = PathBuf::from(&dir);
        let common = common_dir_of(&host, &dir)?;
        let guard = lock_for_common_dir(&common);
        let _lock = guard.lock().expect("git common-dir lock poisoned");
        work(&host, &dir)
    })
    .await
    .map_err(|_| "Git operation failed".to_string())?
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn git_context(
    dir: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<GitContext, String> {
    let host = state.ctx(&window).host();
    tauri::async_runtime::spawn_blocking(move || actions::read_context(&host, Path::new(&dir)))
        .await
        .map_err(|_| "Git inspection failed".to_string())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn git_stage(
    dir: String,
    paths: Vec<String>,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    locked(state.ctx(&window).host(), dir, move |host, dir| {
        actions::stage_paths(host, dir, &paths)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn git_unstage(
    dir: String,
    paths: Vec<String>,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    locked(state.ctx(&window).host(), dir, move |host, dir| {
        actions::unstage_paths(host, dir, &paths)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn git_discard(
    dir: String,
    paths: Vec<String>,
    untracked_paths: Vec<String>,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    locked(state.ctx(&window).host(), dir, move |host, dir| {
        actions::discard_paths(host, dir, &paths, &untracked_paths)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn git_commit(
    dir: String,
    message: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<CommitResult, String> {
    locked(state.ctx(&window).host(), dir, move |host, dir| {
        actions::commit(host, dir, &message)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn git_fetch(
    dir: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<SyncResult, String> {
    locked(state.ctx(&window).host(), dir, actions::fetch).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn git_pull(
    dir: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<SyncResult, String> {
    locked(state.ctx(&window).host(), dir, actions::pull).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn git_push(
    dir: String,
    set_upstream: bool,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<SyncResult, String> {
    locked(state.ctx(&window).host(), dir, move |host, dir| {
        actions::push(host, dir, set_upstream)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn git_checkout_branch(
    dir: String,
    name: String,
    force: bool,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    locked(state.ctx(&window).host(), dir, move |host, dir| {
        actions::checkout_branch(host, dir, &name, force)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn git_create_branch(
    dir: String,
    name: String,
    base: Option<String>,
    checkout: bool,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    locked(state.ctx(&window).host(), dir, move |host, dir| {
        actions::create_branch(host, dir, &name, base.as_deref(), checkout)
    })
    .await
}

/// The index versus HEAD for one path: what a commit would contain.
#[tauri::command]
#[specta::specta]
pub(crate) async fn git_staged_file_diff(
    dir: String,
    path: String,
    max_bytes: Option<usize>,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<FileDiff, String> {
    let host = state.ctx(&window).host();
    tauri::async_runtime::spawn_blocking(move || {
        read_file_diff(
            &host,
            Path::new(&dir),
            "HEAD",
            &path,
            false,
            true,
            max_bytes.unwrap_or(DEFAULT_DIFF_BYTES),
        )
    })
    .await
    .map_err(|_| "Git inspection failed".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::host::Host;
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
        let host = Host::Native;
        let info = read_repo_info(&host, &repo);
        assert!(info.is_git_repo);
        assert_eq!(info.branch.as_deref(), Some("main"));
        assert!(info.common_dir.is_some());

        // A non-git folder reports is_git_repo=false, no error.
        let plain = temp.path().join("plain");
        std::fs::create_dir_all(&plain).unwrap();
        let plain_info = read_repo_info(&host, &plain);
        assert!(!plain_info.is_git_repo);
        assert!(plain_info.error.is_none());

        // Add a linked worktree on a new branch.
        let wt = temp.path().join("wt-feature");
        let add = run_git(
            &host,
            &repo,
            &["worktree", "add", "-b", "feature", wt.to_str().unwrap()],
            WRITE_TIMEOUT,
        )
        .unwrap();
        assert!(add.ok, "worktree add failed: {}", add.stderr);

        // Listing includes main + the linked worktree; neither is Codex-managed
        // because they are not under <codex_home>/worktrees.
        let fake_home = temp.path().join("codex-home");
        let entries = read_worktrees(&host, &repo, &fake_home).unwrap();
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
        let refused = read_status(&host, &wt).unwrap();
        assert!(refused.counts.is_dirty());

        // recent commits are readable.
        let commits = read_recent_commits(&host, &repo, 10).unwrap();
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].subject, "init");

        // Context: the repo is the main checkout, the worktree is linked to it.
        let main_ctx = actions::read_context(&Host::Native, &repo);
        assert_eq!(main_ctx.kind, "main");
        assert!(main_ctx.parent_path.is_none());
        let linked_ctx = actions::read_context(&Host::Native, &wt);
        assert_eq!(linked_ctx.kind, "linked");
        let parent = linked_ctx
            .parent_path
            .expect("linked worktree names its parent");
        assert_eq!(
            std::fs::canonicalize(&parent).unwrap(),
            std::fs::canonicalize(&repo).unwrap()
        );
        let plain_ctx = actions::read_context(&Host::Native, &plain);
        assert_eq!(plain_ctx.kind, "none");
    }

    /// Stage, unstage, commit, discard, branch and sync against a real repo
    /// with a bare remote.
    #[test]
    fn integration_git_actions() {
        if !git_available() {
            eprintln!("skipping: git not available");
            return;
        }
        // The functions under test spawn git themselves, so the isolation the
        // `git()` helper applies has to come from this process's environment.
        for (key, value) in [
            ("GIT_CONFIG_GLOBAL", "/dev/null"),
            ("GIT_CONFIG_SYSTEM", "/dev/null"),
            ("GIT_TERMINAL_PROMPT", "0"),
        ] {
            std::env::set_var(key, value);
        }
        let temp = tempfile::tempdir().expect("temp dir");
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        assert!(git(&repo, &["init", "-q", "-b", "main"]));
        assert!(git(&repo, &["config", "user.name", "Test"]));
        assert!(git(&repo, &["config", "user.email", "test@example.com"]));
        std::fs::write(repo.join("file.txt"), "hello\n").unwrap();
        assert!(git(&repo, &["add", "."]));
        assert!(git(&repo, &["commit", "-q", "-m", "init"]));

        // Stage → staged; unstage → unstaged.
        std::fs::write(repo.join("file.txt"), "hello\nworld\n").unwrap();
        std::fs::write(repo.join("new.txt"), "new\n").unwrap();
        actions::stage_paths(&Host::Native, &repo, &["file.txt".into()]).unwrap();
        let status = read_status(&Host::Native, &repo).unwrap();
        assert_eq!(status.counts.staged, 1);
        assert_eq!(status.counts.untracked, 1);
        let staged = read_file_diff(
            &Host::Native,
            &repo,
            "HEAD",
            "file.txt",
            false,
            true,
            DEFAULT_DIFF_BYTES,
        )
        .unwrap();
        assert!(staged.patch.contains("+world"), "{}", staged.patch);
        actions::unstage_paths(&Host::Native, &repo, &["file.txt".into()]).unwrap();
        let status = read_status(&Host::Native, &repo).unwrap();
        assert_eq!(status.counts.staged, 0);
        assert_eq!(status.counts.unstaged, 1);

        // Paths outside the repository are refused before git runs.
        assert!(actions::stage_paths(&Host::Native, &repo, &["../escape".into()]).is_err());

        // Commit what is staged; the identity is the repo's.
        actions::stage_paths(&Host::Native, &repo, &["file.txt".into(), "new.txt".into()]).unwrap();
        let commit = actions::commit(&Host::Native, &repo, "add world").unwrap();
        assert_eq!(commit.subject, "add world");
        assert_eq!(commit.author_name, "Test");
        assert_eq!(commit.short_hash.len(), 7);
        assert!(read_status(&Host::Native, &repo).unwrap().counts.is_dirty() == false);
        let empty = actions::commit(&Host::Native, &repo, "nothing").unwrap_err();
        assert!(empty.contains("Nothing is staged"), "{empty}");

        // A dirty tree refuses a branch switch unless forced; discard clears it.
        actions::create_branch(&Host::Native, &repo, "feature", None, false).unwrap();
        std::fs::write(repo.join("file.txt"), "dirty\n").unwrap();
        let refused = actions::checkout_branch(&Host::Native, &repo, "feature", false).unwrap_err();
        assert!(refused.starts_with("dirtyTree: "), "{refused}");
        std::fs::write(repo.join("junk.txt"), "x").unwrap();
        actions::discard_paths(
            &Host::Native,
            &repo,
            &["file.txt".into()],
            &["junk.txt".into()],
        )
        .unwrap();
        assert!(!read_status(&Host::Native, &repo).unwrap().counts.is_dirty());
        assert!(!repo.join("junk.txt").exists());
        actions::checkout_branch(&Host::Native, &repo, "feature", false).unwrap();
        assert_eq!(
            read_status(&Host::Native, &repo).unwrap().branch.as_deref(),
            Some("feature")
        );
        let dup = actions::create_branch(&Host::Native, &repo, "feature", None, false).unwrap_err();
        assert!(dup.contains("already exists"), "{dup}");
        assert!(actions::create_branch(&Host::Native, &repo, "bad name", None, false).is_err());

        // No remote: push fails fast with a classified error, never hangs.
        let no_remote = actions::push(&Host::Native, &repo, false).unwrap_err();
        assert!(no_remote.contains(": "), "{no_remote}");

        // Bare remote: publish, fetch, pull.
        let remote = temp.path().join("remote.git");
        std::fs::create_dir_all(&remote).unwrap();
        assert!(git(&remote, &["init", "-q", "--bare", "-b", "main"]));
        assert!(git(
            &repo,
            &["remote", "add", "origin", remote.to_str().unwrap()]
        ));
        let published = actions::push(&Host::Native, &repo, true).unwrap();
        assert_eq!(published.operation, "push");
        assert_eq!(published.upstream.as_deref(), Some("origin/feature"));
        assert!(actions::fetch(&Host::Native, &repo).is_ok());
        let pulled = actions::pull(&Host::Native, &repo).unwrap();
        assert_eq!(pulled.operation, "pull");

        // A second clone pushes first; our push is then non-fast-forward.
        let other = temp.path().join("other");
        assert!(git(
            temp.path(),
            &[
                "clone",
                "-q",
                "-b",
                "feature",
                remote.to_str().unwrap(),
                "other"
            ]
        ));
        assert!(git(&other, &["config", "user.name", "Other"]));
        assert!(git(&other, &["config", "user.email", "other@example.com"]));
        std::fs::write(other.join("other.txt"), "o").unwrap();
        assert!(git(&other, &["add", "."]));
        assert!(git(&other, &["commit", "-q", "-m", "other"]));
        assert!(git(&other, &["push", "-q"]));
        std::fs::write(repo.join("mine.txt"), "m").unwrap();
        actions::stage_paths(&Host::Native, &repo, &["mine.txt".into()]).unwrap();
        actions::commit(&Host::Native, &repo, "mine").unwrap();
        let rejected = actions::push(&Host::Native, &repo, false).unwrap_err();
        assert!(rejected.starts_with("nonFastForward: "), "{rejected}");
    }
}
