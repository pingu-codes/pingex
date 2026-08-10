//! Recent commits, for the base-revision picker and branch context.

use std::path::Path;

use super::run::{run_git, READ_TIMEOUT};
use super::types::CommitInfo;

/// Field and record separators chosen so a commit subject containing tabs,
/// newlines, or pipes cannot break the parse.
const FIELD: char = '\u{1f}';
const RECORD: char = '\u{1e}';

pub(crate) fn read_recent_commits(dir: &Path, limit: usize) -> Result<Vec<CommitInfo>, String> {
    let limit = limit.clamp(1, 100);
    let limit_arg = format!("-n{limit}");
    // Unit-separator (%x1f) between fields, record-separator (%x1e) between lines.
    let output = run_git(
        dir,
        &[
            "log",
            &limit_arg,
            "--pretty=format:%H%x1f%h%x1f%s%x1f%an%x1f%ct%x1e",
        ],
        READ_TIMEOUT,
    )?;
    if !output.ok {
        // A repository with no commits yet is empty, not an error.
        return Ok(Vec::new());
    }
    let commits = output
        .stdout
        .split(RECORD)
        .filter_map(|record| {
            let record = record.trim_start_matches('\n');
            if record.is_empty() {
                return None;
            }
            let mut fields = record.split(FIELD);
            Some(CommitInfo {
                hash: fields.next()?.to_string(),
                short_hash: fields.next()?.to_string(),
                subject: fields.next()?.to_string(),
                author: fields.next()?.to_string(),
                timestamp: fields.next().and_then(|t| t.parse().ok()).unwrap_or(0),
            })
        })
        .collect();
    Ok(commits)
}
