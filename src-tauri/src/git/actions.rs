//! User-initiated Git mutations: staging, committing, syncing, branching.
//!
//! Every function takes an explicit directory and an argument array. Paths are
//! repository-relative (as `git status` reports them) and validated so a caller
//! can never reach outside the repository. Network commands disable credential
//! prompts and use the longer `NETWORK_TIMEOUT`. Locking is the caller's job:
//! `commands.rs` takes the per-common-directory lock before calling in here.

use std::path::{Component, Path};

use super::run::{
    classified_error, redact_classified, run_git, run_git_network, GitErrorKind, READ_TIMEOUT,
    WRITE_TIMEOUT,
};
use super::status::{read_repo_info, read_status};
use super::types::{CommitResult, GitContext, SyncResult};
use crate::util::process::CommandOutput;

/// Hook output longer than this is cut; a chatty hook must not flood the UI.
const MAX_HOOK_OUTPUT: usize = 4 * 1024;

/// Repository-relative paths only: no absolute paths, no `..`.
fn validate_paths(paths: &[String]) -> Result<(), String> {
    if paths.is_empty() {
        return Err(classified_error(GitErrorKind::Other, "No files selected"));
    }
    for path in paths {
        let p = Path::new(path);
        let bad = path.is_empty()
            || p.is_absolute()
            || p.components()
                .any(|c| matches!(c, Component::ParentDir | Component::Prefix(_)));
        if bad {
            return Err(classified_error(
                GitErrorKind::Other,
                "File paths must be relative to the repository",
            ));
        }
    }
    Ok(())
}

fn with_paths<'a>(head: &[&'a str], paths: &'a [String]) -> Vec<&'a str> {
    let mut args: Vec<&str> = head.to_vec();
    args.push("--");
    args.extend(paths.iter().map(String::as_str));
    args
}

fn has_head(dir: &Path) -> bool {
    run_git(dir, &["rev-parse", "--verify", "-q", "HEAD"], READ_TIMEOUT)
        .map(|o| o.ok)
        .unwrap_or(false)
}

pub(crate) fn stage_paths(dir: &Path, paths: &[String]) -> Result<(), String> {
    validate_paths(paths)?;
    let output = run_git(dir, &with_paths(&["add", "-A"], paths), WRITE_TIMEOUT)?;
    if !output.ok {
        return Err(redact_classified("Could not stage those files", &output));
    }
    Ok(())
}

pub(crate) fn unstage_paths(dir: &Path, paths: &[String]) -> Result<(), String> {
    validate_paths(paths)?;
    // `restore --staged` needs a HEAD to restore from; an unborn branch can
    // only drop entries from the index.
    let head: &[&str] = if has_head(dir) {
        &["restore", "--staged"]
    } else {
        &["rm", "--cached", "-r", "-q"]
    };
    let output = run_git(dir, &with_paths(head, paths), WRITE_TIMEOUT)?;
    if !output.ok {
        return Err(redact_classified("Could not unstage those files", &output));
    }
    Ok(())
}

/// Throw away working-tree changes: tracked paths are restored from the index
/// (or HEAD when also staged), untracked paths are deleted.
pub(crate) fn discard_paths(
    dir: &Path,
    tracked: &[String],
    untracked: &[String],
) -> Result<(), String> {
    if tracked.is_empty() && untracked.is_empty() {
        return Err(classified_error(GitErrorKind::Other, "No files selected"));
    }
    if !tracked.is_empty() {
        validate_paths(tracked)?;
        let output = run_git(dir, &with_paths(&["checkout"], tracked), WRITE_TIMEOUT)?;
        if !output.ok {
            return Err(redact_classified(
                "Could not discard those changes",
                &output,
            ));
        }
    }
    if !untracked.is_empty() {
        validate_paths(untracked)?;
        let output = run_git(
            dir,
            &with_paths(&["clean", "-f", "-q"], untracked),
            WRITE_TIMEOUT,
        )?;
        if !output.ok {
            return Err(redact_classified("Could not remove those files", &output));
        }
    }
    Ok(())
}

fn config_value(dir: &Path, key: &str) -> Option<String> {
    run_git(dir, &["config", "--get", key], READ_TIMEOUT)
        .ok()
        .filter(|o| o.ok)
        .map(|o| o.stdout.trim().to_string())
        .filter(|v| !v.is_empty())
}

pub(crate) fn commit_identity(dir: &Path) -> (Option<String>, Option<String>) {
    (
        config_value(dir, "user.name"),
        config_value(dir, "user.email"),
    )
}

