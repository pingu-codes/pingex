//! A segment's file listing (counts, statuses, content keys, never diff
//! bodies) and, on demand, its patches as `PrFile`s.
//!
//! Every read runs from the repository's top level, so `diff`, `ls-files`
//! and `hash-object` all speak root-relative paths. Committed listings are
//! keyed by SHAs and cached; the working tree is read every time.

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;
use std::sync::LazyLock;
use std::time::Duration;

use super::ChangedFile;
use crate::git::objects::{self, require_sha, MAX_READ_BYTES};
use crate::git::run::{run_git, run_git_capped, run_git_with_input, READ_TIMEOUT};
use crate::review::diff::parse_git_diff;
use crate::review::types::{PrFile, ShaDiff};
use crate::util::host::Host;
use crate::util::ttl_cache::TtlCache;

/// Past this many files a listing is cut (and says so).
pub(super) const MAX_LISTED_FILES: usize = 2000;
/// Untracked files whose lines are counted; the rest list with no count.
const COUNTED_UNTRACKED: usize = 200;
/// Working-tree files hashed for content keys.
const HASHED_FILES: usize = 500;
/// Untracked files given a patch in a working-tree diff.
const UNTRACKED_PATCHES: usize = 100;
const UNTRACKED_PATCH_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Listing {
    pub(super) files: Vec<ChangedFile>,
    pub(super) total_files: usize,
    pub(super) truncated: bool,
    pub(super) additions: u64,
    pub(super) deletions: u64,
}

type ListingKey = (Host, String, String, String);

static LISTINGS: LazyLock<TtlCache<ListingKey, Listing>> =
    LazyLock::new(|| TtlCache::new(Duration::from_secs(30 * 60), 512));

/// `base..head`, both full SHAs.
pub(super) fn committed_listing(
    host: &Host,
    root: &Path,
    base: &str,
    head: &str,
) -> Result<Listing, String> {
    let (base, head) = (require_sha(base)?, require_sha(head)?);
    let key = (
        host.clone(),
        host.path_string(root),
        base.to_string(),
        head.to_string(),
    );
    LISTINGS.get_or_try(key, || {
        let files = read_diff_files(host, root, &[base, head])?;
        Ok(finish(files))
    })
}

/// The working tree, untracked files included, against `base`.
pub(super) fn working_listing(host: &Host, root: &Path, base: &str) -> Result<Listing, String> {
    let mut files = read_diff_files(host, root, &[base])?;
    let root_str = host.path_string(root);
    for (index, path) in read_untracked(host, root).into_iter().enumerate() {
        let (additions, binary) = if index < COUNTED_UNTRACKED {
            untracked_line_count(&host.to_local(&host.join_str(&root_str, &path)))
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
            content_key: None,
            before_thread: false,
        });
    }
    hash_working_files(host, root, &mut files);
    Ok(finish(files))
}

/// Root-relative paths with uncommitted changes, untracked files included,
/// for a thread's start tips. Cheap: names only.
pub(super) fn dirty_paths(host: &Host, root: &Path, base: &str) -> Vec<String> {
    let mut paths = run_git(
        host,
        root,
        &["diff", "--name-only", "-z", "--no-renames", base, "--"],
        READ_TIMEOUT,
    )
    .ok()
    .filter(|output| output.ok)
    .map(|output| split_nul(&output.stdout))
    .unwrap_or_default();
    paths.extend(read_untracked(host, root));
    paths.truncate(MAX_LISTED_FILES);
    paths
}

fn finish(mut files: Vec<ChangedFile>) -> Listing {
    let total_files = files.len();
    let truncated = total_files > MAX_LISTED_FILES;
    files.truncate(MAX_LISTED_FILES);
    Listing {
        additions: files.iter().map(|file| file.additions).sum(),
        deletions: files.iter().map(|file| file.deletions).sum(),
        files,
        total_files,
        truncated,
    }
}

