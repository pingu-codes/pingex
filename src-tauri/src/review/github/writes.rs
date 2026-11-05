//! Writes through `gh`: reviews and comments, and changes to the PR itself.
//!
//! Each write names its repository (`slug`, `owner/name`) instead of letting
//! `gh` infer it from the folder, so a PRs-only repository with no clone
//! works the same way. The argument lists are built by pure functions and
//! tested; running them needs a real GitHub, which tests never touch.

use serde_json::json;
use std::path::Path;

use crate::review::types::{
    CommentRef, MergeMethod, MergeRequest, NewPr, PendingComment, ReviewSubmission, ReviewVerdict,
};
use crate::util::host::Host;

use super::run::{redact_gh_error, run_gh, run_gh_with_input, WRITE_TIMEOUT};

const RESOLVE_THREAD_MUTATION: &str =
    "mutation($threadId:ID!){resolveReviewThread(input:{threadId:$threadId}){thread{id isResolved}}}";

fn review_event(verdict: ReviewVerdict) -> &'static str {
    match verdict {
        ReviewVerdict::Approve => "APPROVE",
        ReviewVerdict::RequestChanges => "REQUEST_CHANGES",
        ReviewVerdict::Comment => "COMMENT",
    }
}

const REPLY_IN_REVIEW_MUTATION: &str = "mutation($review:ID!,$thread:ID!,$body:String!){addPullRequestReviewThreadReply(input:{pullRequestReviewId:$review,pullRequestReviewThreadId:$thread,body:$body}){comment{id}}}";

/// The JSON body for `POST pulls/<n>/reviews`. `commit_id` pins the review
/// and its inline comments to the head the reviewer actually read; without
/// it GitHub anchors them to whatever the head is at submit time, so a push
/// in between moves comments onto lines the reviewer never saw.
///
/// With `pending`, the body has no `event`, so GitHub keeps the review
/// pending: replies are added to it before it is submitted.
fn review_payload(review: &ReviewSubmission, pending: bool) -> serde_json::Value {
    let mut payload = json!({
        "body": review.body,
        "comments": review.comments
            .iter()
            .map(|comment: &PendingComment| {
                let mut entry = json!({
                    "path": comment.path,
                    "line": comment.line,
                    "side": comment.side.clone().unwrap_or_else(|| "RIGHT".to_string()),
                    "body": comment.body,
                });
                if let Some(start_line) = comment.start_line {
                    entry["start_line"] = json!(start_line);
                    entry["start_side"] = json!(comment
                        .start_side
                        .clone()
                        .or_else(|| comment.side.clone())
                        .unwrap_or_else(|| "RIGHT".to_string()));
                }
                entry
            })
            .collect::<Vec<_>>(),
    });
    if !pending {
        payload["event"] = json!(review_event(review.verdict));
    }
    if let Some(commit_id) = &review.commit_id {
        payload["commit_id"] = json!(commit_id);
    }
    payload
}

/// The body that submits a pending review.
fn submit_event_payload(review: &ReviewSubmission) -> serde_json::Value {
    json!({ "event": review_event(review.verdict), "body": review.body })
}

/// `gh api graphql` arguments adding one reply to pending review `review_node`.
fn reply_in_review_args(review_node: &str, thread_id: &str, body: &str) -> Vec<String> {
    strings(&[
        "api",
        "graphql",
        "-f",
        &format!("query={REPLY_IN_REVIEW_MUTATION}"),
        "-f",
        &format!("review={review_node}"),
        "-f",
        &format!("thread={thread_id}"),
        "-f",
        &format!("body={body}"),
    ])
}

/// Run a write and turn a refusal into a redacted message.
fn write(host: &Host, dir: &Path, args: &[String], failed: &str) -> Result<String, String> {
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let output = run_gh(host, dir, &args, WRITE_TIMEOUT)?;
    if !output.ok {
        return Err(redact_gh_error(failed, &output));
    }
    Ok(output.stdout)
}

/// A git branch name handed to `gh`. One starting with `-` would read as an
/// option, so it is refused.
fn require_branch(name: &str) -> Result<&str, String> {
    let name = name.trim();
    if name.is_empty() || name.starts_with('-') {
        return Err("That is not a git branch name".to_string());
    }
    Ok(name)
}

fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|arg| (*arg).to_string()).collect()
}

