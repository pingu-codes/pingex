//! The signed-in account's repositories, and the ones asking it for review,
//! for Add repository.

use std::collections::HashMap;
use std::path::Path;

use serde_json::Value;

use crate::review::types::{GitHubRepo, ReviewRequests};
use crate::util::host::Host;

use super::parse::str_field;
use super::run::{redact_gh_error, run_gh, READ_TIMEOUT};

/// The 100 most recently pushed repositories the account owns, collaborates
/// on, or reaches through an organization.
const MY_REPOS_ENDPOINT: &str =
    "user/repos?per_page=100&sort=pushed&affiliation=owner,collaborator,organization_member";

fn read_json(host: &Host, dir: &Path, args: &[&str], failed: &str) -> Result<Value, String> {
    let output = run_gh(host, dir, args, READ_TIMEOUT)?;
    if !output.ok {
        return Err(redact_gh_error(failed, &output));
    }
    serde_json::from_str(&output.stdout).map_err(|_| failed.to_string())
}

/// Parse GitHub's REST repository list.
pub(crate) fn parse_repos(json: &Value) -> Vec<GitHubRepo> {
    let flag = |repo: &Value, key: &str| repo.get(key).and_then(Value::as_bool).unwrap_or(false);
    json.as_array()
        .into_iter()
        .flatten()
        .map(|repo| GitHubRepo {
            slug: str_field(repo, "full_name"),
            private: flag(repo, "private"),
            fork: flag(repo, "fork"),
            parent: repo
                .pointer("/parent/full_name")
                .and_then(Value::as_str)
                .map(str::to_string),
            default_branch: str_field(repo, "default_branch"),
            clone_url: str_field(repo, "clone_url"),
            ssh_url: str_field(repo, "ssh_url"),
        })
        .filter(|repo| !repo.slug.is_empty())
        .collect()
}

/// Count `gh search prs --json repository` results per repository, most
/// first, then by name.
pub(crate) fn count_review_requests(json: &Value) -> Vec<ReviewRequests> {
    let mut counts: HashMap<String, i64> = HashMap::new();
    for pr in json.as_array().into_iter().flatten() {
        if let Some(slug) = pr
            .pointer("/repository/nameWithOwner")
            .and_then(Value::as_str)
        {
            *counts.entry(slug.to_string()).or_default() += 1;
        }
    }
    let mut requests: Vec<ReviewRequests> = counts
        .into_iter()
        .map(|(slug, count)| ReviewRequests { slug, count })
        .collect();
    requests.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.slug.cmp(&b.slug)));
    requests
}

pub(crate) fn my_repositories(host: &Host, dir: &Path) -> Result<Vec<GitHubRepo>, String> {
    let json = read_json(
        host,
        dir,
        &["api", MY_REPOS_ENDPOINT],
        "Could not list your repositories",
    )?;
    Ok(parse_repos(&json))
}

pub(crate) fn repositories_requesting_review(
    host: &Host,
    dir: &Path,
) -> Result<Vec<ReviewRequests>, String> {
    let json = read_json(
        host,
        dir,
        &[
            "search",
            "prs",
            "--review-requested=@me",
            "--state=open",
            "--json",
            "repository",
            "--limit",
            "100",
        ],
        "Could not list review requests",
    )?;
    Ok(count_review_requests(&json))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_the_repository_list() {
        let repos = parse_repos(&json!([
            {"full_name": "pingu-codes/pingex", "private": true, "fork": false,
             "default_branch": "main", "clone_url": "https://github.com/pingu-codes/pingex.git",
             "ssh_url": "git@github.com:pingu-codes/pingex.git"},
            {"full_name": "pingu-codes/codex", "fork": true,
             "parent": {"full_name": "openai/codex"}, "default_branch": "main"},
            {"name": "no-slug"}
        ]));
        assert_eq!(repos.len(), 2);
        assert!(repos[0].private && !repos[0].fork);
        assert_eq!(repos[0].ssh_url, "git@github.com:pingu-codes/pingex.git");
        assert_eq!(repos[1].parent.as_deref(), Some("openai/codex"));
    }

    #[test]
    fn counts_review_requests_per_repository() {
        let requests = count_review_requests(&json!([
            {"repository": {"name": "a", "nameWithOwner": "o/a"}},
            {"repository": {"name": "b", "nameWithOwner": "o/b"}},
            {"repository": {"name": "b", "nameWithOwner": "o/b"}},
            {"repository": {"name": "c", "nameWithOwner": "o/c"}}
        ]));
        let found: Vec<(&str, i64)> = requests
            .iter()
            .map(|request| (request.slug.as_str(), request.count))
            .collect();
        assert_eq!(found, [("o/b", 2), ("o/a", 1), ("o/c", 1)]);
    }
}
