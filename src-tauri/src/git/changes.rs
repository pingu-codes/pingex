//! One file's diff with a hard byte cap (the Git tab), and handing a
//! temporary worktree's branch off to a local checkout.
//!
//! The Changes page reads its listings and patches in `crate::changes`.

use std::path::Path;

use serde::Serialize;

use super::run::{redact_git_error, run_git, run_git_capped, WRITE_TIMEOUT};
use super::status::read_status;
use super::worktrees::worktree_branches;
use crate::util::host::Host;

pub(crate) const DEFAULT_DIFF_BYTES: usize = 256 * 1024;
pub(crate) const MAX_DIFF_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FileDiff {
    pub(crate) path: String,
    pub(crate) patch: String,
    pub(crate) truncated: bool,
    pub(crate) binary: bool,
    /// Bytes actually returned (the child is killed once the cap is reached).
    pub(crate) bytes: usize,
}

fn same_dir(host: &Host, a: &Path, b: &str) -> bool {
    if !host.is_dir(b) {
        return false;
    }
    host.canonical(&host.path_string(a)) == host.canonical(b)
}

/// Run `git diff` for one path, reading at most `max_bytes` of stdout and then
/// killing the child, so a huge generated file costs bounded memory and time.
pub(crate) fn read_file_diff(
    host: &Host,
    dir: &Path,
    base: &str,
    path: &str,
    untracked: bool,
    staged: bool,
    max_bytes: usize,
) -> Result<FileDiff, String> {
    let max_bytes = max_bytes.clamp(1024, MAX_DIFF_BYTES);
    let mut args: Vec<&str> = vec![
        // The path is a file name, not a glob (`[id]` would match `i`, `d`).
        "--literal-pathspecs",
        "diff",
        "--no-ext-diff",
    ];
    // The empty side of an untracked file's diff: the null device as git on
    // that host spells it.
    let null_device = if host.is_wsl() || cfg!(unix) {
        "/dev/null"
    } else {
        "NUL"
    };
    if untracked {
        args.extend(["--no-index", "--", null_device, path]);
    } else if staged {
        args.extend(["--cached", base, "--", path]);
    } else {
        args.extend([base, "--", path]);
    }
    let output = run_git_capped(host, dir, &args, max_bytes)?;
    let truncated = output.truncated;
    let mut patch = output.stdout;
    if truncated {
        // Do not leave a half line dangling in the rendered diff.
        if let Some(cut) = patch.rfind('\n') {
            patch.truncate(cut + 1);
        }
    }
    let binary = patch.contains("Binary files ") && patch.lines().count() < 8;
    Ok(FileDiff {
        path: path.to_string(),
        bytes: patch.len(),
        patch,
        truncated,
        binary,
    })
}

// ---------------------------------------------------------------------------
// Hand off a temporary worktree's branch to a local checkout.

#[derive(Clone, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[serde(rename = "WorktreeHandoffPreflight")]
pub(crate) struct HandoffPreflight {
    pub(crate) branch: Option<String>,
    pub(crate) worktree_dirty: bool,
    pub(crate) target_dirty: bool,
    /// Blocking reason, in the words the dialog shows; `None` when ready.
    pub(crate) blocker: Option<String>,
}

pub(crate) fn handoff_preflight(
    host: &Host,
    worktree: &Path,
    target: &Path,
) -> Result<HandoffPreflight, String> {
    // The first entry of `git worktree list` is the main working tree.
    let entries = worktree_branches(host, worktree)?;
    let position = |dir: &Path| {
        entries
            .iter()
            .position(|(path, _)| same_dir(host, dir, path))
    };
    let index = position(worktree);
    let branch = index.and_then(|index| entries[index].1.clone());
    let worktree_dirty = read_status(host, worktree)
        .map(|status| status.counts.is_dirty())
        .unwrap_or(false);
    let target_status = read_status(host, target);
    let target_dirty = target_status
        .as_ref()
        .map(|status| status.counts.is_dirty())
        .unwrap_or(true);
    let target_index = position(target);
    let same_repo = target_index.is_some();
    let target_is_worktree = target_index.is_some_and(|index| index != 0);
    let blocker = if branch.is_none() {
        Some("This worktree is not on a branch".to_string())
    } else if index == Some(0) {
        Some("This is the main working tree".to_string())
    } else if target_status.is_err() {
        Some("The local workspace is not a Git repository".to_string())
    } else if !same_repo {
        Some("The local workspace is not part of this repository".to_string())
    } else if target_is_worktree {
        Some("Choose the repository itself, not another worktree".to_string())
    } else if target_dirty {
        Some("Stash or commit your local changes to hand off".to_string())
    } else {
        None
    };
    Ok(HandoffPreflight {
        branch,
        worktree_dirty,
        target_dirty,
        blocker,
    })
}

