//! The pending review's entries, and how they become one GitHub review.
//!
//! A pending review is typed: line or range comments, suggestions, replies
//! written into the review, and accepted Findings, plus a summary and a
//! verdict. The frontend edits it; the desk stores it per PR and posts it.

use serde::{Deserialize, Serialize};

use super::types::{PendingComment, ReviewReply, ReviewSubmission, ReviewVerdict};

/// Where an entry sits in the diff: one line, or the range
/// `start_line..=line` on one side.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DiffAnchor {
    pub(crate) path: String,
    /// The last line of the range, or the only line.
    pub(crate) line: i64,
    /// `RIGHT` (head) or `LEFT` (base).
    pub(crate) side: String,
    /// The first line of a range; `None` for one line.
    #[serde(default)]
    pub(crate) start_line: Option<i64>,
}

/// One entry of a pending review. `id` is the frontend's, unique within the
/// review, so an entry can be edited or removed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(crate) enum PendingEntry {
    /// A new comment on a line or a range.
    Comment {
        id: String,
        anchor: DiffAnchor,
        body: String,
    },
    /// A proposed change to the anchored lines, posted as a GitHub
    /// suggestion block under `body`.
    Suggestion {
        id: String,
        anchor: DiffAnchor,
        body: String,
        replacement: String,
    },
    /// A reply to an existing review thread, posted with the review rather
    /// than at once. `comment_id`, `path` and `line` are for display.
    Reply {
        id: String,
        thread_id: String,
        comment_id: i64,
        path: String,
        line: Option<i64>,
        body: String,
    },
    /// An agent Finding the reviewer added to the review. `body` is the
    /// reviewer's (possibly edited) text; it posts as an ordinary comment.
    AcceptedFinding {
        id: String,
        finding_id: String,
        anchor: DiffAnchor,
        body: String,
    },
}

/// The reviewer's unsubmitted review of one PR.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PendingReview {
    /// The head the entries were written against; the review posts pinned
    /// to it.
    pub(crate) head_sha: String,
    pub(crate) entries: Vec<PendingEntry>,
    pub(crate) summary: String,
    pub(crate) verdict: ReviewVerdict,
    /// Unix seconds of the last save; the desk sets it.
    #[serde(default)]
    pub(crate) updated_at: i64,
}

impl PendingReview {
    /// Nothing worth keeping: no entries and no summary.
    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.summary.trim().is_empty()
    }

    /// The one GitHub review this pending review posts.
    pub(crate) fn submission(&self) -> Result<ReviewSubmission, String> {
        if self.entries.is_empty()
            && self.summary.trim().is_empty()
            && self.verdict != ReviewVerdict::Approve
        {
            return Err("Write a comment or a summary first".to_string());
        }
        let mut comments = Vec::new();
        let mut replies = Vec::new();
        for entry in &self.entries {
            match entry {
                PendingEntry::Comment { anchor, body, .. }
                | PendingEntry::AcceptedFinding { anchor, body, .. } => {
                    comments.push(inline_comment(anchor, body.clone()));
                }
                PendingEntry::Suggestion {
                    anchor,
                    body,
                    replacement,
                    ..
                } => comments.push(inline_comment(anchor, suggestion_body(body, replacement))),
                PendingEntry::Reply {
                    thread_id, body, ..
                } => replies.push(ReviewReply {
                    thread_id: thread_id.clone(),
                    body: body.clone(),
                }),
            }
        }
        Ok(ReviewSubmission {
            verdict: self.verdict,
            body: self.summary.trim().to_string(),
            commit_id: Some(self.head_sha.clone()).filter(|sha| !sha.is_empty()),
            comments,
            replies,
        })
    }

    /// A stored payload: today's typed review, or the older untyped draft
    /// (`{pending: [PendingComment], reviewStarted}`), whose comments become
    /// line comments. Anything else reads as no pending review.
    pub(crate) fn from_stored(head_sha: &str, payload: &str) -> Option<Self> {
        if let Ok(review) = serde_json::from_str::<Self>(payload) {
            return Some(review);
        }
        #[derive(Deserialize)]
        struct Legacy {
            #[serde(default)]
            pending: Vec<PendingComment>,
        }
        let legacy = serde_json::from_str::<Legacy>(payload).ok()?;
        let entries = legacy
            .pending
            .into_iter()
            .enumerate()
            .map(|(index, comment)| PendingEntry::Comment {
                id: format!("legacy-{index}"),
                anchor: DiffAnchor {
                    path: comment.path,
                    line: comment.line,
                    side: comment.side.unwrap_or_else(|| "RIGHT".to_string()),
                    start_line: comment.start_line,
                },
                body: comment.body,
            })
            .collect();
        Some(Self {
            head_sha: head_sha.to_string(),
            entries,
            ..Self::default()
        })
    }
}

fn inline_comment(anchor: &DiffAnchor, body: String) -> PendingComment {
    // GitHub wants a range's first line strictly before its last.
    let start_line = anchor.start_line.filter(|start| *start < anchor.line);
    PendingComment {
        path: anchor.path.clone(),
        line: anchor.line,
        side: Some(anchor.side.clone()),
        start_line,
        start_side: start_line.map(|_| anchor.side.clone()),
        body,
    }
}

/// A GitHub suggestion: the reviewer's words, then the replacement lines in
/// a `suggestion` block, which GitHub offers to commit.
fn suggestion_body(body: &str, replacement: &str) -> String {
    let block = format!("```suggestion\n{}\n```", replacement.trim_end_matches('\n'));
    let body = body.trim();
    if body.is_empty() {
        block
    } else {
        format!("{body}\n\n{block}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_comment_review_is_refused_but_a_bare_approval_is_not() {
        assert!(PendingReview::default().submission().is_err());
        let approval = PendingReview {
            verdict: ReviewVerdict::Approve,
            ..PendingReview::default()
        };
        assert_eq!(
            approval.submission().unwrap().verdict,
            ReviewVerdict::Approve
        );
    }

    #[test]
    fn an_older_untyped_draft_reads_as_line_comments() {
        let review = PendingReview::from_stored(
            "abc",
            r#"{"pending":[{"path":"a.rs","line":4,"side":"LEFT","body":"hm"}],"reviewStarted":true}"#,
        )
        .unwrap();
        assert_eq!(review.head_sha, "abc");
        assert_eq!(
            review.entries,
            [PendingEntry::Comment {
                id: "legacy-0".into(),
                anchor: DiffAnchor {
                    path: "a.rs".into(),
                    line: 4,
                    side: "LEFT".into(),
                    start_line: None,
                },
                body: "hm".into(),
            }]
        );
        assert_eq!(PendingReview::from_stored("abc", "not json"), None);
    }

    #[test]
    fn a_one_line_range_posts_as_a_single_line_comment() {
        let review = PendingReview {
            entries: vec![PendingEntry::Comment {
                id: "c".into(),
                anchor: DiffAnchor {
                    path: "a.rs".into(),
                    line: 4,
                    side: "RIGHT".into(),
                    start_line: Some(4),
                },
                body: "x".into(),
            }],
            ..PendingReview::default()
        };
        let comment = &review.submission().unwrap().comments[0];
        assert_eq!(
            (comment.start_line, comment.start_side.as_deref()),
            (None, None)
        );
    }
}
