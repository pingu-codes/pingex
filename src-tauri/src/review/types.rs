//! Provider-neutral wire types for the review view.
//!
//! Deliberately not GitHub-shaped: a future adapter (GitLab, Gitea) would map
//! its own API onto these same structs so the frontend never changes.

use serde::{Deserialize, Serialize};

/// Availability and auth state of the active review provider.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProviderStatus {
    /// The `gh` executable is on PATH.
    pub(crate) installed: bool,
    /// `gh auth status` reports a logged-in account.
    pub(crate) authenticated: bool,
    /// An actionable message when the provider is not ready.
    pub(crate) message: Option<String>,
}

/// A single open pull request in the PR picker.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PrSummary {
    pub(crate) number: i64,
    pub(crate) title: String,
    pub(crate) author: String,
    /// `OPEN`, `CLOSED`, or `MERGED`.
    pub(crate) state: String,
    pub(crate) is_draft: bool,
    pub(crate) base_ref: String,
    pub(crate) head_ref: String,
    pub(crate) updated_at: String,
    pub(crate) url: String,
}

/// One commit on the PR branch.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PrCommit {
    pub(crate) oid: String,
    pub(crate) short_oid: String,
    pub(crate) headline: String,
    pub(crate) author: String,
}

/// One line inside a parsed diff hunk, carrying stable old/new line numbers so
/// the UI can anchor an inline comment to an exact side and line.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DiffLine {
    /// `context`, `add`, or `del`.
    pub(crate) kind: String,
    pub(crate) content: String,
    /// Line number on the old (base) side, when present.
    pub(crate) old_line: Option<i64>,
    /// Line number on the new (head) side, when present.
    pub(crate) new_line: Option<i64>,
}

/// A single `@@` hunk of a file's diff with parsed lines and stable anchors.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DiffHunk {
    pub(crate) header: String,
    pub(crate) old_start: i64,
    pub(crate) old_lines: i64,
    pub(crate) new_start: i64,
    pub(crate) new_lines: i64,
    pub(crate) lines: Vec<DiffLine>,
}

/// One changed file in a PR (or a local branch diff).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PrFile {
    pub(crate) path: String,
    /// The pre-rename path when `status` is `renamed`.
    pub(crate) old_path: Option<String>,
    /// `added`, `modified`, `removed`, or `renamed`.
    pub(crate) status: String,
    pub(crate) additions: i64,
    pub(crate) deletions: i64,
    /// Raw unified diff for the file, fed to `DiffBlock` for rendering.
    pub(crate) patch: String,
    pub(crate) hunks: Vec<DiffHunk>,
    /// Whether the file's patch was omitted (binary, or too large).
    pub(crate) patch_truncated: bool,
}

/// One review or conversation comment, flattened; the frontend groups inline
/// comments into threads by `thread_id`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PrComment {
    /// REST `databaseId`; 0 for a conversation comment with no numeric id.
    pub(crate) id: i64,
    pub(crate) author: String,
    pub(crate) body: String,
    pub(crate) created_at: String,
    /// File path for an inline comment; `None` for a conversation comment.
    pub(crate) path: Option<String>,
    /// Line on the diff the comment anchors to.
    pub(crate) line: Option<i64>,
    /// `RIGHT` (head) or `LEFT` (base) for an inline comment.
    pub(crate) side: Option<String>,
    /// The review-thread node id (GraphQL), shared by every comment in a thread.
    pub(crate) thread_id: Option<String>,
    /// Whether the owning review thread is resolved.
    pub(crate) is_resolved: bool,
}

/// A compact rollup of the head commit's status checks.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChecksSummary {
    pub(crate) total: i64,
    pub(crate) passing: i64,
    pub(crate) failing: i64,
    pub(crate) pending: i64,
}

/// The full review payload for one PR.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PrDetail {
    pub(crate) summary: PrSummary,
    pub(crate) body: String,
    pub(crate) head_sha: String,
    pub(crate) commits: Vec<PrCommit>,
    pub(crate) files: Vec<PrFile>,
    pub(crate) comments: Vec<PrComment>,
    pub(crate) checks: Option<ChecksSummary>,
    /// True when the changed-file list was capped at `MAX_FILES`.
    pub(crate) files_truncated: bool,
}

/// Result of comparing a locally-open PR against the current remote.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PrFreshness {
    pub(crate) stale: bool,
    pub(crate) remote_head: String,
    pub(crate) remote_updated_at: String,
}

/// One pending inline comment to attach to a submitted review.
#[derive(Clone, Debug, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PendingComment {
    pub(crate) path: String,
    pub(crate) line: i64,
    #[serde(default)]
    pub(crate) side: Option<String>,
    pub(crate) body: String,
}
