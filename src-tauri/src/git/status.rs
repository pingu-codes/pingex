//! Working-tree status and repository facts.
//!
//! The parsers are pure and unit-tested against captured porcelain output; the
//! line kinds (`1`/`2`/`u`/`?`/`!`) are documented in gitformat(5).

use std::path::Path;

use super::run::{run_git, READ_TIMEOUT};
use super::types::{BranchInfo, GitRepoInfo, GitStatus, StatusCounts, StatusFile};
use crate::util::host::Host;
use crate::util::time::unix_millis;

/// Upper bound on the per-status file list handed to the frontend.
const MAX_STATUS_FILES: usize = 500;

/// The records of `git status --porcelain=v2 -z`: each one NUL-terminated,
/// paths raw (no quoting), so a name with spaces, tabs or newlines survives.
fn records(stdout: &str) -> impl Iterator<Item = &str> {
    stdout.split('\0').filter(|record| !record.is_empty())
}

/// Parse the `# branch.*` header records of `git status --porcelain=v2 --branch -z`.
fn parse_branch_headers(stdout: &str) -> BranchInfo {
    let mut info = BranchInfo::default();
    for line in records(stdout) {
        let Some(header) = line.strip_prefix("# branch.") else {
            continue;
        };
        if let Some(head) = header.strip_prefix("head ") {
            let head = head.trim();
            if head == "(detached)" {
                info.detached = true;
            } else {
                info.branch = Some(head.to_string());
            }
        } else if let Some(upstream) = header.strip_prefix("upstream ") {
            let upstream = upstream.trim();
            if !upstream.is_empty() {
                info.upstream = Some(upstream.to_string());
            }
        } else if let Some(ab) = header.strip_prefix("ab ") {
            for token in ab.split_whitespace() {
                if let Some(rest) = token.strip_prefix('+') {
                    info.ahead = rest.parse().unwrap_or(0);
                } else if let Some(rest) = token.strip_prefix('-') {
                    info.behind = rest.parse().unwrap_or(0);
                }
            }
        }
    }
    info
}

/// Parse the entry records of `git status --porcelain=v2 -z` into counts and a
/// bounded file list. Counts are always complete even when the list is capped.
fn parse_status_entries(stdout: &str, max_files: usize) -> (StatusCounts, Vec<StatusFile>, bool) {
    let mut counts = StatusCounts::default();
    let mut files = Vec::new();
    let mut truncated = false;

    // A rename/copy record (`2 …`) is followed by its original path as a
    // record of its own.
    let mut original_path_follows = false;
    for line in records(stdout) {
        if std::mem::take(&mut original_path_follows) {
            continue;
        }
        let mut push = |file: StatusFile| {
            if files.len() < max_files {
                files.push(file);
            } else {
                truncated = true;
            }
        };
        if let Some(rest) = line.strip_prefix("? ") {
            counts.untracked += 1;
            push(StatusFile {
                path: rest.to_string(),
                state: "untracked".into(),
                code: String::new(),
            });
        } else if let Some(rest) = line.strip_prefix("! ") {
            push(StatusFile {
                path: rest.to_string(),
                state: "ignored".into(),
                code: String::new(),
            });
        } else if line.starts_with("u ") {
            // u <XY> <sub> <m1> <m2> <m3> <mW> <h1> <h2> <h3> <path>
            counts.conflicted += 1;
            let mut fields = line.splitn(11, ' ');
            let code = fields.nth(1).unwrap_or("").to_string();
            let path = fields.last().unwrap_or("").to_string();
            push(StatusFile {
                path,
                state: "conflicted".into(),
                code,
            });
        } else if line.starts_with("1 ") || line.starts_with("2 ") {
            // 1 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <path>
            // 2 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <Xscore> <path>
            let renamed = line.starts_with("2 ");
            original_path_follows = renamed;
            let mut fields = line.splitn(if renamed { 10 } else { 9 }, ' ');
            fields.next(); // the "1"/"2" marker
            let xy = fields.next().unwrap_or("..");
            let staged = xy.chars().next().is_some_and(|c| c != '.');
            let unstaged = xy.chars().nth(1).is_some_and(|c| c != '.');
            if staged {
                counts.staged += 1;
            }
            if unstaged {
                counts.unstaged += 1;
            }
            // The path is the final field and may itself contain spaces.
            let path = fields.last().unwrap_or("").to_string();
            // A path with a staged component is reported as staged; otherwise
            // it is a working-tree-only change.
            let state = if staged { "staged" } else { "unstaged" };
            push(StatusFile {
                path,
                state: state.into(),
                code: xy.to_string(),
            });
        }
    }
    (counts, files, truncated)
}

pub(crate) fn read_status(host: &Host, dir: &Path) -> Result<GitStatus, String> {
    let output = run_git(
        host,
        dir,
        &[
            "status",
            "--porcelain=v2",
            "--branch",
            "--untracked-files=all",
            "-z",
        ],
        READ_TIMEOUT,
    )?;
    if !output.ok {
        return Err("Could not read Git status for this directory".to_string());
    }
    let branch = parse_branch_headers(&output.stdout);
    let (counts, files, truncated) = parse_status_entries(&output.stdout, MAX_STATUS_FILES);
    Ok(GitStatus {
        branch: branch.branch,
        detached: branch.detached,
        upstream: branch.upstream,
        ahead: branch.ahead,
        behind: branch.behind,
        counts,
        files,
        truncated,
        refreshed_at: unix_millis(),
    })
}

