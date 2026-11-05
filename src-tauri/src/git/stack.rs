//! The git side of stack edits: counting and listing commits between two
//! points, replaying a git branch onto a new parent, squashing one git branch
//! into another, a trial merge that predicts conflicts, putting git branches
//! back where they were, and pushing several git branches atomically against
//! leases.
//!
//! None of these take the repository's mutation lock; a caller running a
//! sequence holds it around the whole sequence with [`locked`] (the lock is
//! not reentrant, so nothing called inside may take it again).

use std::path::Path;

use super::run::{
    common_dir_of, lock_for_common_dir, redact_git_error, run_git, run_git_network,
    run_git_with_input, READ_TIMEOUT, WRITE_TIMEOUT,
};
use crate::review::stack_types::CommitLine;
use crate::util::host::Host;

/// Run `work` holding `repo`'s mutation lock.
pub(crate) fn locked<T>(
    host: &Host,
    repo: &Path,
    work: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let common = common_dir_of(host, repo)?;
    let lock = lock_for_common_dir(&common);
    let _guard = lock.lock().expect("git common-dir lock poisoned");
    work()
}

fn git_ok(host: &Host, dir: &Path, args: &[&str], failed: &str) -> Result<String, String> {
    let output = run_git(host, dir, args, WRITE_TIMEOUT)?;
    if output.ok {
        Ok(output.stdout)
    } else {
        Err(redact_git_error(failed, &output))
    }
}

/// Commits reachable from `to` and not from `from`.
pub(crate) fn count_between(host: &Host, repo: &Path, from: &str, to: &str) -> Result<i64, String> {
    let range = format!("{from}..{to}");
    let output = run_git(host, repo, &["rev-list", "--count", &range], READ_TIMEOUT)?;
    if !output.ok {
        return Err("Those commits are not in this repository".to_string());
    }
    Ok(output.stdout.trim().parse().unwrap_or(0))
}

/// The commits in `from..to`, newest first, at most `max`.
pub(crate) fn commits_between(
    host: &Host,
    repo: &Path,
    from: &str,
    to: &str,
    max: usize,
) -> Result<Vec<CommitLine>, String> {
    let range = format!("{from}..{to}");
    let limit = format!("--max-count={max}");
    let output = run_git(
        host,
        repo,
        &["log", "--format=%H%x09%s", &limit, &range],
        READ_TIMEOUT,
    )?;
    if !output.ok {
        return Err("Those commits are not in this repository".to_string());
    }
    Ok(output
        .stdout
        .lines()
        .filter_map(|line| {
            let (sha, subject) = line.split_once('\t')?;
            Some(CommitLine {
                sha: sha.to_string(),
                subject: subject.to_string(),
            })
        })
        .collect())
}

pub(crate) fn merge_base(
    host: &Host,
    repo: &Path,
    a: &str,
    b: &str,
) -> Result<Option<String>, String> {
    let output = run_git(host, repo, &["merge-base", a, b], READ_TIMEOUT)?;
    Ok(output
        .ok
        .then(|| output.stdout.trim().to_string())
        .filter(|sha| !sha.is_empty()))
}

/// Whether `ancestor` is reachable from `descendant` (either may be a ref).
pub(crate) fn contains(
    host: &Host,
    repo: &Path,
    ancestor: &str,
    descendant: &str,
) -> Result<bool, String> {
    let output = run_git(
        host,
        repo,
        &["merge-base", "--is-ancestor", ancestor, descendant],
        READ_TIMEOUT,
    )?;
    match output.code {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Err("Those commits are not in this repository".to_string()),
    }
}

/// How far back `integrates` looks in a git branch's reflog.
const REFLOG_ENTRIES_CHECKED: usize = 50;

