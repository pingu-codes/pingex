//! Fetching pull requests: the `gh` field lists and the GraphQL query for
//! inline review threads.

use serde_json::Value;
use std::path::Path;

use crate::review::types::{PrComment, PrDetail, PrHead, PrSummary};
use crate::util::host::Host;

use super::parse::{parse_files, parse_pr_list, parse_pr_view, parse_review_threads, str_field};
use super::run::{gh_graphql, redact_gh_error, run_gh, GqlVar, READ_TIMEOUT};

/// How many open PRs the picker lists.
const PR_LIST_LIMIT: &str = "50";

const PR_LIST_FIELDS: &str =
    "number,title,author,state,isDraft,baseRefName,headRefName,updatedAt,url";
const PR_VIEW_FIELDS: &str = "number,title,author,state,isDraft,baseRefName,headRefName,\
    updatedAt,url,body,headRefOid,baseRefOid,commits,comments,reviews,statusCheckRollup";

/// Paginated on `reviewThreads` (see `gh_graphql`); a thread's first 100
/// comments are plenty for display. `diffSide` lives on the thread: GitHub
/// refuses the whole query if a comment asks for it.
const REVIEW_THREADS_QUERY: &str =
    "query($owner:String!,$repo:String!,$number:Int!,$endCursor:String){\
  repository(owner:$owner,name:$repo){\
    pullRequest(number:$number){\
      reviewThreads(first:100,after:$endCursor){pageInfo{hasNextPage endCursor} nodes{\
        id isResolved diffSide \
        comments(first:100){nodes{\
          databaseId body createdAt path line originalLine \
          author{login} pullRequestReview{id}\
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
        base_sha: parsed.base_sha,
        commits: parsed.commits,
        files,
        comments,
        reviews: parsed.reviews,
        checks: parsed.checks,
        files_truncated,
    })
}

/// Inline review threads, which the `pr view` JSON does not carry. Best-effort:
/// a failure leaves the PR without inline comments rather than failing the
/// whole read, so it is logged rather than lost.
fn fetch_review_threads(host: &Host, dir: &Path, number: i64) -> Vec<PrComment> {
    let Ok((owner, name)) = super::inbox::repo_slug(host, dir) else {
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
        Err(error) => {
            eprintln!("could not read review threads for PR #{number}: {error}");
            Vec::new()
        }
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

/// The PR's head and `updatedAt` as GitHub has them now.
pub(crate) fn pr_head(host: &Host, dir: &Path, number: i64) -> Result<PrHead, String> {
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
    Ok(PrHead {
        head_sha: str_field(&json, "headRefOid"),
        updated_at: str_field(&json, "updatedAt"),
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
