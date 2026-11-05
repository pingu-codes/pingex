//! Listing worktrees, enriched with the facts the worktree view needs.
//!
//! `git worktree list --porcelain` gives registration data only, so each entry
//! is joined with a status read, a Codex-managed identity check, and the
//! branch-collision detection that git itself does not report per entry.

use std::collections::HashMap;
use std::path::Path;

use super::run::{run_git, READ_TIMEOUT};
use super::status::read_status;
use super::types::WorktreeEntry;
use crate::util::host::Host;

/// One raw record from the porcelain listing, before enrichment.
struct RawWorktree {
    path: String,
    head: Option<String>,
    branch: Option<String>,
    detached: bool,
    bare: bool,
    locked: bool,
    lock_reason: Option<String>,
    prunable: bool,
    prunable_reason: Option<String>,
}

/// Parse `git worktree list --porcelain -z`: NUL-terminated attribute records,
/// an empty record between worktrees, paths and lock reasons raw.
fn parse_worktree_list(stdout: &str) -> Vec<RawWorktree> {
    let mut entries = Vec::new();
    let mut current: Option<RawWorktree> = None;
    for line in stdout.split('\0') {
        if let Some(path) = line.strip_prefix("worktree ") {
            if let Some(entry) = current.take() {
                entries.push(entry);
            }
            current = Some(RawWorktree {
                path: path.to_string(),
                head: None,
                branch: None,
                detached: false,
                bare: false,
                locked: false,
                lock_reason: None,
                prunable: false,
                prunable_reason: None,
            });
        } else if let Some(entry) = current.as_mut() {
            if let Some(head) = line.strip_prefix("HEAD ") {
                entry.head = Some(head.to_string());
            } else if let Some(branch) = line.strip_prefix("branch ") {
                // `refs/heads/main` -> `main`
                entry.branch = Some(
                    branch
                        .strip_prefix("refs/heads/")
                        .unwrap_or(branch)
                        .to_string(),
                );
            } else if line == "detached" {
                entry.detached = true;
            } else if line == "bare" {
                entry.bare = true;
            } else if line == "locked" || line.starts_with("locked ") {
                entry.locked = true;
                entry.lock_reason = line
                    .strip_prefix("locked ")
                    .map(|reason| reason.trim().to_string())
                    .filter(|reason| !reason.is_empty());
            } else if line == "prunable" || line.starts_with("prunable ") {
                entry.prunable = true;
                entry.prunable_reason = line
                    .strip_prefix("prunable ")
                    .map(|reason| reason.trim().to_string())
                    .filter(|reason| !reason.is_empty());
            }
        }
    }
    if let Some(entry) = current.take() {
        entries.push(entry);
    }
    entries
}

/// Canonical-path identity: which Codex-home worktree root a worktree lives
/// under, `worktrees` (permanent) or `worktrees-tmp` (temporary), if either.
/// Falls back to a lexical check when the path cannot be canonicalized (e.g.
/// missing dir).
fn codex_worktree_root(host: &Host, path: &str, codex_home: &Path) -> Option<&'static str> {
    let home = host.path_string(codex_home);
    let path = host.canonical(path);
    ["worktrees", "worktrees-tmp"].into_iter().find(|dir| {
        let root = host.canonical(&host.join_str(&home, dir));
        host.is_under(&root, &path)
    })
}

