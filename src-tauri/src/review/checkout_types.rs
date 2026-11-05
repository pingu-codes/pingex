//! Wire types for checkout steps and the PR link on threads.

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
    /// A stack's name: its PRs share one review checkout that switches heads.
    pub(crate) stack: Option<String>,
}

/// Where a PR's code is now.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PrCheckout {
    pub(crate) number: i64,
    pub(crate) step: CheckoutStep,
    /// The review checkout's or worktree's directory, on the repository's Host.
    pub(crate) path: Option<String>,
    /// The commit checked out there.
    pub(crate) head_sha: Option<String>,
    /// The worktree's git branch.
    pub(crate) git_branch: Option<String>,
    /// The setup command still to run in a new review checkout.
    pub(crate) setup_pending: Option<String>,
    /// A review checkout that has local changes, so it stayed on an older
    /// head when the PR moved.
    pub(crate) behind_head: bool,
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
            behind_head: false,
        }
    }
}

/// What has to be confirmed before stepping up.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) enum StepConfirm {
    /// The first review checkout in a repository whose policy is Ask first.
    CreateReviewCheckout,
    /// The PR has a review checkout: promote it, or create a worktree.
    PromoteOrCreateWorktree,
    /// The PR has no checkout: create a worktree on its git branch.
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
    pub(crate) confirm: Option<StepConfirm>,
    /// Why the action cannot go ahead (the repository says Never).
    pub(crate) refused: Option<String>,
    /// Where a new review checkout would go, for the confirmation.
    pub(crate) review_checkout_path: String,
}

/// What pruning closed PRs removed.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PruneReport {
    /// PRs whose review checkout was removed.
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
