//! The findings contract: how an agent review hands its Findings back.
//!
//! Harness-neutral (ADR 0006). The agent ends its final message with one
//! fenced JSON block, a list of
//! `{path, line, side, endLine?, severity: high|medium|low, title, body,
//! evidence?, sameAsCommentId?, pr?}`. `parse_findings_block` reads that
//! block out of the message text; `findings_from_report` turns it into the
//! stored `Finding`s of one agent review at one head. A missing or malformed
//! block is an error the review shows as "No findings parsed", never a
//! partial list.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) enum FindingSeverity {
    High,
    Medium,
    Low,
}

/// Where a Finding stands. Draft until the reviewer acts: added to the
/// pending review, dismissed with a reason, or merged into the human
/// comment it repeats.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) enum FindingState {
    #[default]
    Draft,
    Added,
    Dismissed,
    Merged,
}

impl FindingState {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Added => "added",
            Self::Dismissed => "dismissed",
            Self::Merged => "merged",
        }
    }

    pub(crate) fn parse(value: &str) -> Self {
        match value {
            "added" => Self::Added,
            "dismissed" => Self::Dismissed,
            "merged" => Self::Merged,
            _ => Self::Draft,
        }
    }
}

/// Why the reviewer dismissed a Finding. The next run of the agent review
/// reads these.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) enum DismissReason {
    NotAProblem,
    AlreadyRaised,
    OutOfScope,
    /// "Wrong, tell the agent why…": the note says why.
    Wrong,
}

impl DismissReason {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::NotAProblem => "notAProblem",
            Self::AlreadyRaised => "alreadyRaised",
            Self::OutOfScope => "outOfScope",
            Self::Wrong => "wrong",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "notAProblem" => Some(Self::NotAProblem),
            "alreadyRaised" => Some(Self::AlreadyRaised),
            "outOfScope" => Some(Self::OutOfScope),
            "wrong" => Some(Self::Wrong),
            _ => None,
        }
    }

    /// The reason as the reviewer read it in the menu.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::NotAProblem => "Not a problem",
            Self::AlreadyRaised => "Already raised in a comment",
            Self::OutOfScope => "Out of scope for this PR",
            Self::Wrong => "Wrong",
        }
    }
}

/// One entry of the findings block, as the agent wrote it.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReportedFinding {
    pub(crate) path: String,
    /// The first line; `end_line` ends a range.
    pub(crate) line: i64,
    #[serde(default)]
    pub(crate) side: Option<String>,
    #[serde(default)]
    pub(crate) end_line: Option<i64>,
    pub(crate) severity: FindingSeverity,
    #[serde(default)]
    pub(crate) title: String,
    #[serde(default)]
    pub(crate) body: String,
    #[serde(default)]
    pub(crate) evidence: Option<String>,
    /// The existing human comment this repeats.
    #[serde(default)]
    pub(crate) same_as_comment_id: Option<i64>,
    /// The PR the finding is about, when the review covered a stack.
    #[serde(default)]
    pub(crate) pr: Option<i64>,
}

/// One Finding of one agent review, at the head it reviewed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Finding {
    pub(crate) id: String,
    pub(crate) agent_review_id: String,
    pub(crate) pr_number: i64,
    pub(crate) head_sha: String,
    pub(crate) path: String,
    pub(crate) line: i64,
    pub(crate) end_line: Option<i64>,
    /// `RIGHT` (head) or `LEFT` (base).
    pub(crate) side: String,
    pub(crate) severity: FindingSeverity,
    pub(crate) title: String,
    pub(crate) body: String,
    pub(crate) evidence: Option<String>,
    /// Set when the Finding is merged into this human comment.
    pub(crate) same_as_comment_id: Option<i64>,
    pub(crate) state: FindingState,
    pub(crate) dismiss_reason: Option<DismissReason>,
    pub(crate) dismiss_note: Option<String>,
}

/// Why no Findings could be read from a final message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FindingsError {
    /// The message has no fenced JSON block.
    Missing,
    /// It has one, but it is not a valid findings list.
    Malformed(String),
}

impl std::fmt::Display for FindingsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing => write!(f, "The final message has no findings block"),
            Self::Malformed(reason) => write!(f, "The findings block is malformed: {reason}"),
        }
    }
}

/// A fenced block: its info string and its contents.
struct Fence<'a> {
    info: &'a str,
    body: String,
}

