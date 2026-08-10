use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::util::id::unique_suffix;
use tauri::State;

use crate::AppState;

/// Hard cap on a single attachment. Codex reads these directly off disk, so the
/// staging copy is just a stable, bounded location — not a general upload store.
const MAX_ATTACHMENT_BYTES: u64 = 20 * 1024 * 1024;

/// Total size the staging directory is allowed to occupy. On startup (and after
/// each stage) the oldest files are evicted until the directory fits.
const MAX_STAGING_BYTES: u64 = 256 * 1024 * 1024;

/// A validated, staged attachment handed to the frontend. The `staged_path` is
/// what actually gets passed to `turn/start` (as a `localImage` for images, or
/// a labelled path reference in the prompt for other files).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Attachment {
    pub id: String,
    pub filename: String,
    pub mime: String,
    pub size: u64,
    pub staged_path: String,
    /// "image" | "file".
    pub kind: String,
}

/// A typed staging failure. Surfaced to the frontend as a human-readable string
/// so a failed chip can show why (and offer Retry), but kept typed internally so
/// the validation rules stay unit-testable.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum StageError {
    Empty,
    TooLarge { size: u64, cap: u64 },
    UnsupportedType { extension: String },
    NotFound { path: String },
    Io(String),
}

impl std::fmt::Display for StageError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StageError::Empty => write!(formatter, "The file is empty."),
            StageError::TooLarge { size, cap } => write!(
                formatter,
                "That file is {:.1} MB, over the {} MB limit.",
                *size as f64 / 1_048_576.0,
                cap / 1_048_576
            ),
            StageError::UnsupportedType { extension } => {
                if extension.is_empty() {
                    write!(formatter, "Files without an extension are not supported.")
                } else {
                    write!(formatter, "“.{extension}” files are not supported.")
                }
            }
            StageError::NotFound { path } => write!(formatter, "File not found: {path}"),
            StageError::Io(message) => write!(formatter, "{message}"),
        }
    }
}

impl From<StageError> for String {
    fn from(error: StageError) -> Self {
        error.to_string()
    }
}

/// Reduce an arbitrary source name to a safe basename: no path separators, no
/// leading dots, bounded length, with a fallback when nothing usable remains.
fn safe_filename(name: &str) -> String {
    let base = name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(name)
        .trim()
        .trim_start_matches('.');
    let cleaned: String = base
        .chars()
        .map(|character| match character {
            '/' | '\\' | '\0' => '-',
            other => other,
        })
        .collect();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        "attachment".to_string()
    } else if cleaned.len() > 120 {
        cleaned.chars().take(120).collect()
    } else {
        cleaned.to_string()
    }
}

fn extension_of(filename: &str) -> String {
    Path::new(filename)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .unwrap_or_default()
}

/// Returns `(kind, mime)` for a supported extension, or `None` for anything the
/// staging directory refuses to hold.
fn classify(extension: &str) -> Option<(&'static str, &'static str)> {
    let image = |mime| Some(("image", mime));
    let file = |mime| Some(("file", mime));
    match extension {
        "png" => image("image/png"),
        "jpg" | "jpeg" => image("image/jpeg"),
        "gif" => image("image/gif"),
        "webp" => image("image/webp"),
        "bmp" => image("image/bmp"),
        "tif" | "tiff" => image("image/tiff"),
        "heic" => image("image/heic"),
        "svg" => image("image/svg+xml"),
        "txt" | "text" | "log" => file("text/plain"),
        "md" | "markdown" => file("text/markdown"),
        "json" => file("application/json"),
        "csv" => file("text/csv"),
        "tsv" => file("text/tab-separated-values"),
        "xml" => file("application/xml"),
        "html" | "htm" => file("text/html"),
        "css" => file("text/css"),
        "yaml" | "yml" => file("application/yaml"),
        "toml" => file("application/toml"),
        "ini" | "conf" | "cfg" => file("text/plain"),
        "js" | "mjs" | "cjs" | "jsx" => file("text/javascript"),
        "ts" | "tsx" => file("text/typescript"),
        "py" => file("text/x-python"),
        "rs" => file("text/x-rust"),
        "go" => file("text/x-go"),
        "java" => file("text/x-java"),
        "c" | "h" => file("text/x-c"),
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" => file("text/x-c++"),
        "rb" => file("text/x-ruby"),
        "php" => file("text/x-php"),
        "sh" | "bash" | "zsh" => file("text/x-shellscript"),
        "sql" => file("application/sql"),
        "pdf" => file("application/pdf"),
        _ => None,
    }
}

