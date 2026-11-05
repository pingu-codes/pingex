//! The git side of review checkouts: Pingex-owned, detached worktrees at a
//! PR head, and promoting one to a worktree on the PR's git branch in place.
//!
//! A review checkout is `git worktree add --detach` locked with the reason
//! [`REVIEW_CHECKOUT_LOCK`]. The lock keeps `git worktree prune` off it and is
//! how the worktree listing tells it apart (`WorktreeEntry::is_review_checkout`),
//! so it stays out of the Worktrees list and the sidebar nesting. Promotion
//! unlocks it and checks out the git branch where it is: the path, and every
//! thread's cwd in it, stay the same. ADR 0005 has the reasoning.
//!
//! Mutations hold the repository's common-dir lock.

use std::path::Path;

use super::run::{
    common_dir_of, lock_for_common_dir, redact_git_error, run_git, READ_TIMEOUT, WRITE_TIMEOUT,
};
use crate::util::host::Host;

/// The `git worktree lock` reason that marks a review checkout.
pub(crate) const REVIEW_CHECKOUT_LOCK: &str = "pingex review checkout";

fn with_lock<T>(
    host: &Host,
    repo: &Path,
    work: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let common = common_dir_of(host, repo)?;
    let lock = lock_for_common_dir(&common);
    let _guard = lock.lock().expect("git common-dir lock poisoned");
    work()
}

/// Run git and turn a non-zero exit into `failed`, with git's reason.
fn git_ok(host: &Host, dir: &Path, args: &[&str], failed: &str) -> Result<String, String> {
    let output = run_git(host, dir, args, WRITE_TIMEOUT)?;
    if output.ok {
        Ok(output.stdout)
    } else {
        Err(redact_git_error(failed, &output))
    }
}

/// Whether commit `sha` is in the repository.
pub(crate) fn has_commit(host: &Host, repo: &Path, sha: &str) -> Result<bool, String> {
    let spec = format!("{sha}^{{commit}}");
    let output = run_git(host, repo, &["cat-file", "-e", &spec], READ_TIMEOUT)?;
    Ok(output.ok)
}

/// The commit checked out in `dir`.
pub(crate) fn head_of(host: &Host, dir: &Path) -> Result<Option<String>, String> {
    let output = run_git(host, dir, &["rev-parse", "--verify", "HEAD"], READ_TIMEOUT)?;
    Ok(output.ok.then(|| output.stdout.trim().to_string()))
}

/// The git branch checked out in `dir`, or `None` when it is detached (as
/// a review checkout always is) or unreadable.
pub(crate) fn branch_of(host: &Host, dir: &Path) -> Result<Option<String>, String> {
    let output = run_git(
        host,
        dir,
        &["symbolic-ref", "--quiet", "--short", "HEAD"],
        READ_TIMEOUT,
    )?;
    Ok(output
        .ok
        .then(|| output.stdout.trim().to_string())
        .filter(|branch| !branch.is_empty()))
}

/// Whether `dir` has no local changes, untracked files included.
pub(crate) fn is_clean(host: &Host, dir: &Path) -> Result<bool, String> {
    let output = run_git(host, dir, &["status", "--porcelain"], READ_TIMEOUT)?;
    if !output.ok {
        return Err("Could not read the checkout's status".to_string());
    }
    Ok(output.stdout.trim().is_empty())
}

/// Create a review checkout of `repo` at `path`, detached at `sha`.
pub(crate) fn add_review_checkout(
    host: &Host,
    repo: &Path,
    path: &str,
    sha: &str,
) -> Result<(), String> {
    if let Some(parent) = host.parent_str(path) {
        std::fs::create_dir_all(host.to_local(&parent))
            .map_err(|_| "Could not create the review checkouts folder".to_string())?;
    }
    with_lock(host, repo, || {
        git_ok(
            host,
            repo,
            &[
                "worktree",
                "add",
                "--detach",
                "--lock",
                "--reason",
                REVIEW_CHECKOUT_LOCK,
                "--end-of-options",
                path,
                sha,
            ],
            "Could not create the review checkout",
        )
        .map(drop)
    })
}

/// Move the checkout at `dir` to `sha`, detached. Callers check it is clean.
pub(crate) fn detach_at(host: &Host, repo: &Path, dir: &Path, sha: &str) -> Result<(), String> {
    with_lock(host, repo, || {
        git_ok(
            host,
            dir,
            &["switch", "--quiet", "--detach", sha],
            "Could not move the review checkout",
        )
        .map(drop)
    })
}

