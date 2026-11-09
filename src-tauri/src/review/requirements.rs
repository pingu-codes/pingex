//! What each desk action needs on disk: one table, shared with the frontend.
//!
//! A PR is either not on disk (No checkout) or in a worktree, and moves up only
//! when an action needs it. The table is exported into `bindings.ts` as
//! `DESK_ACTION_STEPS`; `src/lib/review/actions.ts` reads it there, so the two
//! cannot drift. Buttons read it to name the step they create.
//!
//! Adding an action: add a `DeskAction` variant, its row in `needs`, its row
//! in the test below, and its label in `src/lib/review/actions.ts`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Where a PR's code lives, lowest first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) enum CheckoutStep {
    /// The PR's commits as fetched refs; the working tree is untouched.
    NoCheckout,
    /// A worktree on a local branch at the PR's head, where agents and people
    /// can build, run and commit.
    Worktree,
}

/// By hand because stored agent reviews may still say `reviewCheckout`, the
/// detached step `Worktree` replaced, and the bindings exporter refuses
/// `#[serde(alias)]`.
impl<'de> Deserialize<'de> for CheckoutStep {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match String::deserialize(deserializer)?.as_str() {
            "noCheckout" => Ok(Self::NoCheckout),
            "worktree" | "reviewCheckout" => Ok(Self::Worktree),
            other => Err(serde::de::Error::unknown_variant(
                other,
                &["noCheckout", "worktree"],
            )),
        }
    }
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
    /// Start a thread in the PR's worktree, to build, run, measure or change it.
    WorkOnPr,
    FixInThread,
    AddressReview,
    FixCi,
    /// Push to the PR's git branch.
    Push,
    Restack,
    ResolveConflicts,
}

impl DeskAction {
    pub(crate) const ALL: [DeskAction; 22] = [
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
        Self::WorkOnPr,
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
            Self::AgentReviewWithTests
            | Self::RunLocally
            | Self::OpenInEditor
            | Self::WorkOnPr
            | Self::FixInThread
            | Self::AddressReview
            | Self::FixCi
            | Self::Push
            | Self::Restack
            | Self::ResolveConflicts => CheckoutStep::Worktree,
        }
    }

    /// Whether the repository's PR worktree policy decides if this action may
    /// make a worktree. The others are explicit requests to change the PR, so
    /// they always ask and are never refused.
    pub(crate) const fn follows_policy(self) -> bool {
        matches!(
            self,
            Self::AgentReviewWithTests | Self::RunLocally | Self::OpenInEditor
        )
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

    #[test]
    fn each_action_needs_the_step_it_works_at() {
        use CheckoutStep::{NoCheckout, Worktree};
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
            (AgentReviewWithTests, Worktree),
            (RunLocally, Worktree),
            (OpenInEditor, Worktree),
            (WorkOnPr, Worktree),
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
        assert_eq!(table["runLocally"], "worktree");
        assert_eq!(table["fixCi"], "worktree");
        assert_eq!(table["read"], "noCheckout");
    }

    #[test]
    fn a_stored_review_checkout_step_reads_as_a_worktree() {
        let step: CheckoutStep = serde_json::from_str("\"reviewCheckout\"").unwrap();
        assert_eq!(step, CheckoutStep::Worktree);
    }
}
