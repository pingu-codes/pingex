//! Wire types for checkout steps, PR worktrees and the PR link on threads.

use serde::{Deserialize, Serialize};

use super::requirements::CheckoutStep;

/// The PR a checkout operation is for, as the inbox row has it.
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
}

/// Where a PR's code is now.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PrCheckout {
    pub(crate) number: i64,
    pub(crate) step: CheckoutStep,
    /// The worktree's directory, on the repository's Host.
    pub(crate) path: Option<String>,
    /// The commit checked out there.
    pub(crate) head_sha: Option<String>,
    /// The worktree's git branch.
    pub(crate) git_branch: Option<String>,
    /// The setup command still to run in a new worktree.
    pub(crate) setup_pending: Option<String>,
    /// Commits here that the PR's head does not have.
    pub(crate) ahead: i64,
    /// Commits on the PR's head that are not here.
    pub(crate) behind: i64,
    /// Uncommitted or untracked files here.
    pub(crate) dirty: bool,
    /// Pingex looks after this worktree and removes it once its PR has closed
    /// and it holds no work. Otherwise it is the user's.
    pub(crate) managed: bool,
}

impl PrCheckout {
    pub(crate) fn none(number: i64) -> Self {
        Self {
            number,
            step: CheckoutStep::NoCheckout,
            path: None,
            head_sha: None,
            git_branch: None,
            setup_pending: None,
            ahead: 0,
            behind: 0,
            dirty: false,
            managed: false,
        }
    }
}

/// What has to be confirmed before stepping up.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) enum StepConfirm {
    /// The PR has no checkout: create a worktree for it.
    CreateWorktree,
    /// Pushing to a git branch someone else owns. Asked every time.
    PushToTheirBranch,
}

/// How to get a PR to the step an action needs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StepPlan {
    pub(crate) current: PrCheckout,
    /// The step to move up to, or `None` when the PR already has enough.
    pub(crate) step_up: Option<CheckoutStep>,
    /// What to confirm first: the step up, or, with no step up, the push
    /// to someone else's git branch.
    pub(crate) confirm: Option<StepConfirm>,
    /// The action pushes to someone else's git branch, which is confirmed
    /// every time: after the step up when `confirm` was for that.
    pub(crate) confirm_push: bool,
    /// Why the action cannot go ahead (the repository says Never).
    pub(crate) refused: Option<String>,
    /// Where a new worktree would go, for the confirmation.
    pub(crate) worktree_path: String,
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
