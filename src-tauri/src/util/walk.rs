//! Shared filesystem-walk configuration.
//!
//! Both the `@`-mention file search and the project content indexer walk user
//! trees; they must agree on what counts as "visible" (gitignore handling) and
//! on the budget that stops a walk of an enormous tree.

use ignore::WalkBuilder;
use std::path::Path;

/// Hard cap on how many directory entries a single walk visits, so a huge
/// project (or an accidental `/`) cannot hang the command.
pub(crate) const MAX_WALKED_FILES: usize = 50_000;

/// Relative paths sent to the UI use `/` on either Host. Joining components
/// preserves a literal backslash in a Unix file name.
pub(crate) fn relative_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// A gitignore-respecting walk of `root` that never follows symlinks.
///
/// Dot-files and dot-directories are included — `.github/workflows` and
/// friends are real mention targets — but the `.git` directory itself is
/// always skipped, since none of its contents are useful to a user.
pub(crate) fn walker(root: &Path) -> ignore::Walk {
    WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .follow_links(false)
        .filter_entry(|entry| entry.file_name() != ".git")
        .build()
}