pub(crate) fn read_repo_info(host: &Host, dir: &Path) -> GitRepoInfo {
    let dir_str = host.path_string(dir);
    let mut info = GitRepoInfo {
        dir: dir_str.clone(),
        is_git_repo: false,
        root: None,
        common_dir: None,
        branch: None,
        detached: false,
        upstream: None,
        ahead: 0,
        behind: 0,
        in_progress: None,
        error: None,
    };
    if !host.is_dir(&dir_str) {
        info.error = Some("This folder does not exist".to_string());
        return info;
    }
    let rev_parse = match run_git(
        host,
        dir,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--show-toplevel",
            "--git-common-dir",
        ],
        READ_TIMEOUT,
    ) {
        Ok(output) => output,
        Err(error) => {
            info.error = Some(error);
            return info;
        }
    };
    if !rev_parse.ok {
        // Not a git repository — a legitimate, reportable state, not an error.
        return info;
    }
    let mut lines = rev_parse.stdout.lines();
    info.is_git_repo = true;
    info.root = lines.next().map(str::to_string).filter(|s| !s.is_empty());
    let common_dir = lines.next().map(str::to_string).filter(|s| !s.is_empty());
    info.common_dir = common_dir.clone();

    if let Ok(status) = read_status(host, dir) {
        info.branch = status.branch;
        info.detached = status.detached;
        info.upstream = status.upstream;
        info.ahead = status.ahead;
        info.behind = status.behind;
    }
    if let Some(common) = common_dir {
        info.in_progress = detect_in_progress(&host.to_local(&common));
    }
    info
}

/// Inspect the common Git dir for an in-progress operation.
fn detect_in_progress(common_dir: &Path) -> Option<String> {
    let has = |name: &str| common_dir.join(name).exists();
    if has("MERGE_HEAD") {
        Some("merge".into())
    } else if has("rebase-merge") || has("rebase-apply") {
        Some("rebase".into())
    } else if has("CHERRY_PICK_HEAD") {
        Some("cherry-pick".into())
    } else if has("REVERT_HEAD") {
        Some("revert".into())
    } else if has("BISECT_LOG") {
        Some("bisect".into())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_branch_headers_with_ahead_behind() {
        let stdout = "# branch.oid abc123\0# branch.head main\0# branch.upstream origin/main\0# branch.ab +2 -3\0";
        let info = parse_branch_headers(stdout);
        assert_eq!(info.branch.as_deref(), Some("main"));
        assert!(!info.detached);
        assert_eq!(info.upstream.as_deref(), Some("origin/main"));
        assert_eq!(info.ahead, 2);
        assert_eq!(info.behind, 3);
    }

    #[test]
    fn parses_detached_head_branch_header() {
        let info = parse_branch_headers("# branch.oid abc123\0# branch.head (detached)\0");
        assert!(info.detached);
        assert_eq!(info.branch, None);
    }

    #[test]
    fn parses_status_v2_entries_and_counts() {
        // one staged modify, one unstaged modify, one untracked, one conflict.
        let stdout = concat!(
            "# branch.head main\0",
            "1 M. N... 100644 100644 100644 aaa bbb src/staged.rs\0",
            "1 .M N... 100644 100644 100644 ccc ddd src/unstaged.rs\0",
            "u UU N... 100644 100644 100644 100644 eee fff ggg src/con flict.rs\0",
            "? untracked.txt\0",
            "! ignored.log\0",
        );
        let (counts, files, truncated) = parse_status_entries(stdout, 500);
        assert_eq!(counts.staged, 1);
        assert_eq!(counts.unstaged, 1);
        assert_eq!(counts.untracked, 1);
        assert_eq!(counts.conflicted, 1);
        assert!(!truncated);
        // 5 rows: staged, unstaged, conflict, untracked, ignored
        assert_eq!(files.len(), 5);
        assert_eq!(files[0].path, "src/staged.rs");
        assert_eq!(files[0].state, "staged");
        assert_eq!(files[2].path, "src/con flict.rs");
        assert_eq!(files[2].code, "UU");
        assert_eq!(files[3].path, "untracked.txt");
        assert_eq!(files[3].state, "untracked");
        assert_eq!(files[4].state, "ignored");
    }

    #[test]
    fn parses_renamed_status_entry_path() {
        let stdout = concat!(
            "2 R. N... 100644 100644 100644 aaa bbb R100 new/my name.rs\0old/name.rs\0",
            "? line\nbreak.txt\0",
        );
        let (counts, files, _) = parse_status_entries(stdout, 500);
        assert_eq!(counts.staged, 1);
        assert_eq!(counts.untracked, 1);
        assert_eq!(
            files.len(),
            2,
            "the original path is not an entry of its own"
        );
        assert_eq!(files[0].path, "new/my name.rs");
        assert_eq!(files[1].path, "line\nbreak.txt");
    }

    #[test]
    fn status_file_list_truncates_at_cap() {
        let stdout: String = (0..10).map(|i| format!("? file{i}.txt\0")).collect();
        let (counts, files, truncated) = parse_status_entries(&stdout, 4);
        assert_eq!(counts.untracked, 10);
        assert_eq!(files.len(), 4);
        assert!(truncated);
    }

    #[test]
    fn detects_in_progress_operations_by_marker_file() {
        let directory = tempfile::tempdir().unwrap();
        let common = directory.path();
        assert_eq!(detect_in_progress(common), None);

        std::fs::write(common.join("MERGE_HEAD"), "").unwrap();
        assert_eq!(detect_in_progress(common).as_deref(), Some("merge"));

        std::fs::remove_file(common.join("MERGE_HEAD")).unwrap();
        std::fs::create_dir(common.join("rebase-merge")).unwrap();
        assert_eq!(detect_in_progress(common).as_deref(), Some("rebase"));
    }
}
