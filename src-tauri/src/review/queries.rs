//! Fetching pull requests: the `gh` field lists, the GraphQL query for inline
//! review threads, and the local-diff path that works before a PR exists.

use serde_json::Value;
use std::path::Path;

use super::diff::parse_git_diff;
use super::gh::{redact_gh_error, repo_name_with_owner, run_gh, READ_TIMEOUT};
use super::parse::{
    compute_freshness, parse_files, parse_pr_list, parse_pr_view, parse_review_threads, str_field,
};
use super::types::{PrComment, PrDetail, PrFile, PrFreshness, PrSummary};
use crate::git::run_git;

/// How many open PRs the picker lists.
const PR_LIST_LIMIT: &str = "50";

const PR_LIST_FIELDS: &str =
    "number,title,author,state,isDraft,baseRefName,headRefName,updatedAt,url";
const PR_VIEW_FIELDS: &str = "number,title,author,state,isDraft,baseRefName,headRefName,\
    updatedAt,url,body,headRefOid,commits,comments,statusCheckRollup";

const REVIEW_THREADS_QUERY: &str = "query($owner:String!,$repo:String!,$number:Int!){\
  repository(owner:$owner,name:$repo){\
    pullRequest(number:$number){\
      reviewThreads(first:100){nodes{\
        id isResolved \
        comments(first:100){nodes{\
          databaseId body createdAt path line originalLine diffSide \
          author{login}\
        }}\
      }}\
    }\
  }\
}";

pub(crate) fn list_open_prs(dir: &Path) -> Result<Vec<PrSummary>, String> {
    let output = run_gh(
        dir,
        &[
            "pr",
            "list",
            "--state",
            "open",
            "--limit",
            PR_LIST_LIMIT,
            "--json",
            PR_LIST_FIELDS,
        ],
        READ_TIMEOUT,
    )?;
    if !output.ok {
        return Err(redact_gh_error("Could not list pull requests", &output));
    }
    let json: Value = serde_json::from_str(&output.stdout)
        .map_err(|_| "Could not parse the pull-request list".to_string())?;
    Ok(parse_pr_list(&json))
}

pub(crate) fn fetch_pr_detail(dir: &Path, number: i64) -> Result<PrDetail, String> {
    let number_str = number.to_string();
    let view = run_gh(
        dir,
        &["pr", "view", &number_str, "--json", PR_VIEW_FIELDS],
        READ_TIMEOUT,
    )?;
    if !view.ok {
        return Err(redact_gh_error("Could not read the pull request", &view));
    }
    let view_json: Value = serde_json::from_str(&view.stdout)
        .map_err(|_| "Could not parse the pull request".to_string())?;
    let parsed = parse_pr_view(&view_json);

    let files_output = run_gh(
        dir,
        &[
            "api",
            "--paginate",
            &format!("repos/{{owner}}/{{repo}}/pulls/{number}/files"),
        ],
        READ_TIMEOUT,
    )?;
    let (files, files_truncated) = if files_output.ok {
        // `--paginate` concatenates JSON arrays; join them into one array.
        let merged = merge_paginated_arrays(&files_output.stdout);
        parse_files(&merged)
    } else {
        (Vec::new(), false)
    };

    let mut comments = parsed.conversation;
    comments.extend(fetch_review_threads(dir, number));

    Ok(PrDetail {
        summary: parsed.summary,
        body: parsed.body,
        head_sha: parsed.head_sha,
        commits: parsed.commits,
        files,
        comments,
        checks: parsed.checks,
        files_truncated,
    })
}

/// Inline review threads, which the `pr view` JSON does not carry. Best-effort:
/// a failure just omits resolved state rather than failing the whole PR read.
fn fetch_review_threads(dir: &Path, number: i64) -> Vec<PrComment> {
    let Ok((owner, name)) = repo_name_with_owner(dir) else {
        return Vec::new();
    };
    let number = number.to_string();
    let args = [
        "api",
        "graphql",
        "-f",
        &format!("query={REVIEW_THREADS_QUERY}"),
        "-f",
        &format!("owner={owner}"),
        "-f",
        &format!("repo={name}"),
        "-F",
        &format!("number={number}"),
    ];
    match run_gh(dir, &args, READ_TIMEOUT) {
        Ok(output) if output.ok => serde_json::from_str(&output.stdout)
            .map(|json: Value| parse_review_threads(&json))
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// `gh api --paginate` emits one JSON array per page, concatenated. Flatten them
/// into a single array of file objects.
fn merge_paginated_arrays(stdout: &str) -> Value {
    // The simple, common case: a single well-formed array.
    if let Ok(value) = serde_json::from_str::<Value>(stdout.trim()) {
        if value.is_array() {
            return value;
        }
    }
    let mut merged = Vec::new();
    let mut depth = 0usize;
    let mut start = None;
    for (index, byte) in stdout.char_indices() {
        match byte {
            '[' => {
                if depth == 0 {
                    start = Some(index);
                }
                depth += 1;
            }
            ']' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    if let Some(begin) = start.take() {
                        if let Ok(Value::Array(items)) =
                            serde_json::from_str::<Value>(&stdout[begin..=index])
                        {
                            merged.extend(items);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    Value::Array(merged)
}

pub(crate) fn check_freshness(
    dir: &Path,
    number: i64,
    known_head: &str,
    known_updated_at: &str,
) -> Result<PrFreshness, String> {
    let number_str = number.to_string();
    let output = run_gh(
        dir,
        &["pr", "view", &number_str, "--json", "headRefOid,updatedAt"],
        READ_TIMEOUT,
    )?;
    if !output.ok {
        return Err(redact_gh_error(
            "Could not refresh the pull request",
            &output,
        ));
    }
    let json: Value = serde_json::from_str(&output.stdout)
        .map_err(|_| "Could not parse the pull request".to_string())?;
    Ok(compute_freshness(
        known_head,
        known_updated_at,
        &str_field(&json, "headRefOid"),
        &str_field(&json, "updatedAt"),
    ))
}

/// Diff the working tree against a base revision, producing the same `PrFile`
/// shape as a real PR so the review view works before a PR exists.
pub(crate) fn local_diff(
    dir: &Path,
    base: &str,
    head: Option<&str>,
) -> Result<Vec<PrFile>, String> {
    let range = match head {
        Some(head) if !head.trim().is_empty() => format!("{base}...{head}"),
        _ => base.to_string(),
    };
    // Reuses the native Git service's runner, so the local diff gets the same
    // argument-array discipline and timeout handling as everything else.
    let output = run_git(dir, &["diff", "--no-color", &range], READ_TIMEOUT)?;
    if !output.ok {
        return Err("Could not diff against that base revision".to_string());
    }
    Ok(parse_git_diff(&output.stdout))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_paginated_file_arrays() {
        let stdout = "[{\"filename\":\"a\"}]\n[{\"filename\":\"b\"}]";
        let merged = merge_paginated_arrays(stdout);
        let arr = merged.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["filename"], "a");
        assert_eq!(arr[1]["filename"], "b");
    }

    #[test]
    fn a_single_page_array_passes_through_unchanged() {
        let merged = merge_paginated_arrays("[{\"filename\":\"only\"}]");
        assert_eq!(merged.as_array().unwrap().len(), 1);
    }

    #[test]
    fn unparseable_pagination_output_yields_an_empty_array() {
        assert_eq!(
            merge_paginated_arrays("not json at all"),
            Value::Array(vec![])
        );
    }
}
