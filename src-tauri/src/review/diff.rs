//! Unified-diff parsing.
//!
//! Two entry points onto the same hunk model: `parse_patch` for the per-file
//! patches GitHub returns, and `parse_git_diff` for whole-repository `git diff`
//! output. Every line keeps its old/new line number so the UI can anchor an
//! inline comment to an exact side and line.

use super::types::{DiffHunk, DiffLine, PrFile};

/// Parse a unified diff `patch` (no file header, as GitHub returns per file)
/// into hunks. Each line carries its old/new line number so the UI can anchor
/// an inline comment to an exact side and line.
pub(crate) fn parse_patch(patch: &str) -> Vec<DiffHunk> {
    let mut hunks: Vec<DiffHunk> = Vec::new();
    let mut old_line = 0i64;
    let mut new_line = 0i64;
    for raw in patch.lines() {
        if let Some(header) = raw.strip_prefix("@@") {
            let (old_start, old_lines, new_start, new_lines) = parse_hunk_header(header);
            old_line = old_start;
            new_line = new_start;
            hunks.push(DiffHunk {
                header: raw.to_string(),
                old_start,
                old_lines,
                new_start,
                new_lines,
                lines: Vec::new(),
            });
            continue;
        }
        let Some(hunk) = hunks.last_mut() else {
            continue;
        };
        // A "\ No newline at end of file" marker is metadata, not a line.
        if raw.starts_with('\\') {
            continue;
        }
        let (marker, content) = raw.split_at(raw.char_indices().next().map_or(0, |_| 1));
        match marker {
            "+" => {
                hunk.lines.push(DiffLine {
                    kind: "add".into(),
                    content: content.to_string(),
                    old_line: None,
                    new_line: Some(new_line),
                });
                new_line += 1;
            }
            "-" => {
                hunk.lines.push(DiffLine {
                    kind: "del".into(),
                    content: content.to_string(),
                    old_line: Some(old_line),
                    new_line: None,
                });
                old_line += 1;
            }
            _ => {
                // A context line (leading space) or an empty line.
                let content = raw.strip_prefix(' ').unwrap_or(raw);
                hunk.lines.push(DiffLine {
                    kind: "context".into(),
                    content: content.to_string(),
                    old_line: Some(old_line),
                    new_line: Some(new_line),
                });
                old_line += 1;
                new_line += 1;
            }
        }
    }
    hunks
}

/// Parse the counts from a hunk header body (the part after the leading `@@`),
/// e.g. ` -12,7 +12,9 @@ fn foo()` -> (12, 7, 12, 9). A missing count is 1.
fn parse_hunk_header(header: &str) -> (i64, i64, i64, i64) {
    let mut old_start = 0;
    let mut old_lines = 1;
    let mut new_start = 0;
    let mut new_lines = 1;
    for token in header.split_whitespace() {
        if let Some(rest) = token.strip_prefix('-') {
            let (start, lines) = parse_range(rest);
            old_start = start;
            old_lines = lines;
        } else if let Some(rest) = token.strip_prefix('+') {
            let (start, lines) = parse_range(rest);
            new_start = start;
            new_lines = lines;
        }
    }
    (old_start, old_lines, new_start, new_lines)
}

fn parse_range(range: &str) -> (i64, i64) {
    let mut parts = range.split(',');
    let start = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let lines = parts.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    (start, lines)
}

/// Build a synthetic per-file diff from whole-repository `git diff` output.
/// Turns local, PR-less changes into the same `PrFile` shape so the review view
/// works before a PR exists.
pub(crate) fn parse_git_diff(diff: &str) -> Vec<PrFile> {
    let mut files: Vec<PrFile> = Vec::new();
    let mut current: Option<(String, Option<String>, String, Vec<String>)> = None;

    let flush = |files: &mut Vec<PrFile>,
                 entry: Option<(String, Option<String>, String, Vec<String>)>| {
        if let Some((path, old_path, status, body_lines)) = entry {
            let patch: String = body_lines
                .iter()
                .filter(|line| {
                    line.starts_with('@') || {
                        matches!(line.chars().next(), Some('+') | Some('-') | Some(' '))
                            && !line.starts_with("+++")
                            && !line.starts_with("---")
                    }
                })
                .cloned()
                .collect::<Vec<_>>()
                .join("\n");
            let hunks = parse_patch(&patch);
            let additions = hunks
                .iter()
                .flat_map(|h| &h.lines)
                .filter(|l| l.kind == "add")
                .count() as i64;
            let deletions = hunks
                .iter()
                .flat_map(|h| &h.lines)
                .filter(|l| l.kind == "del")
                .count() as i64;
            files.push(PrFile {
                path,
                old_path,
                status,
                additions,
                deletions,
                hunks,
                patch,
                patch_truncated: false,
            });
        }
    };

    for line in diff.lines() {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            flush(&mut files, current.take());
            // `a/path b/path` -> take the b/ side as the current path.
            let path = rest
                .split(" b/")
                .nth(1)
                .unwrap_or_else(|| rest.trim_start_matches("a/"))
                .to_string();
            current = Some((path, None, "modified".to_string(), Vec::new()));
        } else if let Some(entry) = current.as_mut() {
            if line.starts_with("new file") {
                entry.2 = "added".to_string();
            } else if line.starts_with("deleted file") {
                entry.2 = "removed".to_string();
            } else if let Some(old) = line.strip_prefix("rename from ") {
                entry.1 = Some(old.to_string());
                entry.2 = "renamed".to_string();
            } else if line.starts_with("@@")
                || matches!(line.chars().next(), Some('+') | Some('-') | Some(' '))
            {
                entry.3.push(line.to_string());
            }
        }
    }
    flush(&mut files, current.take());
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_multi_hunk_patch_line_numbers() {
        let patch = "@@ -1,2 +1,2 @@\n-a\n+A\n b\n@@ -10,1 +10,2 @@\n c\n+C";
        let hunks = parse_patch(patch);
        assert_eq!(hunks.len(), 2);
        assert_eq!(hunks[1].lines[0].old_line, Some(10));
        assert_eq!(hunks[1].lines[1].kind, "add");
        assert_eq!(hunks[1].lines[1].new_line, Some(11));
    }
    #[test]
    fn parses_local_git_diff_into_files() {
        let diff = "diff --git a/src/lib.rs b/src/lib.rs\n\
index 111..222 100644\n\
--- a/src/lib.rs\n\
+++ b/src/lib.rs\n\
@@ -1,2 +1,3 @@\n\
 fn a() {}\n\
-fn b() {}\n\
+fn b2() {}\n\
+fn c() {}\n\
diff --git a/new.txt b/new.txt\n\
new file mode 100644\n\
--- /dev/null\n\
+++ b/new.txt\n\
@@ -0,0 +1,1 @@\n\
+hello";
        let files = parse_git_diff(diff);
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, "src/lib.rs");
        assert_eq!(files[0].status, "modified");
        assert_eq!(files[0].additions, 2);
        assert_eq!(files[0].deletions, 1);
        assert_eq!(files[1].path, "new.txt");
        assert_eq!(files[1].status, "added");
        assert_eq!(files[1].additions, 1);
    }
}
