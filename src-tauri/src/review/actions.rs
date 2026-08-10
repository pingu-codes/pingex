//! Review mutations: submitting a review, replying inline, resolving a thread.

use serde_json::json;
use std::path::Path;

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

pub(crate) fn submit_review(
    dir: &Path,
    number: i64,
    event: &str,
    body: &str,
    comments: &[PendingComment],
) -> Result<(), String> {
    let event = review_event(event);
    let number_str = number.to_string();

    // A review carrying inline comments must go through the REST reviews
    // endpoint; a plain approve/comment/request-changes can use `gh pr review`.
    if comments.is_empty() {
        let flag = match event {
            "APPROVE" => "--approve",
            "REQUEST_CHANGES" => "--request-changes",
            _ => "--comment",
        };
        let mut args = vec!["pr", "review", &number_str, flag];
        // `--comment` and `--request-changes` require a body.
        if !body.is_empty() || event != "APPROVE" {
            args.push("--body");
            args.push(body);
        }
        let output = run_gh(dir, &args, WRITE_TIMEOUT)?;
        if !output.ok {
            return Err(redact_gh_error("Could not submit the review", &output));
        }
        return Ok(());
    }

    // A nested `comments` array can't be expressed with `-f` fields, so POST the
    // JSON body through the endpoint's stdin (`--input -`).
    let payload = json!({
        "event": event,
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
    let endpoint = format!("repos/{{owner}}/{{repo}}/pulls/{number}/reviews");
    let output = run_gh_with_input(dir, &endpoint, &payload.to_string())?;
    if !output.ok {
        return Err(redact_gh_error("Could not submit the review", &output));
    }
    Ok(())
}

pub(crate) fn reply_to_comment(
    dir: &Path,
    number: i64,
    comment_id: i64,
    body: &str,
) -> Result<(), String> {
    let endpoint = format!("repos/{{owner}}/{{repo}}/pulls/{number}/comments/{comment_id}/replies");
    let output = run_gh(
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

pub(crate) fn resolve_thread(dir: &Path, thread_id: &str) -> Result<(), String> {
    let output = run_gh(
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
}
