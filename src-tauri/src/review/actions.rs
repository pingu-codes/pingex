//! Review mutations: submitting a review, replying inline, resolving a thread.

use serde_json::json;
use std::path::Path;

use crate::util::host::Host;

use super::gh::{redact_gh_error, run_gh, run_gh_with_input, WRITE_TIMEOUT};
use super::types::PendingComment;

const RESOLVE_THREAD_MUTATION: &str =
    "mutation($threadId:ID!){resolveReviewThread(input:{threadId:$threadId}){thread{id isResolved}}}";

/// Map the UI's event name onto GitHub's. Anything unrecognised is a plain
/// comment, which is the only non-destructive default.
fn review_event(event: &str) -> &'static str {
    match event {
        "approve" | "APPROVE" => "APPROVE",
        "request-changes" | "REQUEST_CHANGES" => "REQUEST_CHANGES",
        _ => "COMMENT",
    }
}

/// The JSON body for `POST pulls/<n>/reviews`. `commit_id` pins the review
/// and its inline comments to the head the reviewer actually read; without
/// it GitHub anchors them to whatever the head is at submit time, so a push
/// in between moves comments onto lines the reviewer never saw.
fn review_payload(
    event: &str,
    body: &str,
    commit_id: Option<&str>,
    comments: &[PendingComment],
) -> serde_json::Value {
    let mut payload = json!({
        "event": review_event(event),
        "body": body,
        "comments": comments
            .iter()
            .map(|comment| json!({
                "path": comment.path,
                "line": comment.line,
                "side": comment.side.clone().unwrap_or_else(|| "RIGHT".to_string()),
                "body": comment.body,
            }))
            .collect::<Vec<_>>(),
    });
    if let Some(commit_id) = commit_id {
        payload["commit_id"] = json!(commit_id);
    }
    payload
}

pub(crate) fn submit_review(
    host: &Host,
    dir: &Path,
    number: i64,
    event: &str,
    body: &str,
    commit_id: Option<&str>,
    comments: &[PendingComment],
) -> Result<(), String> {
    if let Some(commit_id) = commit_id {
        crate::git::objects::require_sha(commit_id)?;
    }
    // Always the REST reviews endpoint, even for a bare approval: it is the
    // one path that takes `commit_id`. The JSON goes through stdin
    // (`--input -`) since nested `comments` cannot be `-f` fields.
    let payload = review_payload(event, body, commit_id, comments);
    let endpoint = format!("repos/{{owner}}/{{repo}}/pulls/{number}/reviews");
    let output = run_gh_with_input(host, dir, &endpoint, &payload.to_string())?;
    if !output.ok {
        return Err(redact_gh_error("Could not submit the review", &output));
    }
    Ok(())
}

pub(crate) fn reply_to_comment(
    host: &Host,
    dir: &Path,
    number: i64,
    comment_id: i64,
    body: &str,
) -> Result<(), String> {
    let endpoint = format!("repos/{{owner}}/{{repo}}/pulls/{number}/comments/{comment_id}/replies");
    let output = run_gh(
        host,
        dir,
        &[
            "api",
            "--method",
            "POST",
            &endpoint,
            "-f",
            &format!("body={body}"),
        ],
        WRITE_TIMEOUT,
    )?;
    if !output.ok {
        return Err(redact_gh_error("Could not post the reply", &output));
    }
    Ok(())
}

pub(crate) fn resolve_thread(host: &Host, dir: &Path, thread_id: &str) -> Result<(), String> {
    let output = run_gh(
        host,
        dir,
        &[
            "api",
            "graphql",
            "-f",
            &format!("query={RESOLVE_THREAD_MUTATION}"),
            "-f",
            &format!("threadId={thread_id}"),
        ],
        WRITE_TIMEOUT,
    )?;
    if !output.ok {
        return Err(redact_gh_error("Could not resolve the thread", &output));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_review_events_and_defaults_to_comment() {
        assert_eq!(review_event("approve"), "APPROVE");
        assert_eq!(review_event("APPROVE"), "APPROVE");
        assert_eq!(review_event("request-changes"), "REQUEST_CHANGES");
        assert_eq!(review_event("REQUEST_CHANGES"), "REQUEST_CHANGES");
        assert_eq!(review_event("comment"), "COMMENT");
        // An unknown event must never escalate to an approval.
        assert_eq!(review_event("merge-it-please"), "COMMENT");
    }

    #[test]
    fn a_review_is_pinned_to_the_head_it_was_written_against() {
        let sha = "a".repeat(40);
        let comments = [PendingComment {
            path: "src/lib.rs".into(),
            line: 3,
            side: None,
            body: "nit".into(),
        }];
        let payload = review_payload("approve", "", Some(&sha), &comments);
        assert_eq!(payload["commit_id"], sha);
        assert_eq!(payload["event"], "APPROVE");
        assert_eq!(payload["comments"][0]["side"], "RIGHT");
        assert!(review_payload("comment", "x", None, &[])
            .get("commit_id")
            .is_none());
    }
}