/// `git diff --raw` for statuses and blob ids, `--numstat` for counts, over
/// the same revisions.
fn read_diff_files(host: &Host, root: &Path, revs: &[&str]) -> Result<Vec<ChangedFile>, String> {
    let mut raw_args = vec!["diff", "--raw", "-z", "-M", "--no-abbrev", "--no-ext-diff"];
    raw_args.extend_from_slice(revs);
    raw_args.push("--");
    let raw = run_git(host, root, &raw_args, READ_TIMEOUT)?;
    if !raw.ok {
        return Err("Could not read changes for this repository".to_string());
    }
    let mut num_args = vec!["diff", "--numstat", "-z", "-M", "--no-ext-diff"];
    num_args.extend_from_slice(revs);
    num_args.push("--");
    let numstat = run_git(host, root, &num_args, READ_TIMEOUT)?;
    let counts: HashMap<String, (u64, u64, bool)> = if numstat.ok {
        parse_numstat(&numstat.stdout)
    } else {
        HashMap::new()
    };
    let mut files = parse_raw(&raw.stdout);
    for file in &mut files {
        if let Some((additions, deletions, binary)) = counts.get(&file.path) {
            file.additions = *additions;
            file.deletions = *deletions;
            file.binary = *binary;
        }
    }
    Ok(files)
}

fn is_null_sha(sha: &str) -> bool {
    !sha.is_empty() && sha.bytes().all(|byte| byte == b'0')
}

/// `-z --raw`: `:<mode> <mode> <old> <new> <status>\0<path>\0`, with a second
/// path after renames and copies.
fn parse_raw(stdout: &str) -> Vec<ChangedFile> {
    let mut files = Vec::new();
    let mut parts = stdout.split('\0');
    while let Some(meta) = parts.next() {
        let Some(meta) = meta.strip_prefix(':') else {
            continue;
        };
        let fields: Vec<&str> = meta.split(' ').collect();
        let (old_sha, new_sha, code) = match fields.as_slice() {
            [_, _, old, new, code, ..] => (*old, *new, *code),
            _ => continue,
        };
        let letter = code.chars().next().unwrap_or('M');
        let first = parts.next().unwrap_or("").to_string();
        let (path, old_path) = if matches!(letter, 'R' | 'C') {
            (parts.next().unwrap_or("").to_string(), Some(first))
        } else {
            (first, None)
        };
        let status = match letter {
            'A' | 'C' => "added",
            'D' => "deleted",
            'R' => "renamed",
            _ => "modified",
        };
        let content_key = if letter == 'D' {
            Some(format!("deleted:{old_sha}"))
        } else if is_null_sha(new_sha) {
            // A working-tree file: hashed afterwards.
            None
        } else {
            Some(new_sha.to_string())
        };
        files.push(ChangedFile {
            path,
            old_path: if letter == 'R' { old_path } else { None },
            status: status.into(),
            additions: 0,
            deletions: 0,
            binary: false,
            content_key,
            before_thread: false,
        });
    }
    files
}

/// `-z --numstat`: `add\tdel\tpath\0`, or `add\tdel\t\0old\0new\0` for a
/// rename. Binary files count as `-`.
fn parse_numstat(stdout: &str) -> HashMap<String, (u64, u64, bool)> {
    let mut counts = HashMap::new();
    let mut parts = stdout.split('\0');
    while let Some(record) = parts.next() {
        if record.is_empty() {
            continue;
        }
        let mut columns = record.splitn(3, '\t');
        let add = columns.next().unwrap_or("");
        let del = columns.next().unwrap_or("");
        let path = match columns.next() {
            Some(path) if !path.is_empty() => path.to_string(),
            _ => {
                let _old = parts.next();
                parts.next().unwrap_or("").to_string()
            }
        };
        counts.insert(
            path,
            (
                add.parse().unwrap_or(0),
                del.parse().unwrap_or(0),
                add == "-",
            ),
        );
    }
    counts
}

/// Content keys for working-tree files, from one `hash-object` (which
/// writes nothing without `-w`).
fn hash_working_files(host: &Host, root: &Path, files: &mut [ChangedFile]) {
    let wanted: Vec<usize> = files
        .iter()
        .enumerate()
        .filter(|(_, file)| file.content_key.is_none() && !file.path.contains('\n'))
        .map(|(index, _)| index)
        .take(HASHED_FILES)
        .collect();
    if wanted.is_empty() {
        return;
    }
    let input: String = wanted
        .iter()
        .map(|index| format!("{}\n", files[*index].path))
        .collect();
    let Ok(output) = run_git_with_input(host, root, &["hash-object", "--stdin-paths"], &input)
    else {
        return;
    };
    if !output.ok {
        return;
    }
    for (index, sha) in wanted.iter().zip(output.stdout.lines()) {
        files[*index].content_key = Some(sha.trim().to_string());
    }
}

fn split_nul(stdout: &str) -> Vec<String> {
    stdout
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(str::to_string)
        .collect()
}