fn trimmed_output(output: &CommandOutput) -> Option<String> {
    let mut text = String::new();
    for part in [&output.stdout, &output.stderr] {
        let part = part.trim();
        if !part.is_empty() {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(part);
        }
    }
    if text.is_empty() {
        return None;
    }
    if text.len() > MAX_HOOK_OUTPUT {
        let mut cut = MAX_HOOK_OUTPUT;
        while !text.is_char_boundary(cut) {
            cut -= 1;
        }
        text.truncate(cut);
        text.push_str("\n…");
    }
    Some(text)
}

pub(crate) fn commit(dir: &Path, message: &str) -> Result<CommitResult, String> {
    let message = message.trim();
    if message.is_empty() {
        return Err(classified_error(
            GitErrorKind::Other,
            "A commit message is required",
        ));
    }
    let output = run_git(dir, &["commit", "-q", "-m", message], WRITE_TIMEOUT)?;
    if !output.ok {
        // "nothing to commit" is printed on stdout even with `-q`.
        let stderr = format!("{}\n{}", output.stderr, output.stdout).to_lowercase();
        let hint =
            if stderr.contains("please tell me who you are") || stderr.contains("empty ident") {
                classified_error(
                    GitErrorKind::Other,
                    "Git does not know who you are. Set user.name and user.email first.",
                )
            } else if stderr.contains("nothing to commit") || stderr.contains("no changes added") {
                classified_error(GitErrorKind::Other, "Nothing is staged to commit")
            } else {
                redact_classified("Could not create the commit", &output)
            };
        return match trimmed_output(&output) {
            // A hook that refused is the one case where its text is the answer.
            Some(text) if hint.starts_with("hookRejected") => Err(format!("{hint}\n{text}")),
            _ => Err(hint),
        };
    }
    let log = run_git(
        dir,
        &["log", "-1", "--format=%H%x1f%h%x1f%s%x1f%an%x1f%ae"],
        READ_TIMEOUT,
    )?;
    let mut fields = log.stdout.trim_end().split('\x1f').map(str::to_string);
    let hash = fields.next().unwrap_or_default();
    Ok(CommitResult {
        short_hash: fields
            .next()
            .unwrap_or_else(|| hash.chars().take(7).collect()),
        subject: fields.next().unwrap_or_default(),
        author_name: fields.next().unwrap_or_default(),
        author_email: fields.next().unwrap_or_default(),
        hook_output: trimmed_output(&output),
        hash,
    })
}

fn sync_result(dir: &Path, operation: &str, summary: String) -> SyncResult {
    let status = read_status(dir).ok();
    SyncResult {
        operation: operation.to_string(),
        summary,
        upstream: status.as_ref().and_then(|s| s.upstream.clone()),
        ahead: status.as_ref().map(|s| s.ahead).unwrap_or(0),
        behind: status.as_ref().map(|s| s.behind).unwrap_or(0),
    }
}

pub(crate) fn fetch(dir: &Path) -> Result<SyncResult, String> {
    let output = run_git_network(dir, &["fetch", "--prune"])?;
    if !output.ok {
        return Err(redact_classified(
            "Could not fetch from the remote",
            &output,
        ));
    }
    Ok(sync_result(dir, "fetch", "Fetched".to_string()))
}

pub(crate) fn pull(dir: &Path) -> Result<SyncResult, String> {
    let before = read_status(dir).ok();
    let output = run_git_network(dir, &["pull", "--ff-only"])?;
    if !output.ok {
        return Err(redact_classified("Could not pull from the remote", &output));
    }
    let behind = before.map(|s| s.behind).unwrap_or(0);
    let summary = if behind > 0 {
        format!(
            "Fast-forwarded {behind} commit{}",
            if behind == 1 { "" } else { "s" }
        )
    } else if output.stdout.contains("Already up to date") {
        "Already up to date".to_string()
    } else {
        "Pulled".to_string()
    };
    Ok(sync_result(dir, "pull", summary))
}

