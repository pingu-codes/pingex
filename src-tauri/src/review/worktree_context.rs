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
    /// Where the worktree's git branch is in the stack, when it is one of
    /// the stack's PRs.
    pub(crate) position: Option<StackPosition>,
}

/// A PR's place in its stack, counted from the bottom: 1 is the PR on the
/// stack's base.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StackPosition {
    pub(crate) position: u32,
    pub(crate) total: u32,
}

/// Where `branch` is in a Stack `purpose`; `None` for anything else.
pub(crate) fn stack_position(
    purpose: &WorktreePurpose,
    branch: Option<&str>,
) -> Option<StackPosition> {
    let WorktreePurpose::Stack { prs, .. } = purpose else {
        return None;
    };
    let index = prs
        .iter()
        .position(|pr| Some(pr.head_ref.as_str()) == branch)?;
    Some(StackPosition {
        position: u32::try_from(index + 1).ok()?,
        total: u32::try_from(prs.len()).ok()?,
    })
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
            number,
            name,
            base_ref,
            prs,
            local_stack,
            ..
        } => {
            let _ = writeln!(
                out,
                "This directory is the worktree of GitHub stack #{number} \"{name}\": pull requests that build on each other, each on its own git branch. Bottom first:"
            );
            let mut below: Option<i64> = None;
            for (index, pr) in prs.iter().enumerate() {
                let base = match below {
                    Some(number) => format!("#{number}"),
                    None => format!("`{base_ref}`"),
                };
                let here = if branch == Some(pr.head_ref.as_str()) {
                    " ← the worktree is on this branch"
                } else {
                    ""
                };
                let _ = writeln!(
                    out,
                    "{}. #{} {}: branch `{}`, based on {base}{here}",
                    index + 1,
                    pr.number,
                    pr.title,
                    pr.head_ref,
                );
                below = Some(pr.number);
            }
            if let (Some(first), Some(last)) = (prs.first(), prs.last()) {
                if prs.len() > 1 {
                    let _ = writeln!(
                        out,
                        "Positions count from the bottom: PR 1 (\"the first PR\") is #{}, and the top is #{}.",
                        first.number, last.number
                    );
                }
            }
            let _ = writeln!(
                out,
                "This applies only to stack #{number}. If the user asks about a pull request or stack not listed here, say that this worktree isn't on it, and establish its number, head and base (`gh pr view <n> --json number,headRefName,baseRefName`) before you answer."
            );
            out.push_str("Before you change anything, work out which pull request the change belongs to, and commit it on that PR's branch. ");
            if *local_stack {
                out.push_str("`gh stack` tracks this stack here: `gh stack view` shows it, `gh stack up` and `gh stack down` switch PR, and the worktree must be clean first. A change to a lower PR leaves the ones above it needing a restack: run `gh stack rebase`, and say that you did. Don't run `gh stack sync`, `submit` or `push` unless the user asks: they push.");
            } else {
                out.push_str("Switch with `git switch <branch>`; the worktree must be clean first. A change to a lower PR leaves the ones above it needing a restack: rebase each of them onto the branch below it, bottom first, and say that you did.");
            }
            out.push_str("\nThis list is from Pingex's last read of the stack.");
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
            local_stack: false,
        }
    }

    #[test]
    fn a_stack_numbers_its_prs_from_the_bottom_with_their_bases() {
        let text = worktree_context(&stack(), Some("billing/charge"));
        assert!(text.starts_with("This directory is the worktree of GitHub stack #7 \"billing\""));
        assert!(text.contains(
            "Bottom first:\n1. #101 Add the ledger: branch `billing/ledger`, based on `main`\n2. #102 Charge from the ledger: branch `billing/charge`, based on #101 ← the worktree is on this branch\n"
        ));
        assert!(text.contains("PR 1 (\"the first PR\") is #101, and the top is #102."));
        assert!(text.contains("work out which pull request the change belongs to"));
    }

    #[test]
    fn a_stack_context_holds_only_for_its_own_stack() {
        let text = worktree_context(&stack(), Some("billing/charge"));
        assert!(text.contains("This applies only to stack #7."));
        assert!(text.contains("establish its number, head and base"));
    }

    #[test]
    fn without_gh_stack_a_restack_is_done_by_hand() {
        let text = worktree_context(&stack(), Some("billing/charge"));
        assert!(text.contains("rebase each of them onto the branch below it"));
        assert!(!text.contains("gh stack"));
    }

    #[test]
    fn with_gh_stack_the_agent_drives_the_stack_with_it() {
        let text = worktree_context(&stack().tracked_locally(true), Some("billing/charge"));
        assert!(text.contains("`gh stack` tracks this stack here"));
        assert!(text.contains("run `gh stack rebase`"));
        assert!(!text.contains("rebase each of them onto the branch below it"));
    }

    #[test]
    fn a_detached_stack_worktree_marks_no_pr() {
        let text = worktree_context(&stack(), None);
        assert!(!text.contains("the worktree is on this branch"));
        assert!(text.contains("2. #102"));
        assert_eq!(stack_position(&stack(), None), None);
    }

    #[test]
    fn the_position_counts_from_the_bottom() {
        assert_eq!(
            stack_position(&stack(), Some("billing/charge")),
            Some(StackPosition {
                position: 2,
                total: 2
            })
        );
        assert_eq!(stack_position(&stack(), Some("elsewhere")), None);
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
