//! What a worktree is for, when Pingex knows: a managed PR worktree or a
//! Stack worktree.
//!
//! A managed PR worktree is known by its `review_checkouts` row and its
//! `pr/<n>` branch. A Stack worktree is known by a `stack_worktrees` row,
//! written whenever the desk finds or makes one. That row is a cache for
//! listings and for worktree context: where a stack lives is still decided by
//! the git branch checked out there (`ReviewDesk::stack_worktree`), so a reader
//! that needs the truth checks the branch.

use serde::{Deserialize, Serialize};

use super::stack_types::StackInput;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(crate) enum WorktreePurpose {
    /// Someone else's PR or a fork's, on `pr/<n>`, removed by Pingex once the
    /// PR has closed.
    ManagedPr { repo_dir: String, number: i64 },
    /// The one worktree a stack lives in.
    Stack {
        repo_dir: String,
        /// GitHub's stack number.
        number: i64,
        name: String,
        /// The git branch the bottom PR merges into.
        base_ref: String,
        /// The open PRs, bottom first, as last read from GitHub.
        prs: Vec<StackMarkerPr>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StackMarkerPr {
    pub(crate) number: i64,
    pub(crate) title: String,
    pub(crate) head_ref: String,
}

impl WorktreePurpose {
    pub(crate) fn stack(repo_dir: &str, input: &StackInput) -> Self {
        Self::Stack {
            repo_dir: repo_dir.to_string(),
            number: input.number,
            name: input.name.clone(),
            base_ref: input.base_ref.clone(),
            prs: input
                .open_prs()
                .into_iter()
                .map(|pr| StackMarkerPr {
                    number: pr.number,
                    title: pr.title.clone(),
                    head_ref: pr.head_ref.clone(),
                })
                .collect(),
        }
    }

    /// Whether a worktree on `branch` can still be this: a stack's worktree is
    /// on one of its git branches, or detached by a stopped restack.
    pub(crate) fn holds_on(&self, branch: Option<&str>) -> bool {
        match self {
            Self::ManagedPr { .. } => true,
            Self::Stack { prs, .. } => {
                branch.is_none_or(|name| prs.iter().any(|pr| pr.head_ref == name))
            }
        }
    }
}
