//! Worktree context: what an agent starting in a managed PR worktree or a
//! Stack worktree is told about where it is. Pure: the desk reads the purpose
//! and the branch.
//!
//! The frontend appends the text to a new thread's first message inside
//! `<worktree_context>` (`src/lib/thread/worktreeContext.ts` owns the tag
//! there). It is part of the user's message on every harness, so anything
//! that reads the message back for a title strips it with
//! `strip_worktree_context`.

use std::fmt::Write;

use serde::Serialize;

use super::worktree_purpose::WorktreePurpose;

/// Mirrors `OPEN` and `CLOSE` in `src/lib/thread/worktreeContext.ts`.
const OPEN: &str = "<worktree_context>";
const CLOSE: &str = "</worktree_context>";

/// What the worktree a draft starts in is for, and what to tell the agent.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorktreeContext {
    pub(crate) purpose: WorktreePurpose,
    pub(crate) text: String,
}

/// The context for a worktree with `purpose`, on `branch` now (`None` when
/// detached).
pub(crate) fn worktree_context(purpose: &WorktreePurpose, branch: Option<&str>) -> String {
    let mut out = String::new();
    match purpose {
        WorktreePurpose::ManagedPr { number, .. } => {
            let _ = writeln!(
                out,
                "This directory is a worktree for pull request #{number}, which is someone else's or comes from a fork. Pingex made it and removes it once the PR has closed and it holds no work."
            );
            match branch {
                Some(branch) => {
                    let _ = write!(out, "It is on the local branch `{branch}`, which");
                }
                None => out.push_str("Its local branch"),
            }
            out.push_str(" has no upstream, so commits stay local. Don't push and don't set an upstream: the user pushes to the PR from Pingex.");
        }
        WorktreePurpose::Stack {
            name,
            base_ref,
            prs,
            ..
        } => {
            let _ = writeln!(
                out,
                "This directory is the worktree of the stack \"{name}\": pull requests that build on each other, each on its own git branch. Bottom first, on top of `{base_ref}`:"
            );
            for (index, pr) in prs.iter().enumerate() {
                let _ = writeln!(
                    out,
                    "{}. #{} {} (`{}`)",
                    index + 1,
                    pr.number,
                    pr.title,
                    pr.head_ref
                );
            }
            if let Some(current) = branch.and_then(|name| prs.iter().find(|pr| pr.head_ref == name))
            {
                let _ = writeln!(
                    out,
                    "The worktree is on `{}` (#{}) now.",
                    current.head_ref, current.number
                );
            }
            out.push_str("Before you change anything, work out which pull request the change belongs to, and commit it on that PR's branch. Switch with `git switch <branch>`; the worktree must be clean first. A change to a lower PR leaves the ones above it needing a restack: rebase each of them onto the branch below it, bottom first, and say that you did.\nThis list is from Pingex's last read of the stack.");
        }
    }
    out
}

/// `text` without any worktree context block, for titles.
pub(crate) fn strip_worktree_context(text: &str) -> String {
    let mut rest = text;
    let mut out = String::new();
    while let Some(start) = rest.find(OPEN) {
        let Some(length) = rest[start..].find(CLOSE) else {
            break;
        };
        out.push_str(rest[..start].trim_end());
        rest = &rest[start + length + CLOSE.len()..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::worktree_purpose::StackMarkerPr;

    fn stack() -> WorktreePurpose {
        WorktreePurpose::Stack {
            repo_dir: "/repo".into(),
            number: 7,
            name: "billing".into(),
            base_ref: "main".into(),
            prs: vec![
                StackMarkerPr {
                    number: 101,
                    title: "Add the ledger".into(),
                    head_ref: "billing/ledger".into(),
                },
                StackMarkerPr {
                    number: 102,
                    title: "Charge from the ledger".into(),
                    head_ref: "billing/charge".into(),
                },
            ],
        }
    }

    #[test]
    fn a_stack_lists_its_prs_bottom_first_and_names_the_one_checked_out() {
        let text = worktree_context(&stack(), Some("billing/charge"));
        assert!(text.starts_with("This directory is the worktree of the stack \"billing\""));
        assert!(text.contains("on top of `main`:\n1. #101 Add the ledger (`billing/ledger`)\n2. #102 Charge from the ledger (`billing/charge`)\n"));
        assert!(text.contains("The worktree is on `billing/charge` (#102) now.\n"));
        assert!(text.contains("work out which pull request the change belongs to"));
        assert!(text.contains("rebase each of them onto the branch below it"));
    }

    #[test]
    fn a_detached_stack_worktree_names_no_current_pr() {
        let text = worktree_context(&stack(), None);
        assert!(!text.contains("The worktree is on"));
        assert!(text.contains("2. #102"));
    }

    #[test]
    fn a_managed_pr_worktree_says_whose_it_is_and_not_to_push() {
        let purpose = WorktreePurpose::ManagedPr {
            repo_dir: "/repo".into(),
            number: 57,
        };
        let text = worktree_context(&purpose, Some("pr/57"));
        assert!(text.contains("a worktree for pull request #57"));
        assert!(text.contains("It is on the local branch `pr/57`, which has no upstream"));
        assert!(text.contains("Don't push"));
    }

    #[test]
    fn stripping_removes_the_block_wherever_the_harness_left_it() {
        let block = "<worktree_context>\nThis directory is…\n</worktree_context>";
        assert_eq!(
            strip_worktree_context(&format!("Fix the tax bug\n\n{block}")),
            "Fix the tax bug"
        );
        assert_eq!(
            strip_worktree_context(&format!("Fix the tax bug\n\n{block}\n\nOn it.")),
            "Fix the tax bug\n\nOn it."
        );
        assert_eq!(strip_worktree_context("No block"), "No block");
        assert_eq!(
            strip_worktree_context("An open <worktree_context> only"),
            "An open <worktree_context> only"
        );
    }
}