pub(crate) fn read_worktrees(
    host: &Host,
    repo_dir: &Path,
    codex_home: &Path,
) -> Result<Vec<WorktreeEntry>, String> {
    let output = run_git(
        host,
        repo_dir,
        &["worktree", "list", "--porcelain", "-z"],
        READ_TIMEOUT,
    )?;
    if !output.ok {
        return Err("Could not list worktrees for this repository".to_string());
    }
    let raw = parse_worktree_list(&output.stdout);

    // Flag any branch that appears in more than one worktree.
    let mut branch_counts: HashMap<String, usize> = HashMap::new();
    for entry in &raw {
        if let Some(branch) = entry.branch.clone() {
            *branch_counts.entry(branch).or_default() += 1;
        }
    }

    let mut entries = Vec::with_capacity(raw.len());
    for (index, item) in raw.into_iter().enumerate() {
        let missing_dir = !host.is_dir(&item.path);
        let branch_checked_out_elsewhere = item
            .branch
            .as_deref()
            .is_some_and(|branch| branch_counts.get(branch).copied().unwrap_or(0) > 1);
        let root = codex_worktree_root(host, &item.path, codex_home);

        // A worktree that cannot be inspected still gets a row, tagged with why.
        let mut state = None;
        let mut status = None;
        let mut ahead = 0;
        let mut behind = 0;
        let mut upstream = None;
        if missing_dir {
            state = Some("missingDir".to_string());
        } else if item.prunable {
            state = Some("prunable".to_string());
        } else if let Ok(snapshot) = read_status(host, Path::new(&item.path)) {
            ahead = snapshot.ahead;
            behind = snapshot.behind;
            upstream = snapshot.upstream;
            status = Some(snapshot.counts);
            if item.detached {
                state = Some("detached".to_string());
            } else if branch_checked_out_elsewhere {
                state = Some("branchCheckedOutElsewhere".to_string());
            }
        } else {
            state = Some("statusUnavailable".to_string());
        }

        entries.push(WorktreeEntry {
            path: item.path,
            head: item.head,
            branch: item.branch,
            detached: item.detached,
            bare: item.bare,
            locked: item.locked,
            lock_reason: item.lock_reason,
            prunable: item.prunable,
            prunable_reason: item.prunable_reason,
            // Git always lists the main working tree first.
            is_main: index == 0,
            is_codex_managed: root.is_some(),
            is_temp: root == Some("worktrees-tmp"),
            missing_dir,
            branch_checked_out_elsewhere,
            upstream,
            ahead,
            behind,
            status,
            state,
        });
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn parses_worktree_list_with_lock_and_prune() {
        let stdout = concat!(
            "worktree /repo\0",
            "HEAD 1111111111111111111111111111111111111111\0",
            "branch refs/heads/main\0",
            "\0",
            "worktree /repo/../wt-feature\0",
            "HEAD 2222222222222222222222222222222222222222\0",
            "branch refs/heads/feature\0",
            "locked on purpose\0",
            "\0",
            "worktree /repo/../wt gone\0",
            "HEAD 3333333333333333333333333333333333333333\0",
            "detached\0",
            "prunable gitdir file points to non-existent location\0",
        );
        let entries = parse_worktree_list(stdout);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].path, "/repo");
        assert_eq!(entries[0].branch.as_deref(), Some("main"));
        assert!(entries[1].locked);
        assert_eq!(entries[1].lock_reason.as_deref(), Some("on purpose"));
        assert_eq!(entries[1].branch.as_deref(), Some("feature"));
        assert!(entries[2].detached);
        assert_eq!(entries[2].path, "/repo/../wt gone");
        assert!(entries[2].prunable);
        assert_eq!(
            entries[2].prunable_reason.as_deref(),
            Some("gitdir file points to non-existent location")
        );
    }

    #[test]
    fn a_bare_lock_marker_has_no_reason() {
        let entries = parse_worktree_list("worktree /repo\0locked\0");
        assert!(entries[0].locked);
        assert_eq!(entries[0].lock_reason, None);
    }

    #[test]
    fn codex_managed_identity_uses_worktrees_prefix_not_name() {
        let home = PathBuf::from("/home/.codex");
        let host = Host::Native;
        let root = |path: &str| codex_worktree_root(&host, path, &home);
        // A path literally under <home>/worktrees is managed and permanent.
        assert_eq!(
            root("/home/.codex/worktrees/0357/feature"),
            Some("worktrees")
        );
        // Temporary worktrees under <home>/worktrees-tmp are managed too.
        assert_eq!(
            root("/home/.codex/worktrees-tmp/repo/tmp-1"),
            Some("worktrees-tmp")
        );
        // A path merely *named* like a worktree elsewhere is not.
        assert_eq!(root("/elsewhere/worktrees-of-mine"), None);
        assert_eq!(root("/projects/my-worktree"), None);
    }
}