/// Detach the branch from the worktree and check it out in `target`,
/// optionally renaming it to `new_name`. Returns the final branch name.
/// Restores the worktree if the checkout fails.
pub(crate) fn handoff(
    host: &Host,
    worktree: &Path,
    target: &Path,
    commit_uncommitted: bool,
    new_name: Option<&str>,
) -> Result<String, String> {
    let preflight = handoff_preflight(host, worktree, target)?;
    if let Some(blocker) = preflight.blocker {
        return Err(blocker);
    }
    let branch = preflight.branch.expect("preflight guarantees a branch");
    let new_name = new_name
        .map(str::trim)
        .filter(|name| !name.is_empty() && *name != branch);
    if let Some(name) = new_name {
        let valid = run_git(
            host,
            target,
            &["check-ref-format", "--branch", name],
            WRITE_TIMEOUT,
        )?;
        if !valid.ok {
            return Err(format!("\"{name}\" is not a valid branch name"));
        }
        let exists = run_git(
            host,
            target,
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("refs/heads/{name}"),
            ],
            WRITE_TIMEOUT,
        )?;
        if exists.ok {
            return Err(format!("A branch named \"{name}\" already exists"));
        }
    }
    if preflight.worktree_dirty {
        if !commit_uncommitted {
            return Err("This worktree has uncommitted changes".to_string());
        }
        let add = run_git(host, worktree, &["add", "-A"], WRITE_TIMEOUT)?;
        if !add.ok {
            return Err("Could not stage the worktree's changes".to_string());
        }
        let commit = run_git(
            host,
            worktree,
            &[
                "commit",
                "-q",
                "--no-verify",
                "-m",
                "WIP from Codex worktree",
            ],
            WRITE_TIMEOUT,
        )?;
        if !commit.ok {
            return Err("Could not commit the worktree's changes".to_string());
        }
    }
    let worktree_str = host.path_string(worktree);
    let worktree_str = worktree_str.as_str();
    let removed = run_git(
        host,
        target,
        &["worktree", "remove", "--force", worktree_str],
        WRITE_TIMEOUT,
    )?;
    if !removed.ok {
        return Err(redact_git_error(
            "Could not detach the branch from its worktree",
            &removed,
        ));
    }
    let checkout = run_git(host, target, &["checkout", &branch], WRITE_TIMEOUT)?;
    if !checkout.ok {
        // Put the worktree back so nothing is lost.
        let _ = run_git(
            host,
            target,
            &["worktree", "add", worktree_str, &branch],
            WRITE_TIMEOUT,
        );
        return Err(redact_git_error(
            "Could not check the branch out locally",
            &checkout,
        ));
    }
    if let Some(name) = new_name {
        let renamed = run_git(
            host,
            target,
            &["branch", "-m", &branch, name],
            WRITE_TIMEOUT,
        )?;
        if !renamed.ok {
            return Err(redact_git_error("Could not rename the branch", &renamed));
        }
        return Ok(name.to_string());
    }
    Ok(branch)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::test_repo::TestRepo;

    /// The handoff preflight tells a linked worktree from the main one,
    /// from `git worktree list` alone.
    #[test]
    fn preflight_reads_the_worktree_list() {
        let Some(fixture) = TestRepo::new() else {
            return;
        };
        let host = Host::Native;
        let linked = fixture.repo.parent().unwrap().join("linked");
        fixture.git(&[
            "worktree",
            "add",
            "-q",
            "-b",
            "feature",
            linked.to_str().unwrap(),
        ]);

        let from_linked = handoff_preflight(&host, &linked, &fixture.repo).unwrap();
        assert_eq!(from_linked.branch.as_deref(), Some("feature"));
        assert_eq!(from_linked.blocker, None);
        let from_main = handoff_preflight(&host, &fixture.repo, &linked).unwrap();
        assert_eq!(
            from_main.blocker.as_deref(),
            Some("This is the main working tree")
        );
    }

    /// `read_repo_info` skips the untracked scan but still reports the
    /// branch and how far it is from its upstream.
    #[test]
    fn repo_info_reports_branch_and_upstream_distance() {
        let Some(fixture) = TestRepo::new() else {
            return;
        };
        fixture.git(&["branch", "-q", "--set-upstream-to=origin/main"]);
        fixture.commit("ahead.txt", "1\n", "ahead");
        std::fs::write(fixture.repo.join("untracked.txt"), "x").unwrap();
        let info = crate::git::status::read_repo_info(&Host::Native, &fixture.repo);
        assert_eq!(info.branch.as_deref(), Some("main"));
        assert_eq!(info.upstream.as_deref(), Some("origin/main"));
        assert_eq!((info.ahead, info.behind), (1, 0));
    }
}
