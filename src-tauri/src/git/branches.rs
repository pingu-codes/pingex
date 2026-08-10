//! Local and remote-tracking branches, for the review target picker.

use std::collections::HashSet;
use std::path::Path;

use super::run::{run_git, READ_TIMEOUT};
use super::types::BranchRef;

/// Turn `for-each-ref` output into branch rows, most recently committed first.
///
/// One ref per line as `<HEAD marker> <refname>`; a space is an unambiguous
/// separator because Git forbids spaces (and newlines) in ref names.
///
/// `refs/remotes/*/HEAD` is dropped: it is a symbolic alias for whatever the
/// remote's default branch is, so it would list the same branch twice under a
/// name (`origin/HEAD`) nobody reviews against.
fn parse_branches(stdout: &str, limit: usize) -> Vec<BranchRef> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut branches = Vec::new();
    for line in stdout.lines() {
        let line = line.trim_end();
        let Some((head, refname)) = line.split_once(' ') else {
            continue;
        };
        // The marker is a space for every branch that is not checked out, so
        // the refname is preceded by leading whitespace in the common case.
        let refname = refname.trim_start();
        let is_current = head.trim() == "*";
        let (name, is_remote) = if let Some(name) = refname.strip_prefix("refs/heads/") {
            (name, false)
        } else if let Some(name) = refname.strip_prefix("refs/remotes/") {
            (name, true)
        } else {
            continue;
        };
        if name.is_empty() || name.ends_with("/HEAD") {
            continue;
        }
        if !seen.insert(name.to_string()) {
            continue;
        }
        branches.push(BranchRef {
            name: name.to_string(),
            is_remote,
            is_current,
        });
        if branches.len() >= limit {
            break;
        }
    }
    branches
}

pub(crate) fn read_branches(dir: &Path, limit: usize) -> Result<Vec<BranchRef>, String> {
    let limit = limit.clamp(1, 500);
    let count_arg = format!("--count={limit}");
    let output = run_git(
        dir,
        &[
            "for-each-ref",
            &count_arg,
            "--sort=-committerdate",
            // `%(HEAD)` is a single character (`*` on the checked-out branch,
            // otherwise a space). `for-each-ref` does not expand `%xNN` escapes
            // the way `git log --pretty` does, so the separator is a literal
            // space — safe, since ref names cannot contain one.
            "--format=%(HEAD) %(refname)",
            "refs/heads",
            "refs/remotes",
        ],
        READ_TIMEOUT,
    )?;
    if !output.ok {
        // A repository with no commits yet has no branches; that is empty, not
        // an error.
        return Ok(Vec::new());
    }
    Ok(parse_branches(&output.stdout, limit))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_local_and_remote_branches() {
        let stdout = concat!(
            "* refs/heads/main\n",
            "  refs/remotes/origin/main\n",
            "  refs/heads/feature/login\n",
        );
        let branches = parse_branches(stdout, 100);
        assert_eq!(branches.len(), 3);
        assert_eq!(branches[0].name, "main");
        assert!(branches[0].is_current);
        assert!(!branches[0].is_remote);
        assert_eq!(branches[1].name, "origin/main");
        assert!(branches[1].is_remote);
        assert!(!branches[1].is_current);
        assert_eq!(branches[2].name, "feature/login");
    }

    #[test]
    fn drops_remote_head_aliases_and_duplicates() {
        let stdout = concat!(
            "  refs/remotes/origin/HEAD\n",
            "  refs/heads/main\n",
            "  refs/heads/main\n",
            "  refs/tags/v1\n",
        );
        let branches = parse_branches(stdout, 100);
        assert_eq!(branches.len(), 1);
        assert_eq!(branches[0].name, "main");
    }

    #[test]
    fn stops_at_the_limit() {
        let stdout = concat!("  refs/heads/a\n", "  refs/heads/b\n", "  refs/heads/c\n");
        let branches = parse_branches(stdout, 2);
        assert_eq!(branches.len(), 2);
        assert_eq!(branches[1].name, "b");
    }

    #[test]
    fn empty_output_is_no_branches() {
        assert!(parse_branches("", 100).is_empty());
    }
}
