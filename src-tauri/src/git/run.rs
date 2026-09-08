//! Running `git`, and the locking that keeps concurrent mutations apart.
//!
//! Every invocation passes an explicit `-C <dir>` and an argument array (never a
//! shell string). Errors are redacted so raw stderr never leaks paths beyond the
//! repository the caller already knows about.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use crate::util::host::Host;
use crate::util::process::{self, CommandOutput, Run, RunError};

/// Read-only Git commands should return quickly; a slow repository (network
/// filesystem, huge status) is reported as a timeout rather than hanging the UI.
pub(crate) const READ_TIMEOUT: Duration = Duration::from_secs(10);
/// Mutations (worktree add/remove) can legitimately take longer.
pub(crate) const WRITE_TIMEOUT: Duration = Duration::from_secs(60);
/// Network operations (fetch/pull/push) wait on a remote.
pub(crate) const NETWORK_TIMEOUT: Duration = Duration::from_secs(120);

/// Run `git -C <dir> <args...>` on `host` with a timeout. `dir` is a host
/// path. Returns an error when the executable is missing or the command
/// exceeds the timeout; a non-zero exit is surfaced through
/// `CommandOutput::ok` so callers can classify it.
pub(crate) fn run_git(
    host: &Host,
    dir: &Path,
    args: &[&str],
    timeout: Duration,
) -> Result<CommandOutput, String> {
    // `-C <dir>` rather than only a working directory, so git resolves the
    // repository the same way it would from a shell in that folder.
    let dir_str = host.path_string(dir);
    let mut full_args: Vec<&str> = vec!["-C", &dir_str];
    full_args.extend_from_slice(args);

    process::run(Run::on(host, "git", &dir_str, &full_args, timeout)).map_err(|error| match error {
        RunError::NotFound => match host {
            Host::Native => "Git is not installed or not on PATH".to_string(),
            Host::Wsl { distro } => {
                format!("Git is not installed inside WSL ({distro}), or WSL is unavailable")
            }
        },
        RunError::Spawn => "Could not start git".to_string(),
        RunError::Timeout => "git timed out".to_string(),
        RunError::NoOutput => "git did not produce any output".to_string(),
    })
}

/// Run a network command (fetch/pull/push). Credential prompts are disabled so
/// a missing login fails fast with an `Auth` classification instead of
/// hanging until the timeout.
pub(crate) fn run_git_network(
    host: &Host,
    dir: &Path,
    args: &[&str],
) -> Result<CommandOutput, String> {
    let mut full_args: Vec<&str> = vec!["-C"];
    let dir_str = host.path_string(dir);
    full_args.push(&dir_str);
    full_args.extend_from_slice(args);
    let spec = Run {
        host,
        program: "git",
        dir: &dir_str,
        args: &full_args,
        env: &[("GIT_TERMINAL_PROMPT", "0")],
        stdin: None,
        timeout: NETWORK_TIMEOUT,
    };
    process::run(spec).map_err(|error| match error {
        RunError::NotFound => "Git is not installed or not on PATH".to_string(),
        RunError::Spawn => "Could not start git".to_string(),
        RunError::Timeout => "git timed out waiting for the remote".to_string(),
        RunError::NoOutput => "git did not produce any output".to_string(),
    })
}

/// Why a git mutation failed, coarse enough for the UI to pick a hint.
///
/// Commands keep returning `Result<_, String>` like every other command; the
/// kind travels as a `<kind>: ` prefix on the message (see `classified_error`)
/// and the frontend splits it off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GitErrorKind {
    Auth,
    NonFastForward,
    NoUpstream,
    Conflict,
    DirtyTree,
    HookRejected,
    Other,
}

impl GitErrorKind {
    pub(crate) fn tag(self) -> &'static str {
        match self {
            GitErrorKind::Auth => "auth",
            GitErrorKind::NonFastForward => "nonFastForward",
            GitErrorKind::NoUpstream => "noUpstream",
            GitErrorKind::Conflict => "conflict",
            GitErrorKind::DirtyTree => "dirtyTree",
            GitErrorKind::HookRejected => "hookRejected",
            GitErrorKind::Other => "other",
        }
    }
}

