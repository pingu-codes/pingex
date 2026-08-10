//! Mapping `gh` JSON onto the provider-neutral wire types.
//!
//! Nothing here touches the network or the filesystem, so all of it is unit
//! tested against fixture JSON. Diff bodies are handed to `super::diff`.

use serde_json::Value;

use super::diff::parse_patch;
use super::types::{ChecksSummary, PrComment, PrCommit, PrFile, PrFreshness, PrSummary};

/// Upper bound on changed files fetched for one PR so a huge diff cannot stall
/// the UI; the frontend shows a "truncated" note when this bites.
pub(crate) const MAX_FILES: usize = 300;

/// A string field as an owned `String`, empty when absent — the shape most of
/// the `gh` mappings below want.
pub(crate) fn str_field(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

/// Author login for a `gh` object shaped `{ "author": { "login": "x" } }` or a
/// flat `{ "login": "x" }` / `{ "user": { "login": "x" } }`.
fn author_login(value: &Value) -> String {
    for key in ["author", "user"] {
        if let Some(login) = value
            .get(key)
            .and_then(|nested| nested.get("login"))
            .and_then(Value::as_str)
        {
            return login.to_string();
        }
    }
    value
        .get("login")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

/// Map `gh pr list --json ...` output to summaries.
pub(crate) fn parse_pr_list(json: &Value) -> Vec<PrSummary> {
    json.as_array()
        .map(|items| items.iter().map(parse_pr_summary).collect())
        .unwrap_or_default()
}

fn parse_pr_summary(value: &Value) -> PrSummary {
    PrSummary {
        number: value.get("number").and_then(Value::as_i64).unwrap_or(0),
        title: str_field(value, "title"),
        author: author_login(value),
        state: str_field(value, "state"),
        is_draft: value
            .get("isDraft")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        base_ref: str_field(value, "baseRefName"),
        head_ref: str_field(value, "headRefName"),
        updated_at: str_field(value, "updatedAt"),
        url: str_field(value, "url"),
    }
}

/// Roll up `statusCheckRollup` nodes into passing/failing/pending counts.
fn parse_checks(value: &Value) -> Option<ChecksSummary> {
    let nodes = value.get("statusCheckRollup").and_then(Value::as_array)?;
    let mut summary = ChecksSummary {
        total: nodes.len() as i64,
        ..ChecksSummary::default()
    };
    for node in nodes {
        // CheckRun uses `status`/`conclusion`; StatusContext uses `state`.
        let status = str_field(node, "status").to_uppercase();
        let conclusion = str_field(node, "conclusion").to_uppercase();
        let state = str_field(node, "state").to_uppercase();
        let outcome = if !conclusion.is_empty() {
            conclusion
        } else if !state.is_empty() {
            state
        } else {
            status.clone()
        };
        match outcome.as_str() {
            "SUCCESS" | "NEUTRAL" | "SKIPPED" => summary.passing += 1,
            "FAILURE" | "ERROR" | "CANCELLED" | "TIMED_OUT" | "ACTION_REQUIRED" => {
                summary.failing += 1
            }
            _ => summary.pending += 1,
        }
    }
    Some(summary)
}

/// Map `gh pr view --json ...` output to the metadata half of a `PrDetail`.
/// Returns the summary, body, head sha, commits, checks, and conversation
/// comments (inline comments come from a separate GraphQL call).
pub(crate) struct PrView {
    pub(crate) summary: PrSummary,
    pub(crate) body: String,
    pub(crate) head_sha: String,
    pub(crate) commits: Vec<PrCommit>,
    pub(crate) checks: Option<ChecksSummary>,
    pub(crate) conversation: Vec<PrComment>,
}

pub(crate) fn parse_pr_view(json: &Value) -> PrView {
    let commits = json
        .get("commits")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|commit| {
                    let oid = str_field(commit, "oid");
                    let author = commit
                        .get("authors")
                        .and_then(Value::as_array)
                        .and_then(|authors| authors.first())
                        .map(|a| {
                            let login = a.get("login").and_then(Value::as_str).unwrap_or("");
                            if login.is_empty() {
                                a.get("name").and_then(Value::as_str).unwrap_or("")
                            } else {
                                login
                            }
                        })
                        .unwrap_or("")
                        .to_string();
                    PrCommit {
                        short_oid: oid.chars().take(7).collect(),
                        oid,
                        headline: str_field(commit, "messageHeadline"),
                        author,
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    let conversation = json
        .get("comments")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|comment| PrComment {
                    id: comment.get("id").and_then(Value::as_i64).unwrap_or(0),
                    author: author_login(comment),
                    body: str_field(comment, "body"),
                    created_at: str_field(comment, "createdAt"),
                    ..PrComment::default()
                })
                .collect()
        })
        .unwrap_or_default();

    PrView {
        summary: parse_pr_summary(json),
        body: str_field(json, "body"),
        head_sha: str_field(json, "headRefOid"),
        commits,
        checks: parse_checks(json),
        conversation,
    }
}

/// Parse a GitHub files-API array into `PrFile`s, parsing each `patch` into
/// hunks with stable line anchors.
pub(crate) fn parse_files(json: &Value) -> (Vec<PrFile>, bool) {
    let Some(items) = json.as_array() else {
        return (Vec::new(), false);
    };
    let truncated = items.len() > MAX_FILES;
    let files = items
        .iter()
        .take(MAX_FILES)
        .map(|file| {
            let patch = file
                .get("patch")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            // GitHub omits `patch` for binary or very large files.
            let patch_truncated = patch.is_empty();
            let status = match str_field(file, "status").as_str() {
                "added" => "added",
                "removed" => "removed",
                "renamed" => "renamed",
                _ => "modified",
            }
            .to_string();
            PrFile {
                path: str_field(file, "filename"),
                old_path: file
                    .get("previous_filename")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                status,
                additions: file.get("additions").and_then(Value::as_i64).unwrap_or(0),
                deletions: file.get("deletions").and_then(Value::as_i64).unwrap_or(0),
                hunks: parse_patch(&patch),
                patch,
                patch_truncated,
            }
        })
        .collect();
    (files, truncated)
}

/// Parse the GraphQL `reviewThreads` response into flattened inline comments,
/// each tagged with its thread node id and resolved state.
pub(crate) fn parse_review_threads(json: &Value) -> Vec<PrComment> {
    let nodes = json
        .pointer("/data/repository/pullRequest/reviewThreads/nodes")
        .and_then(Value::as_array);
    let Some(nodes) = nodes else {
        return Vec::new();
    };
    let mut comments = Vec::new();
    for thread in nodes {
        let thread_id = thread.get("id").and_then(Value::as_str).map(str::to_string);
        let is_resolved = thread
            .get("isResolved")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let Some(thread_comments) = thread
            .get("comments")
            .and_then(|c| c.get("nodes"))
            .and_then(Value::as_array)
        else {
            continue;
        };
        for comment in thread_comments {
            comments.push(PrComment {
                id: comment
                    .get("databaseId")
                    .and_then(Value::as_i64)
                    .unwrap_or(0),
                author: author_login(comment),
                body: str_field(comment, "body"),
                created_at: str_field(comment, "createdAt"),
                path: comment
                    .get("path")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                line: comment
                    .get("line")
                    .and_then(Value::as_i64)
                    .or_else(|| comment.get("originalLine").and_then(Value::as_i64)),
                side: Some(
                    comment
                        .get("diffSide")
                        .and_then(Value::as_str)
                        .unwrap_or("RIGHT")
                        .to_string(),
                ),
                thread_id: thread_id.clone(),
                is_resolved,
            });
        }
    }
    comments
}

/// Decide whether a locally-open PR is stale relative to the remote. Any change
/// to the head SHA or the `updatedAt` timestamp counts as drift.
pub(crate) fn compute_freshness(
    known_head: &str,
    known_updated_at: &str,
    remote_head: &str,
    remote_updated_at: &str,
) -> PrFreshness {
    let stale = (!remote_head.is_empty() && remote_head != known_head)
        || (!remote_updated_at.is_empty() && remote_updated_at != known_updated_at);
    PrFreshness {
        stale,
        remote_head: remote_head.to_string(),
        remote_updated_at: remote_updated_at.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_pr_list_summaries() {
        let json = json!([
            {
                "number": 42,
                "title": "Add review view",
                "author": {"login": "octocat"},
                "state": "OPEN",
                "isDraft": false,
                "baseRefName": "main",
                "headRefName": "feature/review",
                "updatedAt": "2026-07-20T10:00:00Z",
                "url": "https://github.com/o/r/pull/42"
            }
        ]);
        let prs = parse_pr_list(&json);
        assert_eq!(prs.len(), 1);
        assert_eq!(prs[0].number, 42);
        assert_eq!(prs[0].author, "octocat");
        assert_eq!(prs[0].base_ref, "main");
        assert_eq!(prs[0].head_ref, "feature/review");
        assert!(!prs[0].is_draft);
    }
    #[test]
    fn parses_pr_view_metadata_commits_and_checks() {
        let json = json!({
            "number": 7,
            "title": "Fix bug",
            "author": {"login": "dev"},
            "state": "OPEN",
            "isDraft": true,
            "baseRefName": "main",
            "headRefName": "fix",
            "updatedAt": "2026-07-21T09:00:00Z",
            "url": "https://github.com/o/r/pull/7",
            "body": "Closes #1",
            "headRefOid": "abcdef1234567890",
            "commits": [
                {"oid": "abcdef1234567890", "messageHeadline": "Fix the bug",
                 "authors": [{"login": "dev", "name": "Dev"}]}
            ],
            "comments": [
                {"id": 100, "author": {"login": "rev"}, "body": "LGTM overall",
                 "createdAt": "2026-07-21T09:30:00Z"}
            ],
            "statusCheckRollup": [
                {"__typename": "CheckRun", "status": "COMPLETED", "conclusion": "SUCCESS"},
                {"__typename": "CheckRun", "status": "COMPLETED", "conclusion": "FAILURE"},
                {"__typename": "CheckRun", "status": "IN_PROGRESS", "conclusion": ""}
            ]
        });
        let view = parse_pr_view(&json);
        assert_eq!(view.summary.number, 7);
        assert!(view.summary.is_draft);
        assert_eq!(view.head_sha, "abcdef1234567890");
        assert_eq!(view.body, "Closes #1");
        assert_eq!(view.commits.len(), 1);
        assert_eq!(view.commits[0].short_oid, "abcdef1");
        assert_eq!(view.commits[0].author, "dev");
        assert_eq!(view.conversation.len(), 1);
        assert_eq!(view.conversation[0].author, "rev");
        let checks = view.checks.expect("checks");
        assert_eq!(checks.total, 3);
        assert_eq!(checks.passing, 1);
        assert_eq!(checks.failing, 1);
        assert_eq!(checks.pending, 1);
    }
    #[test]
    fn parses_files_with_patch_into_hunks_and_anchors() {
        let json = json!([
            {
                "filename": "src/main.rs",
                "status": "modified",
                "additions": 2,
                "deletions": 1,
                "patch": "@@ -10,3 +10,4 @@ fn main() {\n ctx\n-old line\n+new line\n+added line"
            },
            {
                "filename": "image.png",
                "status": "added",
                "additions": 0,
                "deletions": 0
            }
        ]);
        let (files, truncated) = parse_files(&json);
        assert!(!truncated);
        assert_eq!(files.len(), 2);
        let main = &files[0];
        assert_eq!(main.path, "src/main.rs");
        assert_eq!(main.status, "modified");
        assert_eq!(main.hunks.len(), 1);
        let hunk = &main.hunks[0];
        assert_eq!(hunk.old_start, 10);
        assert_eq!(hunk.new_start, 10);
        // context, del, add, add
        assert_eq!(hunk.lines.len(), 4);
        assert_eq!(hunk.lines[0].kind, "context");
        assert_eq!(hunk.lines[0].old_line, Some(10));
        assert_eq!(hunk.lines[0].new_line, Some(10));
        assert_eq!(hunk.lines[1].kind, "del");
        assert_eq!(hunk.lines[1].old_line, Some(11));
        assert_eq!(hunk.lines[1].new_line, None);
        assert_eq!(hunk.lines[2].kind, "add");
        assert_eq!(hunk.lines[2].new_line, Some(11));
        assert_eq!(hunk.lines[3].kind, "add");
        assert_eq!(hunk.lines[3].new_line, Some(12));
        // Binary/omitted patch is flagged.
        assert!(files[1].patch_truncated);
        assert!(files[1].hunks.is_empty());
    }
    #[test]
    fn parses_review_threads_with_resolution() {
        let json = json!({
            "data": {"repository": {"pullRequest": {"reviewThreads": {"nodes": [
                {
                    "id": "THREAD_1",
                    "isResolved": true,
                    "comments": {"nodes": [
                        {"databaseId": 1, "body": "nit", "createdAt": "t1",
                         "path": "a.rs", "line": 5, "diffSide": "RIGHT",
                         "author": {"login": "rev"}},
                        {"databaseId": 2, "body": "fixed", "createdAt": "t2",
                         "path": "a.rs", "line": 5, "diffSide": "RIGHT",
                         "author": {"login": "dev"}}
                    ]}
                }
            ]}}}}
        });
        let comments = parse_review_threads(&json);
        assert_eq!(comments.len(), 2);
        assert_eq!(comments[0].thread_id.as_deref(), Some("THREAD_1"));
        assert!(comments[0].is_resolved);
        assert_eq!(comments[1].id, 2);
        assert_eq!(comments[1].path.as_deref(), Some("a.rs"));
        assert_eq!(comments[1].line, Some(5));
    }
    #[test]
    fn freshness_detects_head_and_timestamp_drift() {
        let same = compute_freshness("abc", "t1", "abc", "t1");
        assert!(!same.stale);
        let new_head = compute_freshness("abc", "t1", "def", "t1");
        assert!(new_head.stale);
        let new_time = compute_freshness("abc", "t1", "abc", "t2");
        assert!(new_time.stale);
        // An empty remote (fetch failed) is not treated as drift.
        let empty = compute_freshness("abc", "t1", "", "");
        assert!(!empty.stale);
    }
}