fn read_untracked(host: &Host, root: &Path) -> Vec<String> {
    run_git(
        host,
        root,
        &["ls-files", "--others", "--exclude-standard", "-z"],
        READ_TIMEOUT,
    )
    .ok()
    .filter(|output| output.ok)
    .map(|output| split_nul(&output.stdout))
    .unwrap_or_default()
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
    let total = slice.iter().filter(|byte| **byte == b'\n').count() as u64;
    (total, binary)
}

/// A committed segment's patches: the straight diff `base..head`, read by
/// SHA (and cached by `objects`).
pub(super) fn committed_diff(
    host: &Host,
    root: &Path,
    base: &str,
    head: &str,
) -> Result<ShaDiff, String> {
    let patch = objects::diff_between(host, root, base, head)?;
    Ok(ShaDiff {
        files: parse_git_diff(&patch.text),
        truncated: patch.truncated,
    })
}

/// The working tree against `base`, with a patch for each untracked file
/// (up to a cap; past it, an empty `patchTruncated` entry).
pub(super) fn working_diff(host: &Host, root: &Path, base: &str) -> Result<ShaDiff, String> {
    let base = require_sha(base)?;
    let output = run_git_capped(
        host,
        root,
        &["diff", "--no-ext-diff", "--no-textconv", "-M", base, "--"],
        MAX_READ_BYTES,
    )?;
    if !output.ok && !output.truncated {
        return Err("Could not read changes for this repository".to_string());
    }
    let mut text = output.stdout;
    if output.truncated {
        if let Some(cut) = text.rfind('\n') {
            text.truncate(cut + 1);
        }
    }
    let mut files = parse_git_diff(&text);
    let null_device = if host.is_wsl() || cfg!(unix) {
        "/dev/null"
    } else {
        "NUL"
    };
    for (index, path) in read_untracked(host, root).into_iter().enumerate() {
        if index >= UNTRACKED_PATCHES {
            files.push(placeholder(path));
            continue;
        }
        let read = run_git_capped(
            host,
            root,
            &[
                "--literal-pathspecs",
                "diff",
                "--no-ext-diff",
                "--no-index",
                "--",
                null_device,
                &path,
            ],
            UNTRACKED_PATCH_BYTES,
        );
        match read {
            Ok(read) => {
                let mut parsed = parse_git_diff(&read.stdout);
                match parsed.pop() {
                    Some(mut file) => {
                        file.path = path;
                        file.status = "added".into();
                        file.patch_truncated = read.truncated;
                        files.push(file);
                    }
                    None => files.push(placeholder(path)),
                }
            }
            Err(_) => files.push(placeholder(path)),
        }
    }
    Ok(ShaDiff {
        files,
        truncated: output.truncated,
    })
}

fn placeholder(path: String) -> PrFile {
    PrFile {
        path,
        old_path: None,
        status: "added".into(),
        additions: 0,
        deletions: 0,
        patch: String::new(),
        hunks: Vec::new(),
        patch_truncated: true,
        blob_sha: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_entries_give_status_paths_and_content_keys() {
        let new = "b".repeat(40);
        let old = "a".repeat(40);
        let zero = "0".repeat(40);
        let out = format!(
            ":100644 100644 {old} {new} M\0src/a.rs\0\
             :100644 000000 {old} {zero} D\0gone.txt\0\
             :100644 100644 {old} {new} R086\0old.txt\0new.txt\0\
             :100644 100644 {old} {zero} M\0dirty.txt\0"
        );
        let files = parse_raw(&out);
        assert_eq!(files.len(), 4);
        assert_eq!(files[0].path, "src/a.rs");
        assert_eq!(files[0].content_key.as_deref(), Some(new.as_str()));
        assert_eq!(files[1].status, "deleted");
        assert_eq!(
            files[1].content_key.as_deref(),
            Some(format!("deleted:{old}").as_str())
        );
        assert_eq!(files[2].status, "renamed");
        assert_eq!(files[2].path, "new.txt");
        assert_eq!(files[2].old_path.as_deref(), Some("old.txt"));
        assert_eq!(files[3].content_key, None);
    }

    #[test]
    fn numstat_counts_plain_binary_and_renamed_entries() {
        let out = "3\t1\tsrc/a.rs\0-\t-\timg.png\0\x30\t0\t\0old.txt\0new.txt\0";
        let counts = parse_numstat(out);
        assert_eq!(counts.get("src/a.rs"), Some(&(3, 1, false)));
        assert_eq!(counts.get("img.png"), Some(&(0, 0, true)));
        assert_eq!(counts.get("new.txt"), Some(&(0, 0, false)));
    }
}
