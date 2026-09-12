//! Reading one file for the preview modal.
//!
//! The modal opens whether or not the read succeeds, so the everyday failures
//! — a binary, a deleted path, a directory, a file we may not read — come back
//! as a `problem` on an `Ok` payload rather than as `Err`. `Err` is reserved
//! for something the user can do nothing about.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use serde::Serialize;
use tauri::State;

use crate::AppState;

/// Most of a file the modal will render. Beyond this the viewer's gutter costs
/// more than the preview is worth, and the text is truncated with a note.
pub(crate) const MAX_PREVIEW_BYTES: usize = 512 * 1024;
/// How much of the head is inspected for a NUL byte before calling it text.
const SNIFF_BYTES: usize = 8 * 1024;

/// Why a file has no text preview.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) enum PreviewProblem {
    Binary,
    Missing,
    Denied,
    Directory,
    Unreadable,
}

#[derive(Clone, Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FilePreview {
    /// The host path actually read, with a leading `~` expanded.
    pub(crate) path: String,
    /// The file's text, `\r\n` normalised to `\n`. Empty when `problem` is set.
    pub(crate) text: String,
    /// Size on disk, for the "showing the first N KB" note.
    pub(crate) bytes: u64,
    /// Only the head of the file is in `text`.
    pub(crate) truncated: bool,
    /// `None` means `text` is the file.
    pub(crate) problem: Option<PreviewProblem>,
    /// The raw io error, for `Unreadable`.
    pub(crate) detail: Option<String>,
}

impl FilePreview {
    fn problem(path: &str, problem: PreviewProblem, detail: Option<String>) -> Self {
        Self {
            path: path.to_string(),
            text: String::new(),
            bytes: 0,
            truncated: false,
            problem: Some(problem),
            detail,
        }
    }
}

