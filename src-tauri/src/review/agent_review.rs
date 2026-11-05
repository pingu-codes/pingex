//! Agent reviews: what the reviewer asked for in Start review, and the
//! queued, running or finished run that came of it.
//!
//! An agent review is a thread on the chosen harness, filed under the PR
//! (its PR link). The desk keeps the record and the queue; the frontend
//! starts the thread when the queue lets it, and hands the final message
//! back for its Findings (`findings`).

use serde::{Deserialize, Serialize};

use super::checkout_types::CheckoutTarget;
use super::requirements::CheckoutStep;
use crate::harness::HarnessKind;

/// How many agent reviews run at once, across every repository of a Home.
pub(crate) const MAX_RUNNING: usize = 2;

/// What the review should look at, from the dialog's focus pills.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) enum FocusArea {
    Correctness,
    Tests,
    Accessibility,
    Performance,
    /// Follow the repository's agent instructions (AGENTS.md, CLAUDE.md).
    AgentInstructions,
    /// Run the tests; needs a review checkout.
    RunTests,
}

impl FocusArea {
    /// The line the prompt gives the area.
    pub(crate) fn instruction(self) -> &'static str {
        match self {
            Self::Correctness => "Correctness: bugs, edge cases, wrong behaviour.",
            Self::Tests => "Tests: missing or weak tests for the change.",
            Self::Accessibility => "Accessibility: keyboard use, labels, contrast, screen readers.",
            Self::Performance => "Performance: needless work, slow paths, memory.",
            Self::AgentInstructions => {
                "Follow the repository's agent instructions (AGENTS.md, CLAUDE.md) and flag where the change breaks them."
            }
            Self::RunTests => {
                "Run the repository's tests in this checkout and report failures as findings. Do not edit files."
            }
        }
    }
}

/// What Start review asked for.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentReviewRequest {
    /// The PRs reviewed as one unit: one PR, or a whole stack bottom first.
    /// The review is filed under the last.
    pub(crate) targets: Vec<CheckoutTarget>,
    pub(crate) harness: HarnessKind,
    /// The checkout step the agent works at.
    pub(crate) step: CheckoutStep,
    pub(crate) focus: Vec<FocusArea>,
    /// Findings stay drafts only the reviewer sees until added to the review.
    pub(crate) keep_drafts: bool,
    /// Queue a re-run, of only the new commits, when the PR's head moves.
    pub(crate) rerun_on_new_commits: bool,
}

impl AgentReviewRequest {
    /// The PR the review is filed under: the top of a stack.
    pub(crate) fn filed_under(&self) -> Option<&CheckoutTarget> {
        self.targets.last()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) enum AgentReviewStatus {
    Queued,
    Running,
    /// Finished; `parse_error` says whether its Findings could be read.
    Done,
    /// The thread or its turn failed; `error` says how.
    Failed,
}

impl AgentReviewStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Done => "done",
            Self::Failed => "failed",
        }
    }

    pub(crate) fn parse(value: &str) -> Self {
        match value {
            "running" => Self::Running,
            "done" => Self::Done,
            "failed" => Self::Failed,
            _ => Self::Queued,
        }
    }
}

/// One run of an agent review.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentReview {
    pub(crate) id: String,
    /// The clone the review was started from, as projects store it.
    pub(crate) repo_dir: String,
    /// `owner/name`.
    pub(crate) repo: String,
    /// The PR it is filed under.
    pub(crate) number: i64,
    pub(crate) request: AgentReviewRequest,
    pub(crate) status: AgentReviewStatus,
    pub(crate) thread_id: Option<String>,
    /// The head reviewed (a stack's top).
    pub(crate) head_sha: String,
    /// A re-run reviews only the commits after this head.
    pub(crate) since_sha: Option<String>,
    /// How many Findings it reported.
    pub(crate) findings: i64,
    /// Set when the final message had no readable findings block.
    pub(crate) parse_error: Option<String>,
    pub(crate) error: Option<String>,
    /// Unix milliseconds.
    pub(crate) created_at: i64,
    pub(crate) started_at: Option<i64>,
    pub(crate) finished_at: Option<i64>,
}

/// A repository review rule: a kind of Finding the reviewer asked agents to
/// skip. Every later agent review's prompt carries it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReviewRule {
    pub(crate) id: String,
    pub(crate) rule: String,
    /// Unix milliseconds.
    pub(crate) created_at: i64,
}