/// Every fenced code block in `text`, in order. A fence left open runs to
/// the end of the text and is reported as unclosed.
fn fences(text: &str) -> Result<Vec<Fence<'_>>, FindingsError> {
    let mut found = Vec::new();
    let mut open: Option<(&str, &str, Vec<&str>)> = None;
    for line in text.lines() {
        let trimmed = line.trim_start();
        let marker_len = trimmed.chars().take_while(|c| *c == '`').count();
        match open.as_mut() {
            None if marker_len >= 3 => {
                let (marker, info) = trimmed.split_at(marker_len);
                open = Some((marker, info.trim(), Vec::new()));
            }
            None => {}
            Some((marker, info, lines)) => {
                if marker_len >= marker.len() && trimmed[marker_len..].trim().is_empty() {
                    found.push(Fence {
                        info,
                        body: lines.join("\n"),
                    });
                    open = None;
                } else {
                    lines.push(line);
                }
            }
        }
    }
    match open {
        Some((_, info, _)) if is_json_info(info) => Err(FindingsError::Malformed(
            "the block is never closed".to_string(),
        )),
        _ => Ok(found),
    }
}

fn is_json_info(info: &str) -> bool {
    info.split_whitespace()
        .next()
        .is_some_and(|word| word.eq_ignore_ascii_case("json"))
}

/// The Findings an agent listed in the last fenced JSON block of its final
/// message. An empty list is a review that found nothing.
pub(crate) fn parse_findings_block(message: &str) -> Result<Vec<ReportedFinding>, FindingsError> {
    let block = fences(message)?
        .into_iter()
        .rev()
        .find(|fence| {
            is_json_info(fence.info)
                || (fence.info.is_empty() && fence.body.trim_start().starts_with('['))
        })
        .ok_or(FindingsError::Missing)?;
    let reported: Vec<ReportedFinding> = serde_json::from_str(&block.body)
        .map_err(|error| FindingsError::Malformed(error.to_string()))?;
    for (index, finding) in reported.iter().enumerate() {
        validate(finding).map_err(|reason| {
            FindingsError::Malformed(format!("finding {}: {reason}", index + 1))
        })?;
    }
    Ok(reported)
}

fn validate(finding: &ReportedFinding) -> Result<(), String> {
    if finding.path.trim().is_empty() {
        return Err("no path".to_string());
    }
    if finding.line < 1 {
        return Err("line must be 1 or more".to_string());
    }
    if finding.end_line.is_some_and(|end| end < finding.line) {
        return Err("endLine is before line".to_string());
    }
    if let Some(side) = &finding.side {
        if !side.eq_ignore_ascii_case("right") && !side.eq_ignore_ascii_case("left") {
            return Err(format!("side \"{side}\" is not RIGHT or LEFT"));
        }
    }
    if finding.title.trim().is_empty() && finding.body.trim().is_empty() {
        return Err("no title or body".to_string());
    }
    Ok(())
}

