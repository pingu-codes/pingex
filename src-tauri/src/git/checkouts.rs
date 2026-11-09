//! The git side of PR worktrees: a real worktree on a local branch at a PR's
//! head, where agents and people can build, run and commit.
//!
//! A PR worktree Pingex manages (someone else's PR, or any fork) is locked with
//! the reason [`PR_WORKTREE_LOCK`]. The lock keeps `git worktree prune` off it
//! and is how the worktree listing tells it apart
//! (`WorktreeEntry::is_managed_pr_worktree`), so cleanup and the sidebar need no
//! second store to keep in step with git. Nothing here discards work: moving a
//! worktree is a fast-forward, and removal refuses local changes and commits
//! that exist nowhere else. ADR 0007 has the reasoning.
//!
//! Mutations hold the repository's common-dir lock.

use std::path::Path;

use super::objects;
use super::run::{
    common_dir_of, lock_for_common_dir, redact_git_error, run_git, run_git_network, READ_TIMEOUT,
    WRITE_TIMEOUT,
};
use crate::util::host::Host;

/// The `git worktree lock` reason that marks a worktree Pingex manages.
pub(crate) const PR_WORKTREE_LOCK: &str = "pingex pr worktree";
/// The reason review checkouts were locked with, before they became PR
/// worktrees. Adoption replaces it (`ReviewDesk::adopt_legacy`).
pub(crate) const LEGACY_REVIEW_CHECKOUT_LOCK: &str = "pingex review checkout";

/// Whether a worktree lock reason says Pingex manages the worktree.
pub(crate) fn is_managed_lock(reason: Option<&str>) -> bool {
    matches!(reason, Some(PR_WORKTREE_LOCK | LEGACY_REVIEW_CHECKOUT_LOCK))
}

/// The local branch of a PR worktree Pingex manages.
pub(crate) fn managed_branch(number: i64) -> String {
    format!("pr/{number}")
}

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

/// The git branch checked out in `dir`, or `None` when it is detached or
/// unreadable.
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

/// Whether `dir` has no local changes, untracked files included. Commits are
/// not changes: see [`commits_outside`] before moving or removing a checkout.
pub(crate) fn is_clean(host: &Host, dir: &Path) -> Result<bool, String> {
    let output = run_git(host, dir, &["status", "--porcelain"], READ_TIMEOUT)?;
    if !output.ok {
        return Err("Could not read the checkout's status".to_string());
    }
    Ok(output.stdout.trim().is_empty())
}

/// How many commits reachable from `tips` are on no remote and not reachable
/// from `kept`. A `kept` commit git cannot find is an error, not a zero.
pub(crate) fn commits_outside(
    host: &Host,
    dir: &Path,
    tips: &[&str],
    kept: &[&str],
) -> Result<i64, String> {
    let mut args = vec!["rev-list", "--count"];
    args.extend_from_slice(tips);
    args.push("--not");
    args.extend_from_slice(kept);
    args.push("--remotes");
    let output = run_git(host, dir, &args, READ_TIMEOUT)?;
    if !output.ok {
        return Err("Could not compare the checkout's commits".to_string());
    }
    output
        .stdout
        .trim()
        .parse()
        .map_err(|_| "Could not compare the checkout's commits".to_string())
}

/// Whether `dir` holds work that would be lost if it and `branch` were
/// removed: local changes, or commits outside `last_head` and the remotes.
/// Anything that cannot be proven safe counts as work.
pub(crate) fn has_local_work(
    host: &Host,
    dir: &Path,
    branch: Option<&str>,
    last_head: Option<&str>,
) -> bool {
    if !matches!(is_clean(host, dir), Ok(true)) {
        return true;
    }
    let Some(last_head) = last_head.and_then(|sha| objects::require_sha(sha).ok()) else {
        return true;
    };
    let branch_ref = branch
        .filter(|branch| matches!(branch_tip(host, dir, branch), Ok(Some(_))))
        .map(|branch| format!("refs/heads/{branch}"));
    let mut tips = vec!["HEAD"];
    tips.extend(branch_ref.as_deref());
    !matches!(commits_outside(host, dir, &tips, &[last_head]), Ok(0))
}