pub(crate) fn submit_review(
    host: &Host,
    dir: &Path,
    slug: &str,
    number: i64,
    review: &ReviewSubmission,
) -> Result<(), String> {
    if let Some(commit_id) = &review.commit_id {
        crate::git::objects::require_sha(commit_id)?;
    }
    // Always the REST reviews endpoint, even for a bare approval: it is the
    // one path that takes `commit_id`. The JSON goes through stdin
    // (`--input -`) since nested `comments` cannot be `-f` fields.
    let endpoint = format!("repos/{slug}/pulls/{number}/reviews");
    if review.replies.is_empty() {
        let body = review_payload(review, false).to_string();
        let output = run_gh_with_input(host, dir, &endpoint, &body)?;
        if !output.ok {
            return Err(redact_gh_error("Could not submit the review", &output));
        }
        return Ok(());
    }
    // Replies to existing threads cannot ride in the REST body. Open the
    // review pending, attach each reply to it, then submit it; a failure
    // part way deletes the pending review so nothing half-posts.
    let body = review_payload(review, true).to_string();
    let output = run_gh_with_input(host, dir, &endpoint, &body)?;
    if !output.ok {
        return Err(redact_gh_error("Could not submit the review", &output));
    }
    let created: serde_json::Value = serde_json::from_str(&output.stdout)
        .map_err(|_| "GitHub did not say which review it opened".to_string())?;
    let (Some(id), Some(node)) = (
        created.get("id").and_then(serde_json::Value::as_i64),
        created.get("node_id").and_then(serde_json::Value::as_str),
    ) else {
        return Err("GitHub did not say which review it opened".to_string());
    };
    let finish = || -> Result<(), String> {
        for reply in &review.replies {
            let args = reply_in_review_args(node, &reply.thread_id, &reply.body);
            write(host, dir, &args, "Could not add a reply to the review")?;
        }
        let events = format!("{endpoint}/{id}/events");
        let output = run_gh_with_input(
            host,
            dir,
            &events,
            &submit_event_payload(review).to_string(),
        )?;
        if !output.ok {
            return Err(redact_gh_error("Could not submit the review", &output));
        }
        Ok(())
    };
    finish().inspect_err(|_| {
        let discard = strings(&["api", "--method", "DELETE", &format!("{endpoint}/{id}")]);
        let _ = write(host, dir, &discard, "Could not discard the pending review");
    })
}

pub(crate) fn reply_to_comment(
    host: &Host,
    dir: &Path,
    slug: &str,
    number: i64,
    comment_id: i64,
    body: &str,
) -> Result<(), String> {
    let endpoint = format!("repos/{slug}/pulls/{number}/comments/{comment_id}/replies");
    let args = strings(&[
        "api",
        "--method",
        "POST",
        &endpoint,
        "-f",
        &format!("body={body}"),
    ]);
    write(host, dir, &args, "Could not post the reply").map(drop)
}

pub(crate) fn resolve_thread(host: &Host, dir: &Path, thread_id: &str) -> Result<(), String> {
    let args = strings(&[
        "api",
        "graphql",
        "-f",
        &format!("query={RESOLVE_THREAD_MUTATION}"),
        "-f",
        &format!("threadId={thread_id}"),
    ]);
    write(host, dir, &args, "Could not resolve the thread").map(drop)
}

fn comment_endpoint(slug: &str, comment: CommentRef) -> String {
    match comment {
        CommentRef::Review(id) => format!("repos/{slug}/pulls/comments/{id}"),
        CommentRef::Conversation(id) => format!("repos/{slug}/issues/comments/{id}"),
    }
}

fn edit_comment_args(slug: &str, comment: CommentRef, body: &str) -> Vec<String> {
    strings(&[
        "api",
        "--method",
        "PATCH",
        &comment_endpoint(slug, comment),
        "-f",
        &format!("body={body}"),
    ])
}

pub(crate) fn edit_comment(
    host: &Host,
    dir: &Path,
    slug: &str,
    comment: CommentRef,
    body: &str,
) -> Result<(), String> {
    let args = edit_comment_args(slug, comment, body);
    write(host, dir, &args, "Could not edit the comment").map(drop)
}

pub(crate) fn delete_comment(
    host: &Host,
    dir: &Path,
    slug: &str,
    comment: CommentRef,
) -> Result<(), String> {
    let args = strings(&[
        "api",
        "--method",
        "DELETE",
        &comment_endpoint(slug, comment),
    ]);
    write(host, dir, &args, "Could not delete the comment").map(drop)
}