/// The stored Findings of agent review `review_id` at `head_sha`. A Finding
/// that names one of `comment_ids` (the PR's existing comments) is merged
/// into it; one naming a comment the PR doesn't have stays a draft, without
/// the claim. `default_pr` is the PR a Finding without `pr` belongs to;
/// `prs` are the PRs the review covered.
pub(crate) fn findings_from_report(
    review_id: &str,
    head_sha: &str,
    default_pr: i64,
    prs: &[i64],
    reported: Vec<ReportedFinding>,
    comment_ids: &HashSet<i64>,
) -> Vec<Finding> {
    reported
        .into_iter()
        .enumerate()
        .map(|(index, finding)| {
            let same_as = finding
                .same_as_comment_id
                .filter(|id| comment_ids.contains(id));
            let side = match finding.side.as_deref() {
                Some(side) if side.eq_ignore_ascii_case("left") => "LEFT",
                _ => "RIGHT",
            };
            Finding {
                id: format!("{review_id}-{}", index + 1),
                agent_review_id: review_id.to_string(),
                pr_number: finding
                    .pr
                    .filter(|number| prs.contains(number))
                    .unwrap_or(default_pr),
                head_sha: head_sha.to_string(),
                path: finding.path,
                line: finding.line,
                end_line: finding.end_line.filter(|end| *end > finding.line),
                side: side.to_string(),
                severity: finding.severity,
                title: finding.title,
                body: finding.body,
                evidence: finding.evidence.filter(|text| !text.trim().is_empty()),
                same_as_comment_id: same_as,
                state: if same_as.is_some() {
                    FindingState::Merged
                } else {
                    FindingState::Draft
                },
                dismiss_reason: None,
                dismiss_note: None,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPORT: &str = r#"I read the patch and the files at head.

The weekly reset is computed in UTC.

```json
[
  {"path": "src/usage/window.rs", "line": 90, "endLine": 92, "side": "RIGHT",
   "severity": "medium", "title": "Reset in UTC",
   "body": "The meter shows local time.", "evidence": "UsageMeter.svelte:31"},
  {"path": "src/usage/mod.rs", "line": 12, "severity": "low",
   "title": "Unused import", "body": "", "sameAsCommentId": 501}
]
```
"#;

    #[test]
    fn the_last_json_block_of_the_final_message_is_the_findings_list() {
        let reported = parse_findings_block(REPORT).unwrap();
        assert_eq!(reported.len(), 2);
        assert_eq!(reported[0].path, "src/usage/window.rs");
        assert_eq!(reported[0].end_line, Some(92));
        assert_eq!(reported[0].severity, FindingSeverity::Medium);
        assert_eq!(reported[1].same_as_comment_id, Some(501));
    }

    #[test]
    fn an_earlier_code_block_is_not_mistaken_for_the_findings() {
        let message =
            "Look at this:\n\n```rust\nfn main() {}\n```\n\nFindings:\n\n```json\n[]\n```\n";
        assert_eq!(parse_findings_block(message).unwrap(), vec![]);
    }

    #[test]
    fn a_message_without_a_block_has_missing_findings() {
        assert_eq!(
            parse_findings_block("Looks good to me, no issues."),
            Err(FindingsError::Missing)
        );
        assert_eq!(
            parse_findings_block("```rust\nfn main() {}\n```"),
            Err(FindingsError::Missing)
        );
    }

    #[test]
    fn a_malformed_block_is_an_error_not_a_partial_list() {
        for message in [
            "```json\n[{\"path\": \"a.rs\", \"line\": 3, \"severity\": \"high\", \"title\": \"x\"\n```",
            "```json\n{\"path\": \"a.rs\"}\n```",
            "```json\n[{\"path\": \"a.rs\", \"line\": 3, \"severity\": \"urgent\", \"title\": \"x\"}]\n```",
            "```json\n[{\"path\": \"a.rs\", \"line\": 0, \"severity\": \"low\", \"title\": \"x\"}]\n```",
            "```json\n[{\"path\": \"a.rs\", \"line\": 3, \"side\": \"UP\", \"severity\": \"low\", \"title\": \"x\"}]\n```",
            "```json\n[{\"path\": \"a.rs\", \"line\": 3, \"severity\": \"low\"}]\n```",
            "```json\n[]\n",
        ] {
            assert!(
                matches!(parse_findings_block(message), Err(FindingsError::Malformed(_))),
                "{message}"
            );
        }
    }

    #[test]
    fn a_finding_that_repeats_a_human_comment_is_merged_into_it() {
        let reported = parse_findings_block(REPORT).unwrap();
        let comments = HashSet::from([501, 502]);
        let findings = findings_from_report("ar1", "abc", 409, &[409], reported, &comments);
        assert_eq!(findings[0].state, FindingState::Draft);
        assert_eq!(findings[0].same_as_comment_id, None);
        assert_eq!(findings[1].state, FindingState::Merged);
        assert_eq!(findings[1].same_as_comment_id, Some(501));
        assert_eq!(findings[0].id, "ar1-1");
        assert_eq!(findings[0].head_sha, "abc");
        assert_eq!(findings[0].pr_number, 409);
    }

    #[test]
    fn a_claim_to_repeat_a_comment_the_pr_lacks_stays_a_draft() {
        let reported = parse_findings_block(REPORT).unwrap();
        let findings = findings_from_report("ar1", "abc", 409, &[409], reported, &HashSet::new());
        assert_eq!(findings[1].state, FindingState::Draft);
        assert_eq!(findings[1].same_as_comment_id, None);
    }

    #[test]
    fn a_stack_finding_files_under_the_pr_it_names() {
        let message = "```json\n[{\"path\": \"a.rs\", \"line\": 3, \"severity\": \"low\", \"title\": \"x\", \"pr\": 412},\n {\"path\": \"b.rs\", \"line\": 3, \"severity\": \"low\", \"title\": \"y\", \"pr\": 999}]\n```";
        let reported = parse_findings_block(message).unwrap();
        let findings = findings_from_report(
            "ar1",
            "abc",
            418,
            &[412, 415, 418],
            reported,
            &HashSet::new(),
        );
        assert_eq!(findings[0].pr_number, 412);
        assert_eq!(findings[1].pr_number, 418);
    }
}
