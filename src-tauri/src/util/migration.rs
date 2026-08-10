//! Small, copy-only helpers for adopting data from a previous app identity.
//!
//! Migration must never remove or overwrite the source: users need to be able
//! to reopen the previous app while they decide whether Pingex is right for
//! them.  A temporary file followed by a hard link makes the destination appear
//! only after its full contents are durable, without replacing a destination
//! another Pingex launch may have created first.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

/// Copy `source` to `destination` if and only if the source exists and the
/// destination does not. Returns whether this invocation created the copy.
pub(crate) fn copy_file_if_missing(source: &Path, destination: &Path) -> Result<bool, String> {
    if destination.exists() || !source.is_file() {
        return Ok(false);
    }
    let bytes = fs::read(source)
        .map_err(|error| format!("Could not read {}: {error}", source.display()))?;
    let parent = destination
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", destination.display()))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Could not create {}: {error}", parent.display()))?;

    let temporary = parent.join(format!(
        ".{}.pingex-migration-{}",
        destination
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("state"),
        std::process::id()
    ));
    let mut file = match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => return Ok(false),
        Err(error) => return Err(format!("Could not create {}: {error}", temporary.display())),
    };
    let result = (|| {
        file.write_all(&bytes)
            .map_err(|error| format!("Could not write {}: {error}", temporary.display()))?;
        file.sync_all()
            .map_err(|error| format!("Could not sync {}: {error}", temporary.display()))?;
        fs::hard_link(&temporary, destination).map_err(|error| match error.kind() {
            std::io::ErrorKind::AlreadyExists => "destination already exists".to_string(),
            _ => format!("Could not create {}: {error}", destination.display()),
        })
    })();
    let _ = fs::remove_file(&temporary);
    match result {
        Ok(()) => Ok(true),
        Err(message) if message == "destination already exists" => Ok(false),
        Err(message) => Err(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_once_without_touching_the_source() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("pingu");
        let destination = directory.path().join("pingex");
        fs::write(&source, "keep me").unwrap();
        assert!(copy_file_if_missing(&source, &destination).unwrap());
        assert_eq!(fs::read_to_string(&source).unwrap(), "keep me");
        assert_eq!(fs::read_to_string(&destination).unwrap(), "keep me");
        assert!(!copy_file_if_missing(&source, &destination).unwrap());
    }
}
