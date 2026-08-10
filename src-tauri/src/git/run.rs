//! Running `git`, and the locking that keeps concurrent mutations apart.
//!
//! Every invocation passes an explicit `-C <dir>` and an argument array (never a
//! shell string). Errors are redacted so raw stderr never leaks paths beyond the
//! repository the caller already knows about.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use crate::util::process::{self, CommandOutput, Run, RunError};

/// Read-only Git commands should return quickly; a slow repository (network
/// filesystem, huge status) is reported as a timeout rather than hanging the UI.
pub(crate) const READ_TIMEOUT: Duration = Duration::from_secs(10);
/// Mutations (worktree add/remove) can legitimately take longer.
pub(crate) const WRITE_TIMEOUT: Duration = Duration::from_secs(60);

/// Run `git -C <dir> <args...>` with a timeout. Returns an error when the
/// executable is missing or the command exceeds the timeout; a non-zero exit is
/// surfaced through `CommandOutput::ok` so callers can classify it.
pub(crate) fn run_git(
    dir: &Path,
    args: &[&str],
    timeout: Duration,
) -> Result<CommandOutput, String> {
    // `-C <dir>` rather than only a working directory, so git resolves the
    // repository the same way it would from a shell in that folder.
    let mut full_args: Vec<&str> = vec!["-C"];
    let dir_str = dir.to_str().unwrap_or_default();
    full_args.push(dir_str);
    full_args.extend_from_slice(args);

    process::run(Run::new("git", dir, &full_args, timeout)).map_err(|error| match error {
        RunError::NotFound => "Git is not installed or not on PATH".to_string(),
        RunError::Spawn => "Could not start git".to_string(),
        RunError::Timeout => "git timed out".to_string(),
        RunError::NoOutput => "git did not produce any output".to_string(),
    })
}

/// Redact a `git` failure, promoting a few well-known messages to an actionable
/// form and otherwise using a generic fallback.
pub(crate) fn redact_git_error(fallback: &str, output: &CommandOutput) -> String {
    let stderr = output.stderr.to_lowercase();
    if stderr.contains("is already checked out") {
        "That branch is already checked out in another worktree".to_string()
    } else if stderr.contains("already exists") {
        "A worktree already exists at that location".to_string()
    } else if stderr.contains("contains modified or untracked files") {
        "This worktree has uncommitted changes".to_string()
    } else if stderr.contains("is not a working tree") {
        "That path is not a registered worktree".to_string()
    } else {
        fallback.to_string()
    }
}

/// Per-common-directory mutation locks. Two mutating operations against the
/// same repository (which may target different linked worktrees but share a
/// common Git dir) are serialized; unrelated repositories run concurrently.
fn common_dir_locks() -> &'static Mutex<HashMap<PathBuf, Arc<Mutex<()>>>> {
    static LOCKS: OnceLock<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>> = OnceLock::new();
    LOCKS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(crate) fn lock_for_common_dir(common_dir: &Path) -> Arc<Mutex<()>> {
    let mut map = common_dir_locks().lock().expect("git lock map poisoned");
    map.entry(common_dir.to_path_buf())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

/// Resolve the common Git dir for a repository so mutations can be serialized.
pub(crate) fn common_dir_of(repo_dir: &Path) -> Result<PathBuf, String> {
    let output = run_git(
        repo_dir,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        READ_TIMEOUT,
    )?;
    if !output.ok {
        return Err("This folder is not a Git repository".to_string());
    }
    let common = output.stdout.lines().next().unwrap_or("").trim();
    if common.is_empty() {
        return Err("This folder is not a Git repository".to_string());
    }
    Ok(PathBuf::from(common))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn output(stderr: &str) -> CommandOutput {
        CommandOutput {
            ok: false,
            stdout: String::new(),
            stderr: stderr.to_string(),
        }
    }

    #[test]
    fn known_git_failures_become_actionable_messages() {
        assert_eq!(
            redact_git_error(
                "fallback",
                &output("fatal: 'main' is already checked out at ...")
            ),
            "That branch is already checked out in another worktree"
        );
        assert_eq!(
            redact_git_error("fallback", &output("fatal: '/wt' already exists")),
            "A worktree already exists at that location"
        );
        assert_eq!(
            redact_git_error("fallback", &output("contains modified or untracked files")),
            "This worktree has uncommitted changes"
        );
    }

    #[test]
    fn an_unrecognised_failure_never_leaks_stderr() {
        let redacted = redact_git_error("Could not do the thing", &output("fatal: /secret/path"));
        assert_eq!(redacted, "Could not do the thing");
        assert!(!redacted.contains("/secret/path"));
    }

    #[test]
    fn the_same_common_dir_shares_one_lock() {
        let one = lock_for_common_dir(Path::new("/repo/.git"));
        let same = lock_for_common_dir(Path::new("/repo/.git"));
        let other = lock_for_common_dir(Path::new("/elsewhere/.git"));
        assert!(Arc::ptr_eq(&one, &same));
        assert!(!Arc::ptr_eq(&one, &other));
    }
}