/// Read a file for the preview modal. `path` is a host path of this window's
/// home, optionally `~`-prefixed and never carrying a `:line` suffix — the
/// frontend strips that, because a real filename may contain a colon and the
/// backend cannot tell the two apart.
#[tauri::command]
#[specta::specta]
pub(crate) async fn read_file_preview(
    path: String,
    max_bytes: Option<usize>,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<FilePreview, String> {
    let host = state.ctx(&window).host();
    let host_path = host.expand_tilde(&path);
    // Cheap, and it avoids opening a directory handle only to fail on read.
    if host.is_dir(&host_path) {
        return Ok(FilePreview::problem(
            &host_path,
            PreviewProblem::Directory,
            None,
        ));
    }
    let local = host.to_local(&host_path);
    let cap = max_bytes
        .unwrap_or(MAX_PREVIEW_BYTES)
        .clamp(1024, MAX_PREVIEW_BYTES);
    // On WSL `local` is a `\\wsl.localhost\..` share, so every stat and read is
    // a network round trip; keep it off the main thread.
    tauri::async_runtime::spawn_blocking(move || read_preview_at(&local, &host_path, cap))
        .await
        .map_err(|error| format!("Could not read {path}: {error}"))
}

/// The whole read, with no `Host` and no Tauri in sight so it can be tested
/// against a temp directory.
fn read_preview_at(local: &Path, host_path: &str, cap: usize) -> FilePreview {
    let mut file = match File::open(local) {
        Ok(file) => file,
        Err(error) => {
            let problem = match error.kind() {
                std::io::ErrorKind::NotFound => PreviewProblem::Missing,
                std::io::ErrorKind::PermissionDenied => PreviewProblem::Denied,
                _ => PreviewProblem::Unreadable,
            };
            let detail = (problem == PreviewProblem::Unreadable).then(|| error.to_string());
            return FilePreview::problem(host_path, problem, detail);
        }
    };
    let bytes = file.metadata().map(|meta| meta.len()).unwrap_or(0);

    // One byte past the cap, so a file sitting exactly on it is not called
    // truncated.
    let mut buf = Vec::with_capacity(cap.min(64 * 1024));
    if let Err(error) = file.by_ref().take(cap as u64 + 1).read_to_end(&mut buf) {
        // A directory opens fine on Unix and only fails here; `read_file_preview`
        // catches that earlier, but a plain `read_preview_at` caller may not.
        let problem = match error.kind() {
            std::io::ErrorKind::PermissionDenied => PreviewProblem::Denied,
            std::io::ErrorKind::IsADirectory => PreviewProblem::Directory,
            _ => PreviewProblem::Unreadable,
        };
        let detail = (problem == PreviewProblem::Unreadable).then(|| error.to_string());
        return FilePreview::problem(host_path, problem, detail);
    }
    let truncated = buf.len() > cap;
    if truncated {
        buf.truncate(cap);
    }

    if buf[..buf.len().min(SNIFF_BYTES)].contains(&0) {
        return FilePreview::problem(host_path, PreviewProblem::Binary, None);
    }

    // A truncated read can land inside a multi-byte character. An incomplete
    // trailing sequence is that cut and is dropped; an invalid byte anywhere
    // else means this was never text. `from_utf8_lossy` would paper over both.
    let text = match std::str::from_utf8(&buf) {
        Ok(text) => text,
        Err(error) if error.error_len().is_none() => {
            // Safe by construction: `valid_up_to` is a char boundary.
            std::str::from_utf8(&buf[..error.valid_up_to()]).unwrap_or("")
        }
        Err(_) => return FilePreview::problem(host_path, PreviewProblem::Binary, None),
    };

    FilePreview {
        path: host_path.to_string(),
        // Normalised here, in one place, so the gutter, the highlighter and the
        // viewer's line arithmetic can never disagree about the line count.
        text: text.replace("\r\n", "\n"),
        bytes,
        truncated,
        problem: None,
        detail: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write(dir: &tempfile::TempDir, name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let path = dir.path().join(name);
        File::create(&path).unwrap().write_all(bytes).unwrap();
        path
    }

    #[test]
    fn reads_text_and_normalises_crlf() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(&dir, "a.txt", b"one\r\ntwo\r\n");
        let preview = read_preview_at(&path, "a.txt", MAX_PREVIEW_BYTES);
        assert_eq!(preview.problem, None);
        assert_eq!(preview.text, "one\ntwo\n");
        assert_eq!(preview.bytes, 10);
        assert!(!preview.truncated);
    }

    #[test]
    fn a_nul_byte_means_binary() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(&dir, "a.bin", b"PNG\0\0\0stuff");
        let preview = read_preview_at(&path, "a.bin", MAX_PREVIEW_BYTES);
        assert_eq!(preview.problem, Some(PreviewProblem::Binary));
        assert!(preview.text.is_empty());
    }

    #[test]
    fn invalid_utf8_that_is_not_a_cut_means_binary() {
        let dir = tempfile::tempdir().unwrap();
        // 0xC3 0x28 is a genuine invalid sequence, not a truncation.
        let path = write(&dir, "a.txt", b"ok \xc3\x28 no");
        let preview = read_preview_at(&path, "a.txt", MAX_PREVIEW_BYTES);
        assert_eq!(preview.problem, Some(PreviewProblem::Binary));
    }

    #[test]
    fn truncates_at_the_cap() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(&dir, "big.txt", "x".repeat(5000).as_bytes());
        let preview = read_preview_at(&path, "big.txt", 1024);
        assert_eq!(preview.problem, None);
        assert!(preview.truncated);
        assert_eq!(preview.text.len(), 1024);
        assert_eq!(preview.bytes, 5000);
    }

    #[test]
    fn a_file_exactly_at_the_cap_is_not_truncated() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(&dir, "exact.txt", "x".repeat(1024).as_bytes());
        let preview = read_preview_at(&path, "exact.txt", 1024);
        assert!(!preview.truncated);
        assert_eq!(preview.text.len(), 1024);
    }

    #[test]
    fn a_char_straddling_the_cap_is_dropped_not_mangled() {
        let dir = tempfile::tempdir().unwrap();
        // 1022 ASCII + a 3-byte char: the cap of 1024 cuts it after one byte.
        let mut bytes = vec![b'x'; 1022];
        bytes.extend("→".as_bytes());
        let path = write(&dir, "cut.txt", &bytes);
        let preview = read_preview_at(&path, "cut.txt", 1024);
        assert_eq!(preview.problem, None);
        assert_eq!(preview.text.len(), 1022);
        assert!(!preview.text.contains('\u{fffd}'));
    }

    #[test]
    fn a_missing_file_reports_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nope.txt");
        let preview = read_preview_at(&path, "nope.txt", MAX_PREVIEW_BYTES);
        assert_eq!(preview.problem, Some(PreviewProblem::Missing));
        assert_eq!(preview.detail, None);
    }

    #[test]
    #[cfg(unix)]
    fn an_unreadable_file_reports_denied() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = write(&dir, "secret.txt", b"hi");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
        let preview = read_preview_at(&path, "secret.txt", MAX_PREVIEW_BYTES);
        // root ignores the mode bits, so the assertion only means something
        // when the read actually failed.
        if preview.problem.is_some() {
            assert_eq!(preview.problem, Some(PreviewProblem::Denied));
        }
    }
}