/// The lock reason of the worktree at `dir`, if it is locked.
pub(crate) fn lock_reason(host: &Host, dir: &Path) -> Result<Option<String>, String> {
    let output = run_git(
        host,
        dir,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "locked",
        ],
        READ_TIMEOUT,
    )?;
    if !output.ok {
        return Err("Could not read the worktree".to_string());
    }
    let path = host.to_local(output.stdout.trim());
    Ok(std::fs::read_to_string(path)
        .ok()
        .map(|reason| reason.trim().to_string()))
}

/// Whether Pingex manages the worktree at `dir`.
pub(crate) fn is_managed(host: &Host, dir: &Path) -> Result<bool, String> {
    Ok(is_managed_lock(lock_reason(host, dir)?.as_deref()))
}

/// Whether `dir` is a linked worktree rather than a repository's main clone.
pub(crate) fn is_linked_worktree(host: &Host, dir: &Path) -> Result<bool, String> {
    let output = run_git(
        host,
        dir,
        &["rev-parse", "--git-dir", "--git-common-dir"],
        READ_TIMEOUT,
    )?;
    if !output.ok {
        return Err("Could not read the worktree".to_string());
    }
    let mut lines = output.stdout.lines();
    Ok(lines.next() != lines.next())
}

/// A worktree at `path` on `branch`, at `sha` when the branch is new or merely
/// behind it. A branch with commits `sha` lacks stays as it is. `managed`
/// locks the worktree as one Pingex looks after.
pub(crate) fn add_pr_worktree(
    host: &Host,
    repo: &Path,
    path: &str,
    branch: &str,
    sha: &str,
    managed: bool,
) -> Result<(), String> {
    let sha = objects::require_sha(sha)?;
    if let Some(parent) = host.parent_str(path) {
        std::fs::create_dir_all(host.to_local(&parent))
            .map_err(|_| "Could not create the worktree's folder".to_string())?;
    }
    with_lock(host, repo, || {
        // The directory may be gone with its registration left behind, locked.
        if !host.is_dir(path) {
            let _ = run_git(host, repo, &["worktree", "unlock", path], WRITE_TIMEOUT)?;
            let _ = run_git(host, repo, &["worktree", "prune"], WRITE_TIMEOUT)?;
        }
        let tip = branch_tip(host, repo, branch)?;
        if let Some(tip) = &tip {
            if tip != sha && matches!(objects::is_ancestor(host, repo, tip, sha), Ok(true)) {
                // Refused when it is checked out elsewhere; `worktree add`
                // below then says so.
                let _ = run_git(host, repo, &["branch", "-f", branch, sha], WRITE_TIMEOUT)?;
            }
        }
        let mut args: Vec<&str> = vec!["worktree", "add", "--quiet"];
        if managed {
            args.extend(["--lock", "--reason", PR_WORKTREE_LOCK]);
        }
        if tip.is_some() {
            args.extend(["--end-of-options", path, branch]);
        } else {
            args.extend(["-b", branch, "--end-of-options", path, sha]);
        }
        git_ok(host, repo, &args, "Could not create the worktree").map(drop)
    })
}

/// Make `branch` track its namesake on `remote`, so a plain `git push` from the
/// worktree updates the PR. A branch that already has an upstream keeps it.
pub(crate) fn track_remote_branch(
    host: &Host,
    repo: &Path,
    branch: &str,
    remote: &str,
) -> Result<(), String> {
    let remote_key = format!("branch.{branch}.remote");
    if run_git(host, repo, &["config", "--get", &remote_key], READ_TIMEOUT)?.ok {
        return Ok(());
    }
    // Best effort: the tracking ref lets ahead/behind counts work, and the
    // push works without it.
    let refspec = format!("+refs/heads/{branch}:refs/remotes/{remote}/{branch}");
    let _ = run_git_network(host, repo, &["fetch", "--quiet", remote, &refspec]);
    with_lock(host, repo, || {
        let failed = "Could not set the branch's upstream";
        git_ok(host, repo, &["config", &remote_key, remote], failed)?;
        let merge_key = format!("branch.{branch}.merge");
        let merge = format!("refs/heads/{branch}");
        git_ok(host, repo, &["config", &merge_key, &merge], failed).map(drop)
    })
}