/// Best-effort magic-byte check for the raster image formats we can sniff. A
/// declared image extension whose bytes don't match a known signature is
/// rejected so a mislabelled binary can't ride in as an image.
fn looks_like_image(extension: &str, bytes: &[u8]) -> bool {
    let starts =
        |signature: &[u8]| bytes.len() >= signature.len() && &bytes[..signature.len()] == signature;
    match extension {
        "png" => starts(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]),
        "jpg" | "jpeg" => starts(&[0xFF, 0xD8, 0xFF]),
        "gif" => starts(b"GIF87a") || starts(b"GIF89a"),
        "bmp" => starts(b"BM"),
        "webp" => bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP",
        // No cheap, reliable signature: trust the extension (still size-capped).
        "tif" | "tiff" | "heic" | "svg" => true,
        _ => true,
    }
}

/// Validate a filename + byte length against the type and size rules, returning
/// the resolved `(kind, mime)`. Pure — the workhorse of the unit tests.
fn validate(filename: &str, bytes: &[u8]) -> Result<(&'static str, &'static str), StageError> {
    if bytes.is_empty() {
        return Err(StageError::Empty);
    }
    let size = bytes.len() as u64;
    if size > MAX_ATTACHMENT_BYTES {
        return Err(StageError::TooLarge {
            size,
            cap: MAX_ATTACHMENT_BYTES,
        });
    }
    let extension = extension_of(filename);
    let (kind, mime) = classify(&extension).ok_or(StageError::UnsupportedType {
        extension: extension.clone(),
    })?;
    if kind == "image" && !looks_like_image(&extension, bytes) {
        return Err(StageError::UnsupportedType { extension });
    }
    Ok((kind, mime))
}

/// A short, collision-resistant id embedded in the staged filename so
/// `remove_staged(id)` can find the file again without a side table.
fn next_id() -> String {
    unique_suffix()
}

fn staged_name(id: &str, filename: &str) -> String {
    format!("{id}__{filename}")
}

/// Copy validated bytes into the staging directory and return the metadata.
fn stage_bytes(staging_dir: &Path, filename: &str, bytes: &[u8]) -> Result<Attachment, StageError> {
    let filename = safe_filename(filename);
    let (kind, mime) = validate(&filename, bytes)?;
    fs::create_dir_all(staging_dir).map_err(|error| StageError::Io(error.to_string()))?;
    let id = next_id();
    let target = staging_dir.join(staged_name(&id, &filename));
    fs::write(&target, bytes).map_err(|error| StageError::Io(error.to_string()))?;
    let attachment = Attachment {
        id,
        filename,
        mime: mime.to_string(),
        size: bytes.len() as u64,
        staged_path: target.to_string_lossy().into_owned(),
        kind: kind.to_string(),
    };
    // Keep the directory bounded; a failure here must never fail the stage.
    let _ = cleanup(staging_dir, MAX_STAGING_BYTES);
    Ok(attachment)
}

