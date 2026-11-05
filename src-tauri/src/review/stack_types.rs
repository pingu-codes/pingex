//! Wire types for the Stack page: the stack as the inbox knows it, what is
//! on disk for it, the restack plan and progress, and the stack edits.

use serde::{Deserialize, Serialize};

/// A stack as the inbox has it: GitHub's native stack joined with each PR's
/// inbox facts. Bottom first, merged PRs included (GitHub keeps them in the
/// listing).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StackInput {
    /// GitHub's stack number.
    pub(crate) number: i64,
    pub(crate) name: String,
    /// The git branch the bottom PR merges into.
    pub(crate) base_ref: String,
    /// The base git branch's tip as GitHub last said; fetched if missing.
    pub(crate) base_sha: String,
    pub(crate) prs: Vec<StackInputPr>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StackInputPr {
    pub(crate) number: i64,
    pub(crate) title: String,
    pub(crate) head_ref: String,
    /// The head GitHub has, which is also what a push leases against.
    pub(crate) head_sha: String,
    pub(crate) open: bool,
    pub(crate) merged: bool,
}

impl StackInput {
    /// The open, unmerged PRs, bottom first: the ones a stack edit moves.
    pub(crate) fn open_prs(&self) -> Vec<&StackInputPr> {
        self.prs.iter().filter(|pr| pr.open && !pr.merged).collect()
    }
}

/// One commit, for the base's "N commits ahead" list and the split picker.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CommitLine {
    pub(crate) sha: String,
    pub(crate) subject: String,
}

/// The worktree a stack lives in: one of its git branches is checked out
/// there.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StackWorktree {
    pub(crate) path: String,
    /// The git branch checked out there now.
    pub(crate) branch: Option<String>,
    pub(crate) clean: bool,
}

/// One open PR's git branch, as it is on disk.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StackBranch {
    pub(crate) number: i64,
    pub(crate) head_ref: String,
    /// The local git branch's tip, or the PR head when there is no local
    /// branch.
    pub(crate) tip: String,
    /// Commits on this git branch over the one below it (or the base).
    pub(crate) commits: i64,
    /// The local git branch has commits GitHub does not, or differs from it.
    pub(crate) unpushed: bool,
    pub(crate) commit_lines: Vec<CommitLine>,
}

/// What the Stack page shows besides the PRs' own facts.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StackOverview {
    pub(crate) number: i64,
    /// Commits on the base the stack does not have yet ("behind main by").
    pub(crate) base_ahead: i64,
    /// The newest of those commits, newest first, at most a handful.
    pub(crate) base_commits: Vec<CommitLine>,
    /// Open PRs, bottom first.
    pub(crate) branches: Vec<StackBranch>,
    pub(crate) worktree: Option<StackWorktree>,
    /// Merged PRs whose git branch is still here: the merge follow-up.
    pub(crate) merged_branches: Vec<i64>,
    /// A restack stopped at a conflict, waiting to continue or abort.
    pub(crate) restack: Option<RestackProgress>,
}

/// How far behind its base one stack is, for the inbox badge.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StackBehind {
    pub(crate) number: i64,
    /// `None` when the commits are not here to count.
    pub(crate) behind: Option<i64>,
}

/// One git branch in a restack plan.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RestackPlanStep {
    pub(crate) number: i64,
    pub(crate) head_ref: String,
    /// What it is replayed onto: the base or the git branch below.
    pub(crate) onto: String,
    /// Commits replayed.
    pub(crate) commits: i64,
    /// Already on what it would be replayed onto: nothing to do.
    pub(crate) unchanged: bool,
    /// Files expected to conflict, from a trial merge.
    pub(crate) conflicts: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RestackPlan {
    pub(crate) onto_ref: String,
    pub(crate) steps: Vec<RestackPlanStep>,
    /// PRs whose pushed commits the restack rewrites (a force-push later).
    pub(crate) rewrites_pushed: Vec<i64>,
    /// Whether `conflicts` could be predicted (needs git 2.40).
    pub(crate) predicted: bool,
    /// The worktree it runs in; `None` until the stack has one.
    pub(crate) worktree: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) enum RestackStatus {
    #[default]
    Waiting,
    Replayed,
    Unchanged,
    Conflict,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RestackBranch {
    pub(crate) number: i64,
    pub(crate) head_ref: String,
    pub(crate) status: RestackStatus,
    pub(crate) commits: i64,
    pub(crate) conflicts: Vec<String>,
}

/// Where a restack is: every git branch's status, and whether it finished.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RestackProgress {
    pub(crate) stack: i64,
    pub(crate) worktree: String,
    pub(crate) onto_ref: String,
    pub(crate) branches: Vec<RestackBranch>,
    pub(crate) done: bool,
}

impl RestackProgress {
    /// The git branch stopped at a conflict, if any.
    pub(crate) fn conflict(&self) -> Option<&RestackBranch> {
        self.branches
            .iter()
            .find(|branch| branch.status == RestackStatus::Conflict)
    }
}

/// What pushing a stack sent.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PushReport {
    /// The git branches pushed; the rest were already up to date.
    pub(crate) pushed: Vec<String>,
}
