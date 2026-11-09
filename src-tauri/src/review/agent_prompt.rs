//! The first message of an agent review: what the PR changes, what has been
//! said about it, what to look for, and the findings contract
//! (`review::findings`, ADR 0006). Pure: the desk gathers the parts.

use std::fmt::Write;

use super::agent_review::{AgentReviewRequest, FocusArea};
use super::checkout_types::PrCheckout;
use super::findings::Finding;
use super::requirements::CheckoutStep;
use super::types::PrDetail;

/// Full file contents at head go in until this many bytes; the rest are
/// named so the agent can read them itself.
pub(crate) const FILE_BUDGET: usize = 120_000;

/// What a re-run reviews instead of the whole PR.
pub(crate) struct SinceLastRun<'a> {
    /// The head the last run reviewed.
    pub(crate) sha: &'a str,
    /// `sha..head`, as a patch.
    pub(crate) patch: &'a str,
}

pub(crate) struct PromptInput<'a> {
    pub(crate) request: &'a AgentReviewRequest,
    /// The PRs under review, in the request's order.
    pub(crate) prs: &'a [PrDetail],
    /// The worktree the agent runs in, as it is now.
    pub(crate) checkout: Option<&'a PrCheckout>,
    /// `(path, contents)` of changed files at head, within the budget.
    pub(crate) files_at_head: &'a [(String, String)],
    /// Changed files left out for the budget, or unreadable.
    pub(crate) files_left_out: &'a [String],
    /// The repository's review rules.
    pub(crate) rules: &'a [String],
    /// Findings the reviewer dismissed on the previous run.
    pub(crate) dismissed: &'a [Finding],
    pub(crate) since: Option<SinceLastRun<'a>>,
}

pub(crate) fn agent_review_prompt(input: &PromptInput<'_>) -> String {
    let mut out = String::new();
    let stack = input.prs.len() > 1;
    let numbers: Vec<String> = input
        .prs
        .iter()
        .map(|pr| format!("#{}", pr.summary.number))
        .collect();
    if stack {
        let _ = writeln!(
            out,
            "Review this stack of pull requests as one unit, bottom first: {}.",
            numbers.join(", ")
        );
    } else if let Some(pr) = input.prs.first() {
        let _ = writeln!(
            out,
            "Review pull request #{} \"{}\" by {} ({} into {}).",
            pr.summary.number,
            pr.summary.title,
            pr.summary.author,
            pr.summary.head_ref,
            pr.summary.base_ref
        );
    }
    out.push('\n');
    out.push_str(&workspace_note(input));

    if !input.request.focus.is_empty() {
        out.push_str("\n## Focus on\n\n");
        for area in &input.request.focus {
            let _ = writeln!(out, "- {}", area.instruction());
        }
        if !input.request.focus.contains(&FocusArea::RunTests) {
            out.push_str("- Do not run tests.\n");
        }
    }

    if !input.rules.is_empty() {
        out.push_str(
            "\n## Review rules for this repository\n\nDo not report findings of these kinds:\n\n",
        );
        for rule in input.rules {
            let _ = writeln!(out, "- {rule}");
        }
    }

    if !input.dismissed.is_empty() {
        out.push_str(
            "\n## Dismissed on the last run\n\nThe reviewer dismissed these findings. Don't raise them again unless the code changed:\n\n",
        );
        for finding in input.dismissed {
            let reason = finding
                .dismiss_reason
                .map(|reason| reason.label())
                .unwrap_or("Dismissed");
            let note = finding
                .dismiss_note
                .as_deref()
                .filter(|note| !note.trim().is_empty())
                .map(|note| format!(": {note}"))
                .unwrap_or_default();
            let _ = writeln!(
                out,
                "- {}:{} {}. {reason}{note}",
                finding.path, finding.line, finding.title
            );
        }
    }

    for pr in input.prs {
        let _ = writeln!(
            out,
            "\n## #{} {}\n\nBy {}, {} into {}, head {}.",
            pr.summary.number,
            pr.summary.title,
            pr.summary.author,
            pr.summary.head_ref,
            pr.summary.base_ref,
            pr.head_sha
        );
        if !pr.body.trim().is_empty() {
            let _ = writeln!(out, "\n{}", pr.body.trim());
        }
        // A plain "reviewed" with no body says nothing its inline comments don't.
        let reviews: Vec<_> = pr
            .reviews
            .iter()
            .filter(|review| !review.body.trim().is_empty() || review.state != "COMMENTED")
            .collect();
        if !pr.comments.is_empty() || !reviews.is_empty() {
            out.push_str("\n### Conversation and comments\n\n");
            for review in reviews {
                let verdict = match review.state.as_str() {
                    "APPROVED" => "approved",
                    "CHANGES_REQUESTED" => "requested changes",
                    "DISMISSED" => "reviewed (dismissed)",
                    _ => "reviewed",
                };
                let body = review.body.trim();
                if body.is_empty() {
                    let _ = writeln!(out, "- [review] {} {verdict}", review.author);
                } else {
                    let _ = writeln!(
                        out,
                        "- [review] {} {verdict}: {}",
                        review.author,
                        body.replace('\n', "\n  ")
                    );
                }
            }
            for comment in &pr.comments {
                let place = match (&comment.path, comment.line) {
                    (Some(path), Some(line)) => format!(" on {path}:{line}"),
                    (Some(path), None) => format!(" on {path}"),
                    _ => String::new(),
                };
                let resolved = if comment.is_resolved {
                    " (resolved)"
                } else {
                    ""
                };
                let _ = writeln!(
                    out,
                    "- [comment {}] {}{place}{resolved}: {}",
                    comment.id,
                    comment.author,
                    comment.body.trim().replace('\n', "\n  ")
                );
            }
        }
        if input.since.is_none() {
            out.push_str("\n### Patch\n\n```diff\n");
            for file in &pr.files {
                if file.patch_truncated || file.patch.is_empty() {
                    let _ = writeln!(out, "# {} ({}, no text diff)", file.path, file.status);
                } else {
                    let _ = writeln!(out, "# {}", file.path);
                    out.push_str(file.patch.trim_end());
                    out.push('\n');
                }
            }
            out.push_str("```\n");
        }
    }

    if let Some(since) = &input.since {
        let _ = writeln!(
            out,
            "\n## New commits since the last review\n\nThe last run reviewed {}. Review only what changed since:\n\n```diff\n{}\n```",
            since.sha,
            since.patch.trim_end()
        );
    }

    if !input.files_at_head.is_empty() || !input.files_left_out.is_empty() {
        out.push_str("\n## Changed files at the head\n");
        for (path, text) in input.files_at_head {
            let _ = writeln!(out, "\n### {path}\n\n```\n{}\n```", text.trim_end());
        }
        if !input.files_left_out.is_empty() {
            let _ = writeln!(
                out,
                "\nNot included here, read them if you need to: {}.",
                input.files_left_out.join(", ")
            );
        }
    }

    out.push_str(&contract(stack));
    out
}

