//! The Changes page's read model: what a thread's repositories have changed,
//! split into Change segments the way the review desk splits a stack.
//!
//! A thread can work in several repositories (its own directory, and each
//! member of a multi-project workspace) and, in a Stack worktree, on several
//! git branches. Every repository is read from its own top level, never from
//! the thread's directory, so paths are always root-relative.
//!
//! Two scopes answer two questions:
//! - **Branch**: what each git branch adds over its base, as its pull request
//!   would show it, plus the working tree's uncommitted changes.
//! - **This thread**: what moved since the thread's start tips were recorded
//!   (`storage::thread_snapshots`), plus the working tree.
//!
//! Every committed segment carries a `base_sha` chosen so the straight
//! two-dot diff `base..head` is its answer; the patch read is SHA-keyed and
//! cached. The uncommitted segment has no head: it is the working tree.

use serde::{Deserialize, Serialize};

use crate::review::worktree_purpose::WorktreePurpose;

pub(crate) mod commands;
mod discover;
mod listing;
mod refs;
mod segments;
pub(crate) mod snapshot;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ChangesScope {
    Branch,
    Thread,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) enum SegmentKind {
    /// One pull request of a stack: its git branch against the one below.
    StackPr,
    /// The checked-out git branch against its base.
    Branch,
    /// The working tree, untracked files included, against `HEAD`.
    Uncommitted,
    /// A git branch that moved since the thread started.
    Moved,
    /// A git branch made since the thread started.
    NewBranch,
    /// A git branch deleted since the thread started.
    DeletedBranch,
}

/// One file in a segment's listing (counts only, no patch).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[serde(rename = "ChangeSegmentFile")]
pub(crate) struct ChangedFile {
    pub(crate) path: String,
    pub(crate) old_path: Option<String>,
    /// `added`, `modified`, `deleted`, `renamed`, or `untracked`.
    pub(crate) status: String,
    pub(crate) additions: u64,
    pub(crate) deletions: u64,
    pub(crate) binary: bool,
    /// Names the file's new contents (a blob id), so "viewed" resets when the
    /// file changes again. `None` past the hashing cap.
    pub(crate) content_key: Option<String>,
    /// The file already had uncommitted changes when the thread started.
    pub(crate) before_thread: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChangeSegment {
    /// Stable across reads: `uncommitted`, `pr:<n>`, `branch:<name>`,
    /// `moved:<name>`, `new:<name>`, `deleted:<name>`.
    pub(crate) id: String,
    pub(crate) kind: SegmentKind,
    pub(crate) label: String,
    pub(crate) branch: Option<String>,
    pub(crate) pr_number: Option<i64>,
    /// What the segment is compared with, in words ("origin/main", "#101",
    /// "tip at thread start").
    pub(crate) base_label: String,
    pub(crate) base_sha: String,
    /// `None` for the working tree.
    pub(crate) head_sha: Option<String>,
    /// The git branch checked out (or, for `uncommitted`, always true).
    pub(crate) current: bool,
    /// The branch was rewritten (rebased, amended) rather than extended, so
    /// a tip-to-tip diff can include changes from its base.
    pub(crate) rewritten: bool,
    /// The PR below moved and this branch is not on top of it any more.
    pub(crate) needs_restack: bool,
    /// Why the segment has no files when it should, in words.
    pub(crate) note: Option<String>,
    pub(crate) files: Vec<ChangedFile>,
    pub(crate) total_files: usize,
    pub(crate) truncated: bool,
    pub(crate) additions: u64,
    pub(crate) deletions: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RepoChanges {
    /// The worktree's top level, as a host path.
    pub(crate) root: String,
    /// The workspace member alias, else the folder name.
    pub(crate) label: String,
    pub(crate) branch: Option<String>,
    pub(crate) head_sha: Option<String>,
    /// `rebase` or `merge` while one is stopped half way.
    pub(crate) in_progress: Option<String>,
    pub(crate) purpose: Option<WorktreePurpose>,
    pub(crate) segments: Vec<ChangeSegment>,
    /// The repository could not be read; the others still are.
    pub(crate) error: Option<String>,
    /// Something to know about the segments, in words (no start tips yet).
    pub(crate) note: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ThreadChanges {
    pub(crate) scope: ChangesScope,
    pub(crate) repos: Vec<RepoChanges>,
    /// When the earliest start tips were recorded (Thread scope).
    pub(crate) snapshot_at: Option<i64>,
    /// Unix milliseconds of this read.
    pub(crate) read_at: i64,
}
