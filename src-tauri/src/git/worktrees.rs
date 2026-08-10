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

fn parse_worktree_list(stdout: &str) -> Vec<RawWorktree> {
    let mut entries = Vec::new();
    let mut current: Option<RawWorktree> = None;
    for line in stdout.lines() {
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

/// Canonical-path identity: a worktree is Codex-managed only when its
/// canonicalized path lives under `<codex_home>/worktrees/` (permanent) or
/// `<codex_home>/worktrees-tmp/` (temporary). Falls back to a lexical check
/// when the path cannot be canonicalized (e.g. missing dir).
fn is_codex_managed(path: &str, codex_home: &Path) -> bool {
    ["worktrees", "worktrees-tmp"].iter().any(|dir| {
        let root = codex_home.join(dir);
        let canonical_root = std::fs::canonicalize(&root).unwrap_or(root);
        match std::fs::canonicalize(path) {
            Ok(canonical) => canonical.starts_with(&canonical_root),
            Err(_) => Path::new(path).starts_with(&canonical_root),
        }
    })
}

pub(crate) fn read_worktrees(
    repo_dir: &Path,
    codex_home: &Path,
) -> Result<Vec<WorktreeEntry>, String> {
    let output = run_git(repo_dir, &["worktree", "list", "--porcelain"], READ_TIMEOUT)?;
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
        let missing_dir = !Path::new(&item.path).is_dir();
        let branch_checked_out_elsewhere = item
            .branch
            .as_deref()
            .is_some_and(|branch| branch_counts.get(branch).copied().unwrap_or(0) > 1);
        let is_codex_managed = is_codex_managed(&item.path, codex_home);

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
        } else if let Ok(snapshot) = read_status(Path::new(&item.path)) {
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
            is_codex_managed,
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
            "worktree /repo\n",
            "HEAD 1111111111111111111111111111111111111111\n",
            "branch refs/heads/main\n",
            "\n",
            "worktree /repo/../wt-feature\n",
            "HEAD 2222222222222222222222222222222222222222\n",
            "branch refs/heads/feature\n",
            "locked on purpose\n",
            "\n",
            "worktree /repo/../wt-gone\n",
            "HEAD 3333333333333333333333333333333333333333\n",
            "detached\n",
            "prunable gitdir file points to non-existent location\n",
        );
        let entries = parse_worktree_list(stdout);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].path, "/repo");
        assert_eq!(entries[0].branch.as_deref(), Some("main"));
        assert!(entries[1].locked);
        assert_eq!(entries[1].lock_reason.as_deref(), Some("on purpose"));
        assert_eq!(entries[1].branch.as_deref(), Some("feature"));
        assert!(entries[2].detached);
        assert!(entries[2].prunable);
        assert_eq!(
            entries[2].prunable_reason.as_deref(),
            Some("gitdir file points to non-existent location")
        );
    }

    #[test]
    fn a_bare_lock_marker_has_no_reason() {
        let entries = parse_worktree_list("worktree /repo\nlocked\n");
        assert!(entries[0].locked);
        assert_eq!(entries[0].lock_reason, None);
    }

    #[test]
    fn codex_managed_identity_uses_worktrees_prefix_not_name() {
        let home = PathBuf::from("/home/.codex");
        // A path literally under <home>/worktrees is managed.
        assert!(is_codex_managed(
            "/home/.codex/worktrees/0357/feature",
            &home
        ));
        // Temporary worktrees under <home>/worktrees-tmp are managed too.
        assert!(is_codex_managed(
            "/home/.codex/worktrees-tmp/repo/tmp-1",
            &home
        ));
        // A path merely *named* like a worktree elsewhere is not.
        assert!(!is_codex_managed("/elsewhere/worktrees-of-mine", &home));
        assert!(!is_codex_managed("/projects/my-worktree", &home));
    }
}
