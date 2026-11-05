//! Fetching pull requests: the `gh` field lists, the GraphQL query for inline
//! review threads, and the local-diff path that works before a PR exists.

use serde_json::Value;
use std::path::Path;

use crate::util::host::Host;

use super::diff::parse_git_diff;
use super::gh::{gh_graphql, redact_gh_error, repo_name_with_owner, run_gh, GqlVar, READ_TIMEOUT};
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

/// Paginated on `reviewThreads` (see `gh_graphql`); a thread's first 100
/// comments are plenty for display.
const REVIEW_THREADS_QUERY: &str =
    "query($owner:String!,$repo:String!,$number:Int!,$endCursor:String){\
  repository(owner:$owner,name:$repo){\
    pullRequest(number:$number){\
      reviewThreads(first:100,after:$endCursor){pageInfo{hasNextPage endCursor} nodes{\
        id isResolved \
        comments(first:100){nodes{\
          databaseId body createdAt path line originalLine diffSide \
          author{login}\
        }}\
      }}\
    }\
  }\
}";

pub(crate) fn list_open_prs(host: &Host, dir: &Path) -> Result<Vec<PrSummary>, String> {
    let output = run_gh(
        host,
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

pub(crate) fn fetch_pr_detail(host: &Host, dir: &Path, number: i64) -> Result<PrDetail, String> {
    let number_str = number.to_string();
    let view = run_gh(
        host,
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
        host,
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
    comments.extend(fetch_review_threads(host, dir, number));

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
fn fetch_review_threads(host: &Host, dir: &Path, number: i64) -> Vec<PrComment> {
    let Ok((owner, name)) = repo_name_with_owner(host, dir) else {
        return Vec::new();
    };
    let vars = [
        ("owner", GqlVar::Str(&owner)),
        ("repo", GqlVar::Str(&name)),
        ("number", GqlVar::Int(number)),
    ];
    // Up to 1,000 threads.
    match gh_graphql(host, dir, REVIEW_THREADS_QUERY, &vars, 10) {
        Ok(read) => read.pages.iter().flat_map(parse_review_threads).collect(),
        Err(_) => Vec::new(),
    }
}

/// `gh api --paginate` emits one JSON array per page, concatenated. Flatten them
/// into a single array of file objects.
pub(crate) fn merge_paginated_arrays(stdout: &str) -> Value {
    let mut merged = Vec::new();
    for page in serde_json::Deserializer::from_str(stdout).into_iter::<Value>() {
        match page {
            Ok(Value::Array(items)) => merged.extend(items),
            Ok(_) => {}
            // Past a malformed page there is no telling where the next begins.
            Err(_) => break,
        }
    }
    Value::Array(merged)
}

pub(crate) fn check_freshness(
    host: &Host,
    dir: &Path,
    number: i64,
    known_head: &str,
    known_updated_at: &str,
) -> Result<PrFreshness, String> {
    let number_str = number.to_string();
    let output = run_gh(
        host,
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
    host: &crate::util::host::Host,
    dir: &Path,
    base: &str,
    head: Option<&str>,
) -> Result<Vec<PrFile>, String> {
    let range = diff_range(base, head)?;
    // Reuses the native Git service's runner, so the local diff gets the same
    // argument-array discipline and timeout handling as everything else. The
    // trailing `--` keeps the range from being read as a path.
    let output = run_git(
        host,
        dir,
        &["diff", "--no-color", &range, "--"],
        READ_TIMEOUT,
    )?;
    if !output.ok {
        return Err("Could not diff against that base revision".to_string());
    }
    Ok(parse_git_diff(&output.stdout))
}

/// The `git diff` revision argument for a user-typed base and head. A
/// revision starting with `-` would be parsed as an option (`--output=...`
/// writes a file), so it is refused rather than passed through.
fn diff_range(base: &str, head: Option<&str>) -> Result<String, String> {
    let base = base.trim();
    let head = head.map(str::trim).filter(|head| !head.is_empty());
    if base.is_empty() {
        return Err("Enter a base revision".to_string());
    }
    if base.starts_with('-') || head.is_some_and(|head| head.starts_with('-')) {
        return Err("A revision cannot start with '-'".to_string());
    }
    Ok(match head {
        Some(head) => format!("{base}...{head}"),
        None => base.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bracket_inside_a_string_does_not_drop_a_page() {
        let stdout = "[{\"filename\":\"a\",\"patch\":\"+let x = [1\"}]\n[{\"filename\":\"b\"}]";
        let merged = merge_paginated_arrays(stdout);
        let names: Vec<&str> = merged
            .as_array()
            .unwrap()
            .iter()
            .map(|file| file["filename"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["a", "b"]);
    }

    #[test]
    fn diff_ranges_refuse_option_like_revisions() {
        assert_eq!(diff_range("main", None).unwrap(), "main");
        assert_eq!(diff_range("main", Some("feat")).unwrap(), "main...feat");
        assert_eq!(diff_range(" main ", Some(" ")).unwrap(), "main");
        assert!(diff_range("--output=/tmp/x", None).is_err());
        assert!(diff_range("main", Some("--output=/tmp/x")).is_err());
        assert!(diff_range("", None).is_err());
    }

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