fn merge_args(slug: &str, number: i64, request: &MergeRequest) -> Result<Vec<String>, String> {
    let method = match request.method {
        MergeMethod::Merge => "--merge",
        MergeMethod::Squash => "--squash",
        MergeMethod::Rebase => "--rebase",
    };
    let mut args = strings(&["pr", "merge", &number.to_string(), "--repo", slug, method]);
    if let Some(head) = &request.head_sha {
        crate::git::objects::require_sha(head)?;
        args.push(format!("--match-head-commit={head}"));
    }
    Ok(args)
}

pub(crate) fn merge(
    host: &Host,
    dir: &Path,
    slug: &str,
    number: i64,
    request: &MergeRequest,
) -> Result<(), String> {
    let args = merge_args(slug, number, request)?;
    write(host, dir, &args, "Could not merge the pull request").map(drop)
}

/// `gh pr <verb> <n> --repo <slug>`, for the writes that take nothing else.
fn pr_verb(
    host: &Host,
    dir: &Path,
    slug: &str,
    verb: &str,
    number: i64,
    failed: &str,
) -> Result<(), String> {
    let args = strings(&["pr", verb, &number.to_string(), "--repo", slug]);
    write(host, dir, &args, failed).map(drop)
}

pub(crate) fn close(host: &Host, dir: &Path, slug: &str, number: i64) -> Result<(), String> {
    pr_verb(
        host,
        dir,
        slug,
        "close",
        number,
        "Could not close the pull request",
    )
}

pub(crate) fn mark_ready(host: &Host, dir: &Path, slug: &str, number: i64) -> Result<(), String> {
    pr_verb(
        host,
        dir,
        slug,
        "ready",
        number,
        "Could not mark the pull request ready",
    )
}

fn retarget_args(slug: &str, number: i64, base: &str) -> Result<Vec<String>, String> {
    let base = require_branch(base)?;
    Ok(strings(&[
        "pr",
        "edit",
        &number.to_string(),
        "--repo",
        slug,
        &format!("--base={base}"),
    ]))
}

pub(crate) fn retarget(
    host: &Host,
    dir: &Path,
    slug: &str,
    number: i64,
    base: &str,
) -> Result<(), String> {
    let args = retarget_args(slug, number, base)?;
    write(host, dir, &args, "Could not retarget the pull request").map(drop)
}

fn create_pr_args(slug: &str, pr: &NewPr) -> Result<Vec<String>, String> {
    let mut args = strings(&[
        "pr",
        "create",
        "--repo",
        slug,
        &format!("--base={}", require_branch(&pr.base)?),
        &format!("--head={}", require_branch(&pr.head)?),
        &format!("--title={}", pr.title),
        &format!("--body={}", pr.body),
    ]);
    if pr.draft {
        args.push("--draft".to_string());
    }
    Ok(args)
}

/// The PR number at the end of the URL `gh pr create` prints.
fn number_from_pr_url(stdout: &str) -> Option<i64> {
    let url = stdout.lines().rev().find(|line| line.contains("/pull/"))?;
    url.trim().rsplit('/').next()?.parse().ok()
}