/// Where the agent is and what it may do there.
fn workspace_note(input: &PromptInput<'_>) -> String {
    if input.request.step == CheckoutStep::NoCheckout {
        return "This is a diff review: read the patch, the conversation and the files below. Do not run anything and do not edit files.\n".to_string();
    }
    let mut note = String::from("The working directory is a git worktree of this PR");
    if let Some(branch) = input
        .checkout
        .and_then(|checkout| checkout.git_branch.as_ref())
    {
        let _ = write!(note, ", on the local branch `{branch}`");
    }
    match input.checkout {
        Some(checkout) if checkout.ahead > 0 || checkout.behind > 0 => {
            let _ = write!(
                note,
                ". It has {} local commit(s) the PR does not, and lacks {} commit(s) from the PR's head",
                checkout.ahead, checkout.behind
            );
        }
        Some(_) => note.push_str(", at the PR's head"),
        None => {}
    }
    note.push_str(".\n\nYou can build, run, benchmark and write scratch files here. If trying a change helps you check something, make it and say so in your reasoning. Commits stay local unless you are asked to push.\n");
    let mine = input
        .request
        .filed_under()
        .is_some_and(|target| target.mine);
    if !mine {
        note.push_str("Do not push: this is someone else's pull request.\n");
    }
    note.push_str("Report findings against the PR's head, not your local edits: use the head's line numbers even if you changed files here.\n");
    note
}

/// The findings contract, as the agent reads it.
fn contract(stack: bool) -> String {
    let pr_field = if stack { ", \"pr\": 415" } else { "" };
    let pr_line = if stack {
        "- pr: the number of the PR in the stack whose change the finding is about.\n"
    } else {
        ""
    };
    format!(
        "\n## How to report\n\n\
Explain your reasoning as you go. Then end your final message with one fenced JSON block listing your findings, with nothing after it:\n\n\
```json\n\
[{{\"path\": \"src/example.rs\", \"line\": 12, \"endLine\": 14, \"side\": \"RIGHT\", \"severity\": \"medium\", \"title\": \"Short summary\", \"body\": \"What is wrong and why it matters.\", \"evidence\": \"Where you saw it\", \"sameAsCommentId\": null{pr_field}}}]\n\
```\n\n\
- path: the file, relative to the repository root.\n\
- line, endLine: the first and last line of the range (endLine is optional). Line numbers are in the file at the head for side RIGHT, or at the base for side LEFT (removed lines).\n\
- severity: high, medium or low.\n\
- evidence: optional; what you saw that shows the problem.\n\
- sameAsCommentId: the id of an existing comment above that already makes this point, or null.\n\
{pr_line}\
- Write [] when you found nothing worth raising.\n"
    )
}