/// Read a source file from disk, validate it, and stage a copy.
fn stage_source(staging_dir: &Path, source: &Path) -> Result<Attachment, StageError> {
    let metadata = fs::metadata(source).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            StageError::NotFound {
                path: source.to_string_lossy().into_owned(),
            }
        } else {
            StageError::Io(error.to_string())
        }
    })?;
    if metadata.len() > MAX_ATTACHMENT_BYTES {
        return Err(StageError::TooLarge {
            size: metadata.len(),
            cap: MAX_ATTACHMENT_BYTES,
        });
    }
    let bytes = fs::read(source).map_err(|error| StageError::Io(error.to_string()))?;
    let filename = source
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("attachment");
    stage_bytes(staging_dir, filename, &bytes)
}

fn remove(staging_dir: &Path, id: &str) -> Result<(), StageError> {
    let prefix = format!("{id}__");
    let entries = match fs::read_dir(staging_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(StageError::Io(error.to_string())),
    };
    for entry in entries.flatten() {
        if entry.file_name().to_string_lossy().starts_with(&prefix) {
            let _ = fs::remove_file(entry.path());
        }
    }
    Ok(())
}

/// Evict the oldest staged files until the directory is under `max_total`.
/// Returns the number of files removed. Called on startup and after staging.
pub(crate) fn cleanup(staging_dir: &Path, max_total: u64) -> Result<usize, StageError> {
    let entries = match fs::read_dir(staging_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(StageError::Io(error.to_string())),
    };
    let mut files: Vec<(PathBuf, u64, SystemTime)> = Vec::new();
    let mut total: u64 = 0;
    for entry in entries.flatten() {
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if !metadata.is_file() {
            continue;
        }
        let modified = metadata.modified().unwrap_or(UNIX_EPOCH);
        total += metadata.len();
        files.push((entry.path(), metadata.len(), modified));
    }
    // Oldest first, so we evict the least-recently-staged files.
    files.sort_by(|a, b| a.2.cmp(&b.2));
    let mut removed = 0;
    for (path, size, _) in files {
        if total <= max_total {
            break;
        }
        if fs::remove_file(&path).is_ok() {
            total = total.saturating_sub(size);
            removed += 1;
        }
    }
    Ok(removed)
}

/// Best-effort startup sweep of the staging directory.
pub(crate) fn cleanup_on_startup(codex_home: &Path) {
    let _ = cleanup(&codex_home.join("staging"), MAX_STAGING_BYTES);
}

fn staging_dir(state: &State<'_, AppState>) -> PathBuf {
    state.runtime().codex_home.join("staging")
}

#[tauri::command]
pub(crate) fn stage_attachment(
    source_path: String,
    state: State<'_, AppState>,
) -> Result<Attachment, String> {
    Ok(stage_source(&staging_dir(&state), Path::new(&source_path))?)
}

#[tauri::command]
pub(crate) fn stage_clipboard_image(
    filename: Option<String>,
    mime: Option<String>,
    bytes: Vec<u8>,
    state: State<'_, AppState>,
) -> Result<Attachment, String> {
    let extension = match mime.as_deref() {
        Some("image/png") => "png",
        Some("image/jpeg") => "jpg",
        Some("image/gif") => "gif",
        Some("image/webp") => "webp",
        Some("image/bmp") => "bmp",
        _ => "png",
    };
    let filename = filename.unwrap_or_else(|| format!("pasted-image.{extension}"));
    Ok(stage_bytes(&staging_dir(&state), &filename, &bytes)?)
}

