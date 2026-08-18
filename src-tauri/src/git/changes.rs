//! The "Changes" view: what a directory has changed relative to its base.
//!
//! Split into a cheap summary (`--numstat`, never diff bodies) and a per-file
//! diff read with a hard byte cap, so a repository full of generated samples
//! never makes the backend buffer, or the frontend render, a giant diff.

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Instant;

use serde::Serialize;

use super::run::{redact_git_error, run_git, READ_TIMEOUT, WRITE_TIMEOUT};
use super::status::read_status;
use super::worktrees::read_worktrees;

pub(crate) const MAX_CHANGED_FILES: usize = 2000;
pub(crate) const DEFAULT_DIFF_BYTES: usize = 256 * 1024;
pub(crate) const MAX_DIFF_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChangedFile {
    pub(crate) path: String,
    pub(crate) old_path: Option<String>,
    /// `added`, `modified`, `deleted`, `renamed`, or `untracked`.
    pub(crate) status: String,
    pub(crate) additions: u64,
    pub(crate) deletions: u64,
    pub(crate) binary: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChangesSummary {
    /// The revision everything is compared against (`HEAD` or a merge-base).
    pub(crate) base: String,
    /// The branch the base was derived from, when this is a linked worktree.
    pub(crate) base_branch: Option<String>,
    pub(crate) files: Vec<ChangedFile>,
    pub(crate) truncated: bool,
    pub(crate) total_files: usize,
    pub(crate) additions: u64,
    pub(crate) deletions: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FileDiff {
    pub(crate) path: String,
    pub(crate) patch: String,
    pub(crate) truncated: bool,
    pub(crate) binary: bool,
    /// Bytes actually returned (the child is killed once the cap is reached).
    pub(crate) bytes: usize,
}

fn same_dir(a: &Path, b: &str) -> bool {
    let a = std::fs::canonicalize(a).unwrap_or_else(|_| a.to_path_buf());
    std::fs::canonicalize(b).map(|p| p == a).unwrap_or(false)
}

/// Resolve what to diff against: for a linked worktree, the merge-base with
/// the main worktree's branch; otherwise `HEAD`.
pub(crate) fn resolve_base(dir: &Path, codex_home: &Path) -> (String, Option<String>) {
    let head = || ("HEAD".to_string(), None);
    let Ok(entries) = read_worktrees(dir, codex_home) else {
        return head();
    };
    let main = entries.iter().find(|entry| entry.is_main);
    let is_main = main.map(|m| same_dir(dir, &m.path)).unwrap_or(true);
    if is_main {
        return head();
    }
    let Some(parent) = main.and_then(|entry| entry.branch.clone()) else {
        return head();
    };
    match run_git(dir, &["merge-base", "HEAD", &parent], READ_TIMEOUT) {
        Ok(output) if output.ok && !output.stdout.trim().is_empty() => {
            (output.stdout.trim().to_string(), Some(parent))
        }
        _ => head(),
    }
}

fn parse_numstat(stdout: &str) -> Vec<ChangedFile> {
    // `-z` output: `add\tdel\tpath\0` or, for renames, `add\tdel\t\0old\0new\0`.
    let mut files = Vec::new();
    let mut parts = stdout.split('\0');
    while let Some(record) = parts.next() {
        if record.is_empty() {
            continue;
        }
        let mut columns = record.splitn(3, '\t');
        let add = columns.next().unwrap_or("");
        let del = columns.next().unwrap_or("");
        let binary = add == "-";
        let additions = add.parse().unwrap_or(0);
        let deletions = del.parse().unwrap_or(0);
        let (path, old_path, status) = match columns.next() {
            Some(path) if !path.is_empty() => (path.to_string(), None, "modified"),
            _ => {
                let old = parts.next().unwrap_or("").to_string();
                let new = parts.next().unwrap_or("").to_string();
                (new, Some(old), "renamed")
            }
        };
        files.push(ChangedFile {
            path,
            old_path,
            status: status.into(),
            additions,
            deletions,
            binary,
        });
    }
    files
}

/// `git diff --name-status` to tell added/deleted from modified.
fn read_name_status(dir: &Path, base: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Ok(output) = run_git(
        dir,
        &["diff", "--name-status", "-z", "-M", "--no-color", base],
        READ_TIMEOUT,
    ) else {
        return map;
    };
    let mut parts = output.stdout.split('\0');
    while let Some(code) = parts.next() {
        if code.is_empty() {
            continue;
        }
        let path = parts.next().unwrap_or("").to_string();
        let status = match code.chars().next() {
            Some('A') => "added",
            Some('D') => "deleted",
            Some('R') => {
                let new = parts.next().unwrap_or("").to_string();
                map.insert(new, "renamed".into());
                continue;
            }
            _ => "modified",
        };
        map.insert(path, status.into());
    }
    map
}

fn read_untracked(dir: &Path) -> Vec<String> {
    let Ok(output) = run_git(
        dir,
        &["ls-files", "--others", "--exclude-standard", "-z"],
        READ_TIMEOUT,
    ) else {
        return Vec::new();
    };
    output
        .stdout
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .collect()
}

pub(crate) fn read_changes_summary(
    dir: &Path,
    codex_home: &Path,
) -> Result<ChangesSummary, String> {
    let (base, base_branch) = resolve_base(dir, codex_home);
    let output = run_git(
        dir,
        &[
            "-c",
            "core.quotepath=off",
            "diff",
            "--numstat",
            "-z",
            "-M",
            "--no-color",
            &base,
        ],
        READ_TIMEOUT,
    )?;
    if !output.ok {
        return Err("Could not read changes for this directory".to_string());
    }
    let mut files = parse_numstat(&output.stdout);
    let statuses = read_name_status(dir, &base);
    for file in &mut files {
        if let Some(status) = statuses.get(&file.path) {
            file.status = status.clone();
        }
    }
    let untracked = read_untracked(dir);
    // Only sample line counts for a bounded number of untracked files; a
    // generated corpus of thousands of files must not cost thousands of reads.
    for (index, path) in untracked.into_iter().enumerate() {
        let (additions, binary) = if index < 200 {
            untracked_line_count(&dir.join(&path))
        } else {
            (0, false)
        };
        files.push(ChangedFile {
            path,
            old_path: None,
            status: "untracked".into(),
            additions,
            deletions: 0,
            binary,
        });
    }
    let total_files = files.len();
    let truncated = total_files > MAX_CHANGED_FILES;
    files.truncate(MAX_CHANGED_FILES);
    let additions = files.iter().map(|f| f.additions).sum();
    let deletions = files.iter().map(|f| f.deletions).sum();
    Ok(ChangesSummary {
        base,
        base_branch,
        files,
        truncated,
        total_files,
        additions,
        deletions,
    })
}

/// Cheap line count for an untracked file: read at most 1 MB, and treat a NUL
/// byte as binary. Bigger files report the count for the sampled prefix only.
fn untracked_line_count(path: &Path) -> (u64, bool) {
    let Ok(mut file) = std::fs::File::open(path) else {
        return (0, false);
    };
    let mut buf = vec![0u8; 1024 * 1024];
    let Ok(read) = file.read(&mut buf) else {
        return (0, false);
    };
    let slice = &buf[..read];
    let binary = slice.contains(&0);
    let total = slice.iter().filter(|b| **b == b'\n').count() as u64;
    (total, binary)
}

/// Run `git diff` for one path, reading at most `max_bytes` of stdout and then
/// killing the child, so a huge generated file costs bounded memory and time.
pub(crate) fn read_file_diff(
    dir: &Path,
    base: &str,
    path: &str,
    untracked: bool,
    max_bytes: usize,
) -> Result<FileDiff, String> {
    let max_bytes = max_bytes.clamp(1024, MAX_DIFF_BYTES);
    let mut command = Command::new("git");
    command.arg("-C").arg(dir).args([
        "-c",
        "core.quotepath=off",
        "diff",
        "--no-color",
        "--no-ext-diff",
    ]);
    if untracked {
        command.args(["--no-index", "--", "/dev/null", path]);
    } else {
        command.args([base, "--", path]);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = command
        .spawn()
        .map_err(|_| "Could not start git".to_string())?;
    let mut stdout = child.stdout.take().ok_or("Could not read git output")?;
    let mut buffer = Vec::with_capacity(max_bytes.min(64 * 1024));
    let mut chunk = [0u8; 16 * 1024];
    let mut truncated = false;
    let started = Instant::now();
    loop {
        if started.elapsed() > READ_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            return Err("git timed out".to_string());
        }
        match stdout.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                let room = max_bytes.saturating_sub(buffer.len());
                if n > room {
                    buffer.extend_from_slice(&chunk[..room]);
                    truncated = true;
                    let _ = child.kill();
                    break;
                }
                buffer.extend_from_slice(&chunk[..n]);
            }
            Err(_) => break,
        }
    }
    drop(stdout);
    let _ = child.wait();
    let mut patch = String::from_utf8_lossy(&buffer).into_owned();
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HandoffPreflight {
    pub(crate) branch: Option<String>,
    pub(crate) worktree_dirty: bool,
    pub(crate) target_dirty: bool,
    /// Blocking reason, in the words the dialog shows; `None` when ready.
    pub(crate) blocker: Option<String>,
}

pub(crate) fn handoff_preflight(
    worktree: &Path,
    target: &Path,
    codex_home: &Path,
) -> Result<HandoffPreflight, String> {
    let entries = read_worktrees(worktree, codex_home)?;
    let entry = entries.iter().find(|entry| same_dir(worktree, &entry.path));
    let branch = entry.and_then(|entry| entry.branch.clone());
    let worktree_dirty = read_status(worktree)
        .map(|status| status.counts.is_dirty())
        .unwrap_or(false);
    let target_status = read_status(target);
    let target_dirty = target_status
        .as_ref()
        .map(|status| status.counts.is_dirty())
        .unwrap_or(true);
    let target_entry = entries.iter().find(|entry| same_dir(target, &entry.path));
    let same_repo = target_entry.is_some();
    let target_is_worktree = target_entry.map(|e| !e.is_main).unwrap_or(false);
    let blocker = if branch.is_none() {
        Some("This worktree is not on a branch".to_string())
    } else if entry.map(|e| e.is_main).unwrap_or(false) {
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
    worktree: &Path,
    target: &Path,
    codex_home: &Path,
    commit_uncommitted: bool,
    new_name: Option<&str>,
) -> Result<String, String> {
    let preflight = handoff_preflight(worktree, target, codex_home)?;
    if let Some(blocker) = preflight.blocker {
        return Err(blocker);
    }
    let branch = preflight.branch.expect("preflight guarantees a branch");
    let new_name = new_name
        .map(str::trim)
        .filter(|name| !name.is_empty() && *name != branch);
    if let Some(name) = new_name {
        let valid = run_git(target, &["check-ref-format", "--branch", name], WRITE_TIMEOUT)?;
        if !valid.ok {
            return Err(format!("\"{name}\" is not a valid branch name"));
        }
        let exists = run_git(
            target,
            &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{name}")],
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
        let add = run_git(worktree, &["add", "-A"], WRITE_TIMEOUT)?;
        if !add.ok {
            return Err("Could not stage the worktree's changes".to_string());
        }
        let commit = run_git(
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
    let worktree_str = worktree.to_str().unwrap_or_default();
    let removed = run_git(
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
    let checkout = run_git(target, &["checkout", &branch], WRITE_TIMEOUT)?;
    if !checkout.ok {
        // Put the worktree back so nothing is lost.
        let _ = run_git(
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
        let renamed = run_git(target, &["branch", "-m", &branch, name], WRITE_TIMEOUT)?;
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

    #[test]
    fn numstat_parses_plain_and_renamed_entries() {
        let out = "3\t1\tsrc/a.rs\0-\t-\timg.png\0\x30\t0\t\0old.txt\0new.txt\0";
        let files = parse_numstat(out);
        assert_eq!(files.len(), 3);
        assert_eq!(files[0].path, "src/a.rs");
        assert_eq!(files[0].additions, 3);
        assert!(files[1].binary);
        assert_eq!(files[2].status, "renamed");
        assert_eq!(files[2].old_path.as_deref(), Some("old.txt"));
        assert_eq!(files[2].path, "new.txt");
    }
}