/// Turn the review checkout at `path` into a worktree on `branch`, in place.
/// A missing local `branch` is created at `sha`.
pub(crate) fn promote_in_place(
    host: &Host,
    repo: &Path,
    path: &str,
    branch: &str,
    sha: &str,
) -> Result<(), String> {
    with_lock(host, repo, || {
        let dir = Path::new(path);
        let exists = run_git(
            host,
            repo,
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("refs/heads/{branch}"),
            ],
            READ_TIMEOUT,
        )?
        .ok;
        let switch: Vec<&str> = if exists {
            vec!["switch", "--quiet", "--end-of-options", branch]
        } else {
            vec!["switch", "--quiet", "-c", branch, sha]
        };
        git_ok(host, dir, &switch, "Could not check out the git branch")?;
        // Unlocked only once the branch is out, so a failed switch leaves a
        // review checkout that still reads as one.
        let _ = run_git(host, repo, &["worktree", "unlock", path], WRITE_TIMEOUT)?;
        Ok(())
    })
}

/// A named worktree at `path` on `branch`, created at `sha` when the branch
/// does not exist locally.
pub(crate) fn add_branch_worktree(
    host: &Host,
    repo: &Path,
    path: &str,
    branch: &str,
    sha: &str,
) -> Result<(), String> {
    if let Some(parent) = host.parent_str(path) {
        std::fs::create_dir_all(host.to_local(&parent))
            .map_err(|_| "Could not create the worktree's folder".to_string())?;
    }
    with_lock(host, repo, || {
        let exists = run_git(
            host,
            repo,
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("refs/heads/{branch}"),
            ],
            READ_TIMEOUT,
        )?
        .ok;
        let args: Vec<&str> = if exists {
            vec!["worktree", "add", "--end-of-options", path, branch]
        } else {
            vec![
                "worktree",
                "add",
                "-b",
                branch,
                "--end-of-options",
                path,
                sha,
            ]
        };
        git_ok(host, repo, &args, "Could not create the worktree").map(drop)
    })
}

/// Remove the review checkout at `path`, local changes and all.
pub(crate) fn remove_review_checkout(host: &Host, repo: &Path, path: &str) -> Result<(), String> {
    with_lock(host, repo, || {
        let _ = run_git(host, repo, &["worktree", "unlock", path], WRITE_TIMEOUT)?;
        let output = run_git(
            host,
            repo,
            &["worktree", "remove", "--force", path],
            WRITE_TIMEOUT,
        )?;
        if output.ok || !host.is_dir(path) {
            // Drop the registration of a directory someone already deleted.
            let _ = run_git(host, repo, &["worktree", "prune"], WRITE_TIMEOUT)?;
            Ok(())
        } else {
            Err(redact_git_error(
                "Could not remove the review checkout",
                &output,
            ))
        }
    })
}

/// The commit `branch` points at now, if it exists.
pub(crate) fn branch_tip(host: &Host, repo: &Path, branch: &str) -> Result<Option<String>, String> {
    let output = run_git(
        host,
        repo,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}^{{commit}}"),
        ],
        READ_TIMEOUT,
    )?;
    Ok(output.ok.then(|| output.stdout.trim().to_string()))
}

/// `(ahead, behind)`: commits `tip` has that `start` lacks, and the reverse.
pub(crate) fn ahead_behind(
    host: &Host,
    repo: &Path,
    start: &str,
    tip: &str,
) -> Result<(i64, i64), String> {
    let range = format!("{start}...{tip}");
    let output = run_git(
        host,
        repo,
        &["rev-list", "--left-right", "--count", &range],
        READ_TIMEOUT,
    )?;
    if !output.ok {
        return Err("Those commits are not in this repository".to_string());
    }
    let mut counts = output
        .stdout
        .split_whitespace()
        .map(|count| count.parse::<i64>().unwrap_or(0));
    let behind = counts.next().unwrap_or(0);
    let ahead = counts.next().unwrap_or(0);
    Ok((ahead, behind))
}

/// The PR numbers with refs under `refs/pingex/pr/`.
pub(crate) fn kept_pr_numbers(host: &Host, repo: &Path) -> Result<Vec<u32>, String> {
    let output = run_git(
        host,
        repo,
        &["for-each-ref", "--format=%(refname)", "refs/pingex/pr/"],
        READ_TIMEOUT,
    )?;
    if !output.ok {
        return Err("Could not read the repository".to_string());
    }
    let mut numbers: Vec<u32> = output
        .stdout
        .lines()
        .filter_map(|name| {
            name.strip_prefix("refs/pingex/pr/")?
                .split('/')
                .next()?
                .parse()
                .ok()
        })
        .collect();
    numbers.sort_unstable();
    numbers.dedup();
    Ok(numbers)
}