/// Classify a failed invocation from its stderr.
pub(crate) fn classify_git_error(output: &CommandOutput) -> GitErrorKind {
    let stderr = output.stderr.to_lowercase();
    let any = |needles: &[&str]| needles.iter().any(|n| stderr.contains(n));
    if any(&[
        "authentication failed",
        "could not read username",
        "could not read password",
        "permission denied (publickey)",
        "terminal prompts disabled",
        "invalid credentials",
    ]) {
        GitErrorKind::Auth
    } else if any(&["non-fast-forward", "fetch first", "[rejected]"]) {
        GitErrorKind::NonFastForward
    } else if any(&[
        "no upstream",
        "no tracking information",
        "has no upstream branch",
    ]) {
        GitErrorKind::NoUpstream
    } else if any(&[
        "automatic merge failed",
        "needs merge",
        "conflict",
        "not possible to fast-forward",
    ]) {
        GitErrorKind::Conflict
    } else if any(&[
        "would be overwritten by",
        "local changes",
        "uncommitted changes",
    ]) {
        GitErrorKind::DirtyTree
    } else if any(&[
        "hook declined",
        "pre-commit hook",
        "commit-msg hook",
        "pre-push hook",
    ]) {
        GitErrorKind::HookRejected
    } else {
        GitErrorKind::Other
    }
}

/// `"<kind>: <message>"` — the wire form of a classified error.
pub(crate) fn classified_error(kind: GitErrorKind, message: &str) -> String {
    format!("{}: {}", kind.tag(), message)
}

/// Redact a failed mutation into a classified, actionable message that never
/// includes raw stderr.
pub(crate) fn redact_classified(fallback: &str, output: &CommandOutput) -> String {
    let kind = classify_git_error(output);
    let message = match kind {
        GitErrorKind::Auth => {
            "Git could not authenticate with the remote. Sign in on the command line or configure a credential helper."
        }
        GitErrorKind::NonFastForward => "The remote has commits you do not have. Pull first, then push again.",
        GitErrorKind::NoUpstream => "This branch has no upstream. Publish it to create one.",
        GitErrorKind::Conflict => "The change could not be applied cleanly. Resolve the conflicts in a terminal.",
        GitErrorKind::DirtyTree => "Uncommitted changes would be overwritten. Commit or discard them first.",
        GitErrorKind::HookRejected => "A Git hook rejected the operation.",
        GitErrorKind::Other => fallback,
    };
    classified_error(kind, message)
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
pub(crate) fn common_dir_of(host: &Host, repo_dir: &Path) -> Result<PathBuf, String> {
    let output = run_git(
        host,
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
    fn classifies_mutation_failures() {
        let cases = [
            (
                "fatal: Authentication failed for 'https://x'",
                GitErrorKind::Auth,
            ),
            (
                "fatal: could not read Username for 'https://x': terminal prompts disabled",
                GitErrorKind::Auth,
            ),
            (
                "! [rejected] main -> main (non-fast-forward)",
                GitErrorKind::NonFastForward,
            ),
            (
                "fatal: The current branch feat has no upstream branch.",
                GitErrorKind::NoUpstream,
            ),
            (
                "There is no tracking information for the current branch.",
                GitErrorKind::NoUpstream,
            ),
            (
                "CONFLICT (content): Merge conflict in a.txt",
                GitErrorKind::Conflict,
            ),
            (
                "fatal: Not possible to fast-forward, aborting.",
                GitErrorKind::Conflict,
            ),
            (
                "error: Your local changes to the following files would be overwritten by checkout",
                GitErrorKind::DirtyTree,
            ),
            (
                "error: failed to push some refs\nremote: hook declined",
                GitErrorKind::HookRejected,
            ),
            ("fatal: something else", GitErrorKind::Other),
        ];
        for (stderr, expected) in cases {
            assert_eq!(classify_git_error(&output(stderr)), expected, "{stderr}");
        }
    }

    #[test]
    fn classified_errors_carry_a_kind_prefix_and_no_stderr() {
        let redacted =
            redact_classified("Could not push", &output("fatal: /secret non-fast-forward"));
        assert!(redacted.starts_with("nonFastForward: "));
        assert!(!redacted.contains("/secret"));
        let other = redact_classified("Could not push", &output("fatal: /secret"));
        assert_eq!(other, "other: Could not push");
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