pub(crate) fn push(dir: &Path, set_upstream: bool) -> Result<SyncResult, String> {
    let output = if set_upstream {
        let status = read_status(dir)?;
        let Some(branch) = status.branch.filter(|_| !status.detached) else {
            return Err(classified_error(
                GitErrorKind::Other,
                "Check out a branch before publishing it",
            ));
        };
        run_git_network(dir, &["push", "-u", "origin", &branch])?
    } else {
        run_git_network(dir, &["push"])?
    };
    if !output.ok {
        return Err(redact_classified("Could not push to the remote", &output));
    }
    let result = sync_result(dir, "push", String::new());
    let summary = match &result.upstream {
        Some(upstream) => format!("Pushed to {upstream}"),
        None => "Pushed".to_string(),
    };
    Ok(SyncResult { summary, ..result })
}

fn validate_branch_name(dir: &Path, name: &str) -> Result<(), String> {
    let check = run_git(dir, &["check-ref-format", "--branch", name], READ_TIMEOUT)?;
    if !check.ok {
        return Err(classified_error(
            GitErrorKind::Other,
            "That is not a valid branch name",
        ));
    }
    Ok(())
}

/// Switch branches. A dirty tree is refused unless `force`, and `force` only
/// lifts that check: git still refuses when local changes would be lost.
pub(crate) fn checkout_branch(dir: &Path, name: &str, force: bool) -> Result<(), String> {
    validate_branch_name(dir, name)?;
    if !force {
        let status = read_status(dir)?;
        if status.counts.is_dirty() {
            return Err(classified_error(
                GitErrorKind::DirtyTree,
                "This checkout has uncommitted changes. Commit or discard them, or switch anyway to carry them over.",
            ));
        }
    }
    let output = run_git(dir, &["switch", name], WRITE_TIMEOUT)?;
    if !output.ok {
        let stderr = output.stderr.to_lowercase();
        if stderr.contains("is already checked out") || stderr.contains("already used by worktree")
        {
            return Err(classified_error(
                GitErrorKind::Other,
                "That branch is already checked out in another worktree",
            ));
        }
        if stderr.contains("invalid reference") || stderr.contains("did not match any") {
            return Err(classified_error(GitErrorKind::Other, "No such branch"));
        }
        return Err(redact_classified("Could not switch branches", &output));
    }
    Ok(())
}

pub(crate) fn create_branch(
    dir: &Path,
    name: &str,
    base: Option<&str>,
    checkout: bool,
) -> Result<(), String> {
    validate_branch_name(dir, name)?;
    let base = base.map(str::trim).filter(|b| !b.is_empty());
    let mut args: Vec<&str> = if checkout {
        vec!["switch", "-c", name]
    } else {
        vec!["branch", name]
    };
    if let Some(base) = base {
        args.push(base);
    }
    let output = run_git(dir, &args, WRITE_TIMEOUT)?;
    if !output.ok {
        let stderr = output.stderr.to_lowercase();
        if stderr.contains("already exists") {
            return Err(classified_error(
                GitErrorKind::Other,
                "A branch with that name already exists",
            ));
        }
        if stderr.contains("not a valid object name") || stderr.contains("invalid reference") {
            return Err(classified_error(
                GitErrorKind::Other,
                "The base revision does not exist",
            ));
        }
        return Err(redact_classified("Could not create the branch", &output));
    }
    Ok(())
}

/// Main checkout, linked worktree, or not a repository, from `rev-parse`:
/// the git dir equals the common dir only for the main working tree.
pub(crate) fn read_context(dir: &Path) -> GitContext {
    let info = read_repo_info(dir);
    let (name, email) = if info.is_git_repo {
        commit_identity(dir)
    } else {
        (None, None)
    };
    let (kind, parent_path) = if !info.is_git_repo {
        ("none", None)
    } else {
        match crate::projects::worktrees::linked_worktree_parent(dir) {
            Ok(parent) => ("linked", Some(parent)),
            Err(_) => ("main", None),
        }
    };
    GitContext {
        dir: info.dir,
        is_git_repo: info.is_git_repo,
        kind: kind.to_string(),
        parent_path,
        root: info.root,
        common_dir: info.common_dir,
        branch: info.branch,
        detached: info.detached,
        upstream: info.upstream,
        ahead: info.ahead,
        behind: info.behind,
        in_progress: info.in_progress,
        identity_name: name,
        identity_email: email,
        error: info.error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_paths_that_leave_the_repository() {
        assert!(validate_paths(&["/etc/passwd".into()]).is_err());
        assert!(validate_paths(&["../x".into()]).is_err());
        assert!(validate_paths(&["a/../../x".into()]).is_err());
        assert!(validate_paths(&[]).is_err());
        assert!(validate_paths(&["src/lib.rs".into(), "a/b".into()]).is_ok());
    }
}