/// Whether local git branch `branch` has had commit `sha`: it is in the
/// tip's history, or in the history of a tip the git branch had before (its
/// reflog), as when a restack rewrote it on purpose. The same test as
/// `push --force-if-includes`, which an explicit lease turns off.
pub(crate) fn integrates(
    host: &Host,
    repo: &Path,
    branch: &str,
    sha: &str,
) -> Result<bool, String> {
    let reference = format!("refs/heads/{branch}");
    if contains(host, repo, sha, &reference)? {
        return Ok(true);
    }
    let output = run_git(
        host,
        repo,
        &["reflog", "show", "--format=%H", &reference],
        READ_TIMEOUT,
    )?;
    if !output.ok {
        return Ok(false);
    }
    let mut seen = std::collections::HashSet::new();
    for entry in output
        .stdout
        .lines()
        .filter(|entry| seen.insert(*entry))
        .take(REFLOG_ENTRIES_CHECKED)
    {
        if entry == sha || contains(host, repo, sha, entry).unwrap_or(false) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Files `from..to` changes.
pub(crate) fn changed_files(
    host: &Host,
    repo: &Path,
    from: &str,
    to: &str,
) -> Result<Vec<String>, String> {
    let output = run_git(
        host,
        repo,
        &["diff", "--name-only", "--no-renames", from, to],
        READ_TIMEOUT,
    )?;
    if !output.ok {
        return Err("Those commits are not in this repository".to_string());
    }
    Ok(output.stdout.lines().map(str::to_string).collect())
}

/// The files a merge of `theirs` into `ours` over `base` would leave
/// conflicted, without touching any checkout. `None` when this git cannot
/// do a trial merge (`merge-tree --merge-base` needs 2.40).
pub(crate) fn predict_conflicts(
    host: &Host,
    repo: &Path,
    base: &str,
    ours: &str,
    theirs: &str,
) -> Result<Option<Vec<String>>, String> {
    let merge_base = format!("--merge-base={base}");
    let output = run_git(
        host,
        repo,
        &[
            "merge-tree",
            "--write-tree",
            "--name-only",
            "--no-messages",
            &merge_base,
            ours,
            theirs,
        ],
        WRITE_TIMEOUT,
    )?;
    match output.code {
        Some(0) => Ok(Some(Vec::new())),
        // The first line is the tree; conflicted paths follow until a blank.
        Some(1) => Ok(Some(
            output
                .stdout
                .lines()
                .skip(1)
                .take_while(|line| !line.is_empty())
                .map(str::to_string)
                .collect(),
        )),
        _ => Ok(None),
    }
}

/// How replaying commits ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Replay {
    Done,
    /// Stopped with these files unmerged; the rebase is still in progress.
    Conflict(Vec<String>),
}

/// The files left unmerged in `dir`.
pub(crate) fn conflicted_files(host: &Host, dir: &Path) -> Result<Vec<String>, String> {
    let output = run_git(
        host,
        dir,
        &["diff", "--name-only", "--diff-filter=U"],
        READ_TIMEOUT,
    )?;
    Ok(output.stdout.lines().map(str::to_string).collect())
}

fn replay_outcome(
    host: &Host,
    dir: &Path,
    output: &crate::util::process::CommandOutput,
    failed: &str,
) -> Result<Replay, String> {
    if output.ok {
        return Ok(Replay::Done);
    }
    let files = conflicted_files(host, dir)?;
    if !files.is_empty() && rebase_in_progress(host, dir) {
        return Ok(Replay::Conflict(files));
    }
    if rebase_in_progress(host, dir) {
        let _ = run_git(host, dir, &["rebase", "--abort"], WRITE_TIMEOUT);
    }
    Err(redact_git_error(failed, output))
}

/// Replay `branch`'s commits after `upstream` onto `onto`, in the worktree
/// at `dir` (which ends up on `branch`). Never opens an editor.
pub(crate) fn replay_onto(
    host: &Host,
    dir: &Path,
    onto: &str,
    upstream: &str,
    branch: &str,
) -> Result<Replay, String> {
    let output = run_git(
        host,
        dir,
        &[
            "-c",
            "core.editor=true",
            "rebase",
            "--no-autostash",
            "--onto",
            onto,
            upstream,
            branch,
        ],
        WRITE_TIMEOUT,
    )?;
    replay_outcome(host, dir, &output, "Replaying the git branch failed")
}

/// Continue a rebase stopped at a conflict the user has resolved.
pub(crate) fn continue_replay(host: &Host, dir: &Path) -> Result<Replay, String> {
    // A text conflict both sides changed counts as resolved once it has no
    // conflict markers left; staging it is done here, so an editor's save is
    // enough. Any other conflict (a binary file, a deletion on one side) has
    // no markers to look for, and git has left one side in place: it stays
    // conflicted until the user stages their choice.
    let unmerged = unmerged_paths(host, dir)?;
    let base = host.path_string(dir);
    let still: Vec<String> = unmerged
        .iter()
        .filter(|(path, both_sides)| {
            !(*both_sides && edited_clean(&host.to_local(&host.join_str(&base, path))))
        })
        .map(|(path, _)| path.clone())
        .collect();
    if !still.is_empty() {
        return Ok(Replay::Conflict(still));
    }
    if !unmerged.is_empty() {
        let mut args = vec!["add", "--"];
        args.extend(unmerged.iter().map(|(path, _)| path.as_str()));
        git_ok(host, dir, &args, "Staging the resolution failed")?;
    }
    let output = run_git(
        host,
        dir,
        &["-c", "core.editor=true", "rebase", "--continue"],
        WRITE_TIMEOUT,
    )?;
    replay_outcome(host, dir, &output, "Continuing the restack failed")
}

/// The unmerged paths in `dir`, sorted, each with whether both sides
/// changed it (index stages 2 and 3 are both there).
fn unmerged_paths(host: &Host, dir: &Path) -> Result<Vec<(String, bool)>, String> {
    let output = run_git(host, dir, &["ls-files", "-u", "-z"], READ_TIMEOUT)?;
    if !output.ok {
        return Err("Could not read the conflicted files".to_string());
    }
    let mut paths: Vec<(String, [bool; 2])> = Vec::new();
    for entry in output.stdout.split('\0').filter(|entry| !entry.is_empty()) {
        // `<mode> <sha> <stage>\t<path>`
        let Some((meta, path)) = entry.split_once('\t') else {
            continue;
        };
        let stage = meta.rsplit(' ').next().unwrap_or("");
        let index = match paths.iter().position(|(known, _)| known == path) {
            Some(index) => index,
            None => {
                paths.push((path.to_string(), [false; 2]));
                paths.len() - 1
            }
        };
        match stage {
            "2" => paths[index].1[0] = true,
            "3" => paths[index].1[1] = true,
            _ => {}
        }
    }
    paths.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(paths
        .into_iter()
        .map(|(path, [ours, theirs])| (path, ours && theirs))
        .collect())
}

/// Whether the file at `local` is text with no conflict markers left.
fn edited_clean(local: &Path) -> bool {
    let Ok(bytes) = std::fs::read(local) else {
        return false;
    };
    if bytes.contains(&0) {
        return false;
    }
    std::str::from_utf8(&bytes)
        .is_ok_and(|text| !text.lines().any(|line| line.starts_with("<<<<<<< ")))
}

pub(crate) fn rebase_in_progress(host: &Host, dir: &Path) -> bool {
    ["rebase-merge", "rebase-apply"].iter().any(|name| {
        run_git(
            host,
            dir,
            &["rev-parse", "--path-format=absolute", "--git-path", name],
            READ_TIMEOUT,
        )
        .ok()
        .filter(|output| output.ok)
        .is_some_and(|output| host.is_dir(output.stdout.trim()))
    })
}

pub(crate) fn abort_replay(host: &Host, dir: &Path) -> Result<(), String> {
    if rebase_in_progress(host, dir) {
        git_ok(
            host,
            dir,
            &["rebase", "--abort"],
            "Stopping the rebase failed",
        )?;
    }
    Ok(())
}

/// The tip of local git branch `name`.
pub(crate) fn branch_tip(host: &Host, repo: &Path, name: &str) -> Result<Option<String>, String> {
    super::checkouts::branch_tip(host, repo, name)
}

/// Point every `(git branch, sha)` in one transaction, creating any that are
/// missing. A worktree with one of them checked out must be detached first.
pub(crate) fn set_branches(
    host: &Host,
    repo: &Path,
    branches: &[(String, String)],
) -> Result<(), String> {
    if branches.is_empty() {
        return Ok(());
    }
    let mut tx = String::from("start\n");
    for (name, sha) in branches {
        tx.push_str(&format!("update refs/heads/{name} {sha}\n"));
    }
    tx.push_str("prepare\ncommit\n");
    let output = run_git_with_input(host, repo, &["update-ref", "--stdin"], &tx)?;
    if output.ok {
        Ok(())
    } else {
        Err(redact_git_error(
            "Could not put the git branches back",
            &output,
        ))
    }
}

/// Leave `dir` detached at its current commit.
pub(crate) fn detach(host: &Host, dir: &Path) -> Result<(), String> {
    git_ok(
        host,
        dir,
        &["switch", "--detach", "-q"],
        "Detaching the worktree failed",
    )
    .map(drop)
}

pub(crate) fn switch(host: &Host, dir: &Path, branch: &str) -> Result<(), String> {
    git_ok(
        host,
        dir,
        &["switch", "-q", "--no-guess", branch],
        &format!("Switching to {branch} failed"),
    )
    .map(drop)
}

/// Throw away the checkout's changes and match `HEAD`: only after a
/// failed operation in a worktree that was clean when it started.
pub(crate) fn reset_hard(host: &Host, dir: &Path) -> Result<(), String> {
    git_ok(
        host,
        dir,
        &["reset", "-q", "--hard"],
        "Resetting the worktree failed",
    )
    .map(drop)
}

/// Move local git branch `name` from `from` up to `to`, only if it is still
/// at `from`. No worktree may have it checked out.
pub(crate) fn fast_forward(
    host: &Host,
    repo: &Path,
    name: &str,
    from: &str,
    to: &str,
) -> Result<(), String> {
    let reference = format!("refs/heads/{name}");
    git_ok(
        host,
        repo,
        &["update-ref", &reference, to, from],
        &format!("Moving {name} up to GitHub's head failed"),
    )
    .map(drop)
}

pub(crate) fn create_branch(host: &Host, repo: &Path, name: &str, at: &str) -> Result<(), String> {
    git_ok(
        host,
        repo,
        &["branch", "--no-track", name, at],
        &format!("Creating {name} failed"),
    )
    .map(drop)
}

pub(crate) fn delete_branch(host: &Host, repo: &Path, name: &str) -> Result<(), String> {
    git_ok(
        host,
        repo,
        &["branch", "-D", name],
        &format!("Deleting {name} failed"),
    )
    .map(drop)
}

pub(crate) fn rename_branch(host: &Host, repo: &Path, from: &str, to: &str) -> Result<(), String> {
    git_ok(
        host,
        repo,
        &["branch", "-m", from, to],
        &format!("Renaming {from} failed"),
    )
    .map(drop)
}

/// After local git branch `from` was renamed to `to` on the remote as well:
/// point `to`'s upstream at the new name, if it tracked the old one.
pub(crate) fn follow_renamed_upstream(
    host: &Host,
    repo: &Path,
    from: &str,
    to: &str,
) -> Result<(), String> {
    let key = format!("branch.{to}.merge");
    let output = run_git(host, repo, &["config", "--get", &key], READ_TIMEOUT)?;
    if !output.ok || output.stdout.trim() != format!("refs/heads/{from}") {
        return Ok(());
    }
    git_ok(
        host,
        repo,
        &["config", &key, &format!("refs/heads/{to}")],
        &format!("Pointing {to} at its new upstream failed"),
    )
    .map(drop)
}

/// Squash `branch`'s changes over `parent` into one commit on `parent`, in
/// the worktree at `dir` (which ends up on `parent`). `parent` must be an
/// ancestor of `branch`.
pub(crate) fn squash_into(
    host: &Host,
    dir: &Path,
    parent: &str,
    branch: &str,
    message: &str,
) -> Result<(), String> {
    switch(host, dir, parent)?;
    git_ok(
        host,
        dir,
        &["merge", "--squash", "-q", branch],
        &format!("Squashing {branch} failed"),
    )?;
    git_ok(
        host,
        dir,
        &["commit", "-q", "--no-verify", "-m", message],
        &format!("Committing {branch}'s changes failed"),
    )
    .map(drop)
}

/// One git branch to push, and the sha the remote must still have for it
/// (`None`: the remote must not have it yet).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LeasedPush {
    pub(crate) branch: String,
    pub(crate) lease: Option<String>,
}

/// Push every git branch in `pushes` to `remote` in one atomic push, each
/// only if the remote still has its lease.
pub(crate) fn push_leased(
    host: &Host,
    dir: &Path,
    remote: &str,
    pushes: &[LeasedPush],
) -> Result<(), String> {
    if pushes.is_empty() {
        return Ok(());
    }
    let mut args: Vec<String> = vec!["push".into(), "--atomic".into(), "--porcelain".into()];
    for push in pushes {
        args.push(format!(
            "--force-with-lease=refs/heads/{}:{}",
            push.branch,
            push.lease.as_deref().unwrap_or("")
        ));
    }
    args.push("--force-if-includes".into());
    args.push(remote.to_string());
    for push in pushes {
        args.push(format!("refs/heads/{0}:refs/heads/{0}", push.branch));
    }
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let output = run_git_network(host, dir, &args)?;
    if output.ok {
        Ok(())
    } else if output.stdout.contains("stale info") || output.stderr.contains("stale info") {
        Err("GitHub has commits on one of these git branches that are not here. Sync, then try again".to_string())
    } else {
        Err(redact_git_error("Pushing the stack failed", &output))
    }
}