#[tauri::command]
pub(crate) fn remove_staged(id: String, state: State<'_, AppState>) -> Result<(), String> {
    Ok(remove(&staging_dir(&state), &id)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0];

    #[test]
    fn classifies_images_and_files() {
        assert_eq!(classify("png"), Some(("image", "image/png")));
        assert_eq!(classify("md"), Some(("file", "text/markdown")));
        assert_eq!(classify("exe"), None);
        assert_eq!(classify(""), None);
    }

    #[test]
    fn safe_filename_strips_paths_and_dots() {
        assert_eq!(safe_filename("/etc/../foo/bar.png"), "bar.png");
        assert_eq!(safe_filename(".hidden.txt"), "hidden.txt");
        assert_eq!(safe_filename("   "), "attachment");
        assert_eq!(safe_filename("a/b\\c.txt"), "c.txt");
    }

    #[test]
    fn validate_rejects_empty_oversize_and_unknown() {
        assert_eq!(validate("a.png", &[]), Err(StageError::Empty));
        assert_eq!(validate("a.png", PNG).unwrap(), ("image", "image/png"));
        assert_eq!(
            validate("a.exe", &[1, 2, 3]),
            Err(StageError::UnsupportedType {
                extension: "exe".into()
            })
        );
        let big = vec![b'x'; (MAX_ATTACHMENT_BYTES + 1) as usize];
        // Use a text extension so size is the only failure reason.
        match validate("a.txt", &big) {
            Err(StageError::TooLarge { cap, .. }) => assert_eq!(cap, MAX_ATTACHMENT_BYTES),
            other => panic!("expected TooLarge, got {other:?}"),
        }
    }

    #[test]
    fn validate_rejects_mislabelled_image() {
        // Declares .png but the bytes aren't a PNG.
        assert_eq!(
            validate("fake.png", b"not really a png"),
            Err(StageError::UnsupportedType {
                extension: "png".into()
            })
        );
    }

    #[test]
    fn stage_and_remove_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let staging = dir.path().join("staging");
        let attachment = stage_bytes(&staging, "note.txt", b"hello world").unwrap();
        assert_eq!(attachment.kind, "file");
        assert_eq!(attachment.filename, "note.txt");
        assert_eq!(attachment.size, 11);
        assert!(Path::new(&attachment.staged_path).is_file());
        assert!(attachment.staged_path.contains(&attachment.id));

        remove(&staging, &attachment.id).unwrap();
        assert!(!Path::new(&attachment.staged_path).exists());
        // Removing an absent id is a no-op.
        remove(&staging, "nope").unwrap();
        remove(&staging, &attachment.id).unwrap();
    }

    #[test]
    fn stage_source_reads_from_disk() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("photo.png");
        fs::write(&source, PNG).unwrap();
        let attachment = stage_source(&dir.path().join("staging"), &source).unwrap();
        assert_eq!(attachment.kind, "image");
        assert_eq!(attachment.mime, "image/png");

        let missing = dir.path().join("gone.txt");
        match stage_source(&dir.path().join("staging"), &missing) {
            Err(StageError::NotFound { .. }) => {}
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    #[test]
    fn cleanup_evicts_oldest_until_under_cap() {
        let dir = tempfile::tempdir().unwrap();
        let staging = dir.path().join("staging");
        fs::create_dir_all(&staging).unwrap();
        // Three 10-byte files; write with increasing mtimes.
        for (index, name) in ["a__x", "b__y", "c__z"].iter().enumerate() {
            let path = staging.join(name);
            fs::write(&path, vec![b'x'; 10]).unwrap();
            let when = UNIX_EPOCH + std::time::Duration::from_secs(1000 + index as u64 * 10);
            filetime_set(&path, when);
        }
        // Cap of 20 bytes must evict the single oldest (10 bytes) to reach 20.
        let removed = cleanup(&staging, 20).unwrap();
        assert_eq!(removed, 1);
        assert!(!staging.join("a__x").exists());
        assert!(staging.join("b__y").exists());
        assert!(staging.join("c__z").exists());
        // Missing directory is a no-op.
        assert_eq!(cleanup(&dir.path().join("absent"), 20).unwrap(), 0);
    }

    /// Set a file's mtime for deterministic eviction ordering, using a tiny
    /// touch loop rather than pulling in the `filetime` crate.
    fn filetime_set(path: &Path, when: SystemTime) {
        // `fs` can't set mtime directly on stable without a crate; approximate
        // ordering by writing in sequence and sleeping is flaky, so re-open and
        // rely on the OS honouring set_modified via File (stable since 1.75).
        let file = fs::OpenOptions::new().write(true).open(path).unwrap();
        file.set_modified(when).unwrap();
    }
}
