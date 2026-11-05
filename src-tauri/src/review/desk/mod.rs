//! The review desk service: every desk operation on one repository, and the
//! backend test seam.
//!
//! A `ReviewDesk` is a repository's clone (`repo_dir` on `host`), its GitHub
//! provider and the profile database. Tauri commands build one per call and
//! forward to it; tests build one over `git::test_repo::TestRepo` and
//! `review::fake::FakeGitHub` and assert on what a caller observes: the
//! values returned, git state on disk, and what GitHub was sent.
//!
//! Operations are grouped by area, one file each, so slices of the desk grow
//! side by side:
//!
//! | File | Area |
//! | --- | --- |
//! | `sync` | Inbox sync and the "since you last looked" feed, native stacks |
//! | `seen` | Seen head and "changes since" |
//! | `review` | One PR under review: detail, freshness, the review and comments written on it |
//! | `pending` | The pending review: stored per PR, submitted as one review |
//! | `viewed` | Files marked viewed, keyed by blob id |
//! | `pr` | Changes to the PR itself: merge, close, retarget, mark ready, checks, new PRs |
//! | `checkouts` | Checkout steps; today the No checkout step: PR refs, diffs and files by SHA |
//! | `repositories` | The account's repositories, for Add repository |
//! | `cleanup` | Dropping what a closed PR left behind |
//!
//! A new area (agent reviews, stack operations) gets its own file with its
//! own `impl ReviewDesk` block and tests.
//!
//! Every operation is async and does its git and `gh` work on a blocking
//! task, so callers never block the runtime.

mod checkouts;
mod cleanup;
mod pending;
mod pr;
mod repositories;
mod review;
mod seen;
mod sync;
#[cfg(test)]
mod test_support;
mod viewed;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use turso::Database;

use super::provider::GitHubProvider;
use crate::util::host::Host;

/// The provider name stored rows are keyed by.
const PROVIDER: &str = "github";

#[derive(Clone)]
pub(crate) struct ReviewDesk {
    host: Host,
    repo_dir: PathBuf,
    github: Arc<dyn GitHubProvider>,
    database: Database,
}

impl ReviewDesk {
    pub(crate) fn new(
        host: Host,
        repo_dir: PathBuf,
        github: Arc<dyn GitHubProvider>,
        database: Database,
    ) -> Self {
        Self {
            host,
            repo_dir,
            github,
            database,
        }
    }

    fn dir(&self) -> &Path {
        &self.repo_dir
    }

    /// Run `work` on a blocking task: it spawns git or `gh`.
    async fn blocking<T, F>(&self, failed: &str, work: F) -> Result<T, String>
    where
        T: Send + 'static,
        F: FnOnce(&ReviewDesk) -> Result<T, String> + Send + 'static,
    {
        let desk = self.clone();
        tauri::async_runtime::spawn_blocking(move || work(&desk))
            .await
            .map_err(|_| failed.to_string())?
    }
}

/// A PR number for a ref name: positive and in range.
fn pr_number(number: i64) -> Result<u32, String> {
    u32::try_from(number)
        .ok()
        .filter(|number| *number > 0)
        .ok_or_else(|| "That is not a pull request number".to_string())
}
