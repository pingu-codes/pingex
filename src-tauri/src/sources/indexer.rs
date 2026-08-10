//! Rust-owned content indexing for project sources.
//!
//! Walks an attached source (a folder, respecting `.gitignore`, or a single
//! file) and turns its text content into per-line rows the LIKE-based workspace
//! search matches against. Filesystem access lives here, never in the renderer.

use std::path::Path;

use crate::storage::IndexedLine;
use crate::util::walk::{walker, MAX_WALKED_FILES};

/// Skip files larger than this; large files are usually generated or binary and
/// blow up the index for little search value.
const MAX_FILE_BYTES: u64 = 1_048_576; // 1 MiB
/// Hard cap on indexed lines per source so a huge tree cannot grow the index
/// without bound.
const MAX_LINES: usize = 200_000;
/// Truncate very long lines (usually minified assets) to keep rows bounded.
const MAX_LINE_LEN: usize = 1_000;

/// Index one source. `kind` is "folder" (walk it, respecting `.gitignore`) or
/// "file" (index just that file). Returns the collected content lines; the
/// caller persists them.
pub(crate) fn index_source(root: &Path, kind: &str) -> Vec<IndexedLine> {
    let mut lines = Vec::new();
    if kind == "file" {
        let file_name = root
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_string();
        index_file(root, &file_name, &mut lines);
        return lines;
    }
    let mut walked = 0usize;
    for entry in walker(root) {
        if lines.len() >= MAX_LINES || walked >= MAX_WALKED_FILES {
            break;
        }
        let Ok(entry) = entry else { continue };
        walked += 1;
        if !entry.file_type().is_some_and(|kind| kind.is_file()) {
            continue;
        }
        let Ok(relative) = entry.path().strip_prefix(root) else {
            continue;
        };
        let relative = relative.to_string_lossy().to_string();
        let file_name = entry.file_name().to_string_lossy().to_string();
        index_file_into(entry.path(), &relative, &file_name, &mut lines);
    }
    lines
}

/// Index a single file addressed by its own name (used for "file" sources).
fn index_file(path: &Path, file_name: &str, lines: &mut Vec<IndexedLine>) {
    index_file_into(path, file_name, file_name, lines);
}

fn index_file_into(path: &Path, file_path: &str, file_name: &str, lines: &mut Vec<IndexedLine>) {
    let Ok(metadata) = std::fs::metadata(path) else {
        return;
    };
    if metadata.len() > MAX_FILE_BYTES {
        return;
    }
    let Ok(bytes) = std::fs::read(path) else {
        return;
    };
    // Treat NUL bytes as a binary marker and skip; only text is searchable.
    if bytes.contains(&0) {
        return;
    }
    let Ok(text) = String::from_utf8(bytes) else {
        return;
    };
    for (offset, raw) in text.lines().enumerate() {
        if lines.len() >= MAX_LINES {
            break;
        }
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let content: String = trimmed.chars().take(MAX_LINE_LEN).collect();
        lines.push(IndexedLine {
            file_path: file_path.to_string(),
            file_name: file_name.to_string(),
            line_number: (offset + 1) as i64,
            content,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn indexes_folder_respecting_gitignore_and_skipping_binaries() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("node_modules")).unwrap();
        fs::write(root.join(".gitignore"), "node_modules/\n").unwrap();
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {}\n\n// note\n").unwrap();
        fs::write(root.join("node_modules/ignored.js"), "secret\n").unwrap();
        fs::write(root.join("binary.bin"), [0u8, 1, 2, 3]).unwrap();

        let lines = index_source(root, "folder");
        let files: Vec<_> = lines.iter().map(|line| line.file_path.as_str()).collect();
        assert!(files.contains(&"src/main.rs"));
        // Blank lines are dropped; two non-empty lines remain.
        assert_eq!(files.iter().filter(|f| **f == "src/main.rs").count(), 2);
        assert!(!files.iter().any(|f| f.contains("node_modules")));
        assert!(!files.iter().any(|f| f.contains("binary.bin")));
        // Line numbers are 1-based and skip blank lines' gaps correctly.
        let note = lines.iter().find(|l| l.content == "// note").unwrap();
        assert_eq!(note.line_number, 3);
    }

    #[test]
    fn indexes_a_single_file_by_name() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("AGENTS.md");
        fs::write(&path, "# Title\nUse tabs\n").unwrap();
        let lines = index_source(&path, "file");
        assert_eq!(lines.len(), 2);
        assert!(lines.iter().all(|line| line.file_path == "AGENTS.md"));
        assert_eq!(lines[1].content, "Use tabs");
    }
}