pub(crate) fn create_pr(host: &Host, dir: &Path, slug: &str, pr: &NewPr) -> Result<i64, String> {
    let args = create_pr_args(slug, pr)?;
    let stdout = write(host, dir, &args, "Could not open the pull request")?;
    number_from_pr_url(&stdout)
        .ok_or_else(|| "GitHub did not say which pull request it opened".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_review_is_pinned_to_the_head_it_was_written_against() {
        let sha = "a".repeat(40);
        let review = ReviewSubmission {
            verdict: ReviewVerdict::Approve,
            body: String::new(),
            commit_id: Some(sha.clone()),
            comments: vec![PendingComment {
                path: "src/lib.rs".into(),
                line: 3,
                side: None,
                start_line: None,
                start_side: None,
                body: "nit".into(),
            }],
            replies: vec![],
        };
        let payload = review_payload(&review, false);
        assert_eq!(payload["commit_id"], sha);
        assert_eq!(payload["event"], "APPROVE");
        assert_eq!(payload["comments"][0]["side"], "RIGHT");
        assert!(payload["comments"][0].get("start_line").is_none());
        assert!(review_payload(&ReviewSubmission::default(), false)
            .get("commit_id")
            .is_none());
    }

    #[test]
    fn a_review_with_replies_opens_pending_and_ranges_carry_their_first_line() {
        let review = ReviewSubmission {
            verdict: ReviewVerdict::RequestChanges,
            body: "See inline".into(),
            commit_id: None,
            comments: vec![PendingComment {
                path: "a.rs".into(),
                line: 9,
                side: Some("LEFT".into()),
                start_line: Some(7),
                start_side: None,
                body: "range".into(),
            }],
            replies: vec![crate::review::types::ReviewReply {
                thread_id: "PRRT_1".into(),
                body: "Agreed".into(),
            }],
        };
        let pending = review_payload(&review, true);
        assert!(pending.get("event").is_none());
        assert_eq!(pending["comments"][0]["start_line"], 7);
        assert_eq!(pending["comments"][0]["start_side"], "LEFT");
        assert_eq!(
            submit_event_payload(&review),
            json!({"event": "REQUEST_CHANGES", "body": "See inline"})
        );
        let args = reply_in_review_args("PRR_9", "PRRT_1", "Agreed");
        assert!(args.contains(&"review=PRR_9".to_string()));
        assert!(args.contains(&"thread=PRRT_1".to_string()));
        assert!(args.contains(&"body=Agreed".to_string()));
    }

    #[test]
    fn an_unknown_event_never_escalates_to_an_approval() {
        assert_eq!(ReviewVerdict::from_event("approve"), ReviewVerdict::Approve);
        assert_eq!(ReviewVerdict::from_event("APPROVE"), ReviewVerdict::Approve);
        assert_eq!(
            ReviewVerdict::from_event("request-changes"),
            ReviewVerdict::RequestChanges
        );
        assert_eq!(
            ReviewVerdict::from_event("REQUEST_CHANGES"),
            ReviewVerdict::RequestChanges
        );
        assert_eq!(ReviewVerdict::from_event("comment"), ReviewVerdict::Comment);
        assert_eq!(
            ReviewVerdict::from_event("merge-it-please"),
            ReviewVerdict::Comment
        );
    }

    #[test]
    fn inline_and_conversation_comments_live_at_different_endpoints() {
        assert_eq!(
            edit_comment_args("o/r", CommentRef::Review(5), "@file")[3],
            "repos/o/r/pulls/comments/5"
        );
        assert_eq!(
            comment_endpoint("o/r", CommentRef::Conversation(9)),
            "repos/o/r/issues/comments/9"
        );
    }

    #[test]
    fn a_merge_names_its_method_and_the_head_it_expects() {
        let sha = "b".repeat(40);
        let args = merge_args(
            "o/r",
            7,
            &MergeRequest {
                method: MergeMethod::Squash,
                head_sha: Some(sha.clone()),
            },
        )
        .unwrap();
        assert_eq!(
            args,
            [
                "pr",
                "merge",
                "7",
                "--repo",
                "o/r",
                "--squash",
                &format!("--match-head-commit={sha}")
            ]
        );
        let bad_head = MergeRequest {
            method: MergeMethod::Merge,
            head_sha: Some("HEAD".into()),
        };
        assert!(merge_args("o/r", 7, &bad_head).is_err());
    }

    #[test]
    fn option_like_git_branch_names_are_refused() {
        assert_eq!(
            retarget_args("o/r", 3, "main").unwrap().last().unwrap(),
            "--base=main"
        );
        assert!(retarget_args("o/r", 3, "--help").is_err());
        assert!(retarget_args("o/r", 3, " ").is_err());
        let pr = NewPr {
            base: "main".into(),
            head: "-x".into(),
            ..NewPr::default()
        };
        assert!(create_pr_args("o/r", &pr).is_err());
    }

    #[test]
    fn a_new_pr_is_read_back_from_the_url_gh_prints() {
        let pr = NewPr {
            base: "feat/a".into(),
            head: "feat/b".into(),
            title: "-Title".into(),
            body: String::new(),
            draft: true,
        };
        let args = create_pr_args("o/r", &pr).unwrap();
        assert!(args.contains(&"--title=-Title".to_string()));
        assert_eq!(args.last().unwrap(), "--draft");
        assert_eq!(
            number_from_pr_url("Creating pull request\nhttps://github.com/o/r/pull/431\n"),
            Some(431)
        );
        assert_eq!(number_from_pr_url("nothing useful"), None);
    }
}
