//! Wire types for PR worktrees and the PR link on threads.

use serde::{Deserialize, Serialize};

use super::stack_types::KeptBranch;

/// A PR as the desk works on it: its head, whether it is mine and its place
/// in a stack. The desk derives it (`ReviewDesk::target_for`); agent review
/// requests still carry the heads they were queued for.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CheckoutTarget {
    pub(crate) number: i64,
    /// The PR's git branch.
    pub(crate) head_ref: String,
    pub(crate) head_sha: String,
    /// Whether I wrote the PR.
    pub(crate) mine: bool,
    /// A stack's name. Stack actions work on the PRs' real git branches, so a
    /// stack's worktree is on one whoever wrote it.
    pub(crate) stack: Option<String>,
    /// The `owner/name` the PR's head lives in; `None` when it was deleted.
    #[serde(default)]
    pub(crate) head_repo: Option<String>,
    /// The git branches of the stack's open PRs, bottom first.
    #[serde(default)]
    pub(crate) stack_branches: Vec<String>,
}

/// A checked-out PR's worktree, as it is on disk now. A PR that is not
/// checked out has none.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PrCheckout {
    pub(crate) number: i64,
    /// The worktree's directory, on the repository's Host.
    pub(crate) path: String,
    /// The commit checked out there.
    pub(crate) head_sha: Option<String>,
    /// The git branch checked out there.
    pub(crate) git_branch: Option<String>,
    /// The setup command still to run in a new worktree.
    pub(crate) setup_pending: Option<String>,
    /// Commits here that the PR's head does not have.
    pub(crate) ahead: i64,
    /// Commits on the PR's head that are not here.
    pub(crate) behind: i64,
    /// Uncommitted or untracked files here.
    pub(crate) dirty: bool,
    /// Pingex found this worktree rather than made it, so it never switches,
    /// fast-forwards or removes it. Otherwise Pingex removes it once its PR
    /// (or every PR of its stack) has closed and it holds no work.
    pub(crate) adopted: bool,
    /// The worktree is on this PR's own git branch. False when it is on
    /// another branch, as a stack's worktree is for all but one of its PRs;
    /// `ahead` and `behind` then count from this PR's local branch, and
    /// `git_branch` names the one checked out.
    pub(crate) on_pr_branch: bool,
    /// The directory is the repository's main clone, not a linked worktree.
    pub(crate) main_checkout: bool,
    /// GitHub's number for the stack whose worktree this is.
    pub(crate) stack: Option<i64>,
}

/// What Check out did.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(crate) enum CheckOutOutcome {
    /// The PR (or every open PR of its stack) is checked out.
    CheckedOut {
        /// The PR asked for; for a stack, its top open PR.
        checkout: PrCheckout,
        /// A new worktree was made; otherwise it had one.
        created: bool,
        /// Local git branches made for a stack's PRs.
        created_branches: Vec<String>,
        /// A stack's git branches left where they were.
        kept: Vec<KeptBranch>,
        /// Why the provider's stack tool (`gh stack`) doesn't track the stack
        /// in the worktree, when it is installed and should.
        local_stack_warning: Option<String>,
    },
    /// The git branch the worktree would be on is checked out at `path`
    /// already. `review_adopt_worktree` makes that the PR's worktree.
    BranchInUse {
        /// The PR whose branch it is.
        number: i64,
        branch: String,
        path: String,
        /// `path` is the repository's main clone.
        main_clone: bool,
    },
}

/// The PR a Pingex-managed worktree belongs to, found from its directory.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ManagedPrWorktree {
    /// The repository's clone, as the desk is keyed.
    pub(crate) repo_dir: String,
    pub(crate) number: i64,
}

/// What pruning closed PRs removed.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PruneReport {
    /// PRs whose managed worktree was removed.
    pub(crate) removed_checkouts: Vec<i64>,
    /// PRs seen closed for the first time.
    pub(crate) closed: Vec<i64>,
}

/// How far a thread's git branch has moved since the thread started.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BranchProgress {
    pub(crate) start_sha: String,
    /// The git branch's tip now; `None` when the branch is gone.
    pub(crate) tip_sha: Option<String>,
    /// Commits on the tip that the start does not have.
    pub(crate) ahead: i64,
    /// Commits at the start that the tip no longer has (a force-push).
    pub(crate) behind: i64,
}