/// Fast-forward the branch checked out in `dir` to `sha`. Callers check that
/// `dir` is clean and that its HEAD is an ancestor of `sha`.
pub(crate) fn fast_forward(host: &Host, repo: &Path, dir: &Path, sha: &str) -> Result<(), String> {
    let sha = objects::require_sha(sha)?;
    with_lock(host, repo, || {
        git_ok(
            host,
            dir,
            &["merge", "--quiet", "--ff-only", sha],
            "Could not update the worktree",
        )
        .map(drop)
    })
}

/// Move the checkout at `dir` to `sha`, detached. Callers check it is clean
/// and holds no commits this would strand.
pub(crate) fn detach_at(host: &Host, repo: &Path, dir: &Path, sha: &str) -> Result<(), String> {
    with_lock(host, repo, || {
        git_ok(
            host,
            dir,
            &["switch", "--quiet", "--detach", sha],
            "Could not move the checkout",
        )
        .map(drop)
    })
}

/// Check out the existing `branch` in `dir`.
pub(crate) fn switch_branch(
    host: &Host,
    repo: &Path,
    dir: &Path,
    branch: &str,
) -> Result<(), String> {
    with_lock(host, repo, || {
        git_ok(
            host,
            dir,
            &["switch", "--quiet", "--end-of-options", branch],
            "Could not check out the git branch",
        )
        .map(drop)
    })
}

/// Turn a review checkout at `path` into a managed PR worktree in place, with
/// its local changes and commits: detached, it gets a branch at its HEAD. The
/// branch it ends up on is returned.
pub(crate) fn adopt_review_checkout(
    host: &Host,
    repo: &Path,
    path: &str,
    number: i64,
) -> Result<String, String> {
    with_lock(host, repo, || {
        let dir = Path::new(path);
        let branch = match branch_of(host, dir)? {
            Some(branch) => branch,
            None => {
                let wanted = managed_branch(number);
                let free = !matches!(branch_tip(host, repo, &wanted), Ok(Some(_)));
                let branch = if free {
                    wanted
                } else {
                    format!("{wanted}-local")
                };
                git_ok(
                    host,
                    dir,
                    &["switch", "--quiet", "-c", &branch],
                    "Could not give the checkout a branch",
                )?;
                branch
            }
        };
        let _ = run_git(host, repo, &["worktree", "unlock", path], WRITE_TIMEOUT)?;
        let _ = run_git(
            host,
            repo,
            &["worktree", "lock", "--reason", PR_WORKTREE_LOCK, path],
            WRITE_TIMEOUT,
        )?;
        Ok(branch)
    })
}

/// Remove the managed PR worktree at `path` and its local `branch`. Callers
/// check [`has_local_work`] first; git still refuses a tree that changed since.
pub(crate) fn remove_pr_worktree(
    host: &Host,
    repo: &Path,
    path: &str,
    branch: Option<&str>,
) -> Result<(), String> {
    with_lock(host, repo, || {
        let _ = run_git(host, repo, &["worktree", "unlock", path], WRITE_TIMEOUT)?;
        let output = run_git(host, repo, &["worktree", "remove", path], WRITE_TIMEOUT)?;
        if output.ok || !host.is_dir(path) {
            // Drop the registration of a directory someone already deleted.
            let _ = run_git(host, repo, &["worktree", "prune"], WRITE_TIMEOUT)?;
            if let Some(branch) = branch {
                let _ = run_git(host, repo, &["branch", "-D", branch], WRITE_TIMEOUT)?;
            }
            Ok(())
        } else {
            let _ = run_git(
                host,
                repo,
                &["worktree", "lock", "--reason", PR_WORKTREE_LOCK, path],
                WRITE_TIMEOUT,
            )?;
            Err(redact_git_error("Could not remove the worktree", &output))
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
