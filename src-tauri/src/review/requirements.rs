//! What each desk action needs on disk: one table, shared with the frontend.
//!
//! A PR moves up the checkout steps (No checkout, Review checkout, Worktree)
//! only when an action needs it, and only as far as that action needs. The
//! table is the canvas's Model board, "What each action needs". It is
//! exported into `bindings.ts` as `DESK_ACTION_STEPS`; `src/lib/review/
//! actions.ts` reads it there, so the two cannot drift. Buttons read it to
//! name the step they create; stepping a PR up goes only as far as
//! `DeskAction::needs`.
//!
//! Adding an action: add a `DeskAction` variant, its row in `needs`, its row
//! in the test below, and its label in `src/lib/review/actions.ts`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Where a PR's code lives, lowest first. Each step adds to the one below.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, specta::Type,
)]
#[serde(rename_all = "camelCase")]
pub(crate) enum CheckoutStep {
    /// The PR's commits as fetched refs; the working tree is untouched.
    NoCheckout,
    /// A detached, Pingex-owned checkout at the PR head that can run code.
    ReviewCheckout,
    /// A named worktree on the PR's git branch, where commits and pushes go.
    Worktree,
}

/// A desk action that touches a PR's code, or explicitly does not.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, specta::Type,
)]
#[serde(rename_all = "camelCase")]
pub(crate) enum DeskAction {
    Read,
    Comment,
    SubmitReview,
    ResolveThread,
    ViewWholeFile,
    ChangesSince,
    Merge,
    Close,
    Retarget,
    MarkReady,
    /// An agent review that reads the patch and runs nothing.
    DiffReview,
    /// Ask an agent about a line or a file.
    AskAgent,
    /// An agent review with "Run tests" on.
    AgentReviewWithTests,
    RunLocally,
    OpenInEditor,
    FixInThread,
    AddressReview,
    FixCi,
    /// Push to the PR's git branch.
    Push,
    Restack,
    ResolveConflicts,
}

impl DeskAction {
    pub(crate) const ALL: [DeskAction; 21] = [
        Self::Read,
        Self::Comment,
        Self::SubmitReview,
        Self::ResolveThread,
        Self::ViewWholeFile,
        Self::ChangesSince,
        Self::Merge,
        Self::Close,
        Self::Retarget,
        Self::MarkReady,
        Self::DiffReview,
        Self::AskAgent,
        Self::AgentReviewWithTests,
        Self::RunLocally,
        Self::OpenInEditor,
        Self::FixInThread,
        Self::AddressReview,
        Self::FixCi,
        Self::Push,
        Self::Restack,
        Self::ResolveConflicts,
    ];

    /// The lowest checkout step this action works at.
    pub(crate) const fn needs(self) -> CheckoutStep {
        match self {
            Self::Read
            | Self::Comment
            | Self::SubmitReview
            | Self::ResolveThread
            | Self::ViewWholeFile
            | Self::ChangesSince
            | Self::Merge
            | Self::Close
            | Self::Retarget
            | Self::MarkReady
            | Self::DiffReview
            | Self::AskAgent => CheckoutStep::NoCheckout,
            Self::AgentReviewWithTests | Self::RunLocally | Self::OpenInEditor => {
                CheckoutStep::ReviewCheckout
            }
            Self::FixInThread
            | Self::AddressReview
            | Self::FixCi
            | Self::Push
            | Self::Restack
            | Self::ResolveConflicts => CheckoutStep::Worktree,
        }
    }
}

/// The table, as exported to the frontend.
pub(crate) fn action_steps() -> BTreeMap<DeskAction, CheckoutStep> {
    DeskAction::ALL
        .into_iter()
        .map(|action| (action, action.needs()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The "What each action needs" table on the canvas's Model board.
    #[test]
    fn each_action_needs_the_step_the_model_board_gives_it() {
        use CheckoutStep::{NoCheckout, ReviewCheckout, Worktree};
        use DeskAction::*;
        let expected = [
            (Read, NoCheckout),
            (Comment, NoCheckout),
            (SubmitReview, NoCheckout),
            (ResolveThread, NoCheckout),
            (ViewWholeFile, NoCheckout),
            (ChangesSince, NoCheckout),
            (Merge, NoCheckout),
            (Close, NoCheckout),
            (Retarget, NoCheckout),
            (MarkReady, NoCheckout),
            (DiffReview, NoCheckout),
            (AskAgent, NoCheckout),
            (AgentReviewWithTests, ReviewCheckout),
            (RunLocally, ReviewCheckout),
            (OpenInEditor, ReviewCheckout),
            (FixInThread, Worktree),
            (AddressReview, Worktree),
            (FixCi, Worktree),
            (Push, Worktree),
            (Restack, Worktree),
            (ResolveConflicts, Worktree),
        ];
        assert_eq!(
            DeskAction::ALL.len(),
            expected.len(),
            "every action is listed"
        );
        for (action, step) in expected {
            assert_eq!(action.needs(), step, "{action:?}");
        }
    }

    #[test]
    fn the_exported_table_names_actions_and_steps_as_the_frontend_does() {
        let table = serde_json::to_value(action_steps()).unwrap();
        assert_eq!(table["runLocally"], "reviewCheckout");
        assert_eq!(table["fixCi"], "worktree");
        assert_eq!(table["read"], "noCheckout");
    }
}
