//! Structured Git output handed to the frontend.
//!
//! Everything here is a plain projection: no raw stderr, no unbounded lists, and
//! no paths beyond the repository the caller already named.

use serde::{Deserialize, Serialize};

/// Working-tree status counts, shared by the worktree cards and the branch chip.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StatusCounts {
    pub(crate) staged: usize,
    pub(crate) unstaged: usize,
    pub(crate) untracked: usize,
    pub(crate) conflicted: usize,
}

impl StatusCounts {
    pub(crate) fn is_dirty(&self) -> bool {
        self.staged + self.unstaged + self.untracked + self.conflicted > 0
    }
}

/// One changed path from `git status --porcelain=v2`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StatusFile {
    pub(crate) path: String,
    /// `staged`, `unstaged`, `untracked`, `conflicted`, or `ignored`.
    pub(crate) state: String,
    /// Two-letter porcelain-v2 XY code (e.g. `M.`, `.M`, `A.`); empty for `?`/`!`.
    pub(crate) code: String,
}

/// Branch context parsed from the `# branch.*` headers of `status --branch`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BranchInfo {
    pub(crate) branch: Option<String>,
    pub(crate) detached: bool,
    pub(crate) upstream: Option<String>,
    pub(crate) ahead: i64,
    pub(crate) behind: i64,
}

/// Full working-tree status for one directory.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GitStatus {
    pub(crate) branch: Option<String>,
    pub(crate) detached: bool,
    pub(crate) upstream: Option<String>,
    pub(crate) ahead: i64,
    pub(crate) behind: i64,
    pub(crate) counts: StatusCounts,
    pub(crate) files: Vec<StatusFile>,
    /// True when the real file list was longer than `MAX_STATUS_FILES`.
    pub(crate) truncated: bool,
    /// Milliseconds since the Unix epoch when this snapshot was taken.
    pub(crate) refreshed_at: i64,
}

/// High-level repository facts for a directory.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GitRepoInfo {
    pub(crate) dir: String,
    pub(crate) is_git_repo: bool,
    pub(crate) root: Option<String>,
    pub(crate) common_dir: Option<String>,
    pub(crate) branch: Option<String>,
    pub(crate) detached: bool,
    pub(crate) upstream: Option<String>,
    pub(crate) ahead: i64,
    pub(crate) behind: i64,
    /// `merge`, `rebase`, `cherry-pick`, `revert`, or `bisect` when mid-operation.
    pub(crate) in_progress: Option<String>,
    /// Set only for a hard failure (e.g. git missing); non-git folders instead
    /// report `is_git_repo = false` with no error.
    pub(crate) error: Option<String>,
}

/// One entry from `git worktree list --porcelain`, enriched with identity and
/// a lightweight status summary so the worktree view renders from one call.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorktreeEntry {
    pub(crate) path: String,
    pub(crate) head: Option<String>,
    pub(crate) branch: Option<String>,
    pub(crate) detached: bool,
    pub(crate) bare: bool,
    pub(crate) locked: bool,
    pub(crate) lock_reason: Option<String>,
    pub(crate) prunable: bool,
    pub(crate) prunable_reason: Option<String>,
    pub(crate) is_main: bool,
    /// True only when the canonical path lives under `<codex_home>/worktrees/`;
    /// never inferred from the display name.
    pub(crate) is_codex_managed: bool,
    /// The registered directory is gone (a stale registration to prune).
    pub(crate) missing_dir: bool,
    /// This worktree's branch is also checked out in another listed worktree.
    pub(crate) branch_checked_out_elsewhere: bool,
    pub(crate) upstream: Option<String>,
    pub(crate) ahead: i64,
    pub(crate) behind: i64,
    /// Working-tree counts; `None` when the directory is missing or status failed.
    pub(crate) status: Option<StatusCounts>,
    /// A per-worktree problem to surface instead of dropping the row.
    pub(crate) state: Option<String>,
}

/// A recent commit for the base-revision picker and branch context.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CommitInfo {
    pub(crate) hash: String,
    pub(crate) short_hash: String,
    pub(crate) subject: String,
    pub(crate) author: String,
    pub(crate) timestamp: i64,
}

/// One branch for the review target picker. Remote-tracking branches keep their
/// remote prefix (`origin/main`), which is also what `git diff` expects.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BranchRef {
    pub(crate) name: String,
    pub(crate) is_remote: bool,
    pub(crate) is_current: bool,
}

#[derive(Debug, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub(crate) enum WorktreeBranch {
    /// Check out an existing local branch.
    Existing { name: String },
    /// Create a new branch, optionally from a base revision.
    New { name: String, base: Option<String> },
    /// Create a local branch that tracks a remote-tracking branch
    /// (`origin/feature` → `feature`).
    Tracking { name: String, remote_ref: String },
}

#[derive(Debug, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorktreeAddRequest {
    pub(crate) path: String,
    pub(crate) branch: WorktreeBranch,
}
