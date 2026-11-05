//! The signed-in account's repositories, and the ones asking it for review,
//! for Add repository.

use std::collections::HashMap;
use std::path::Path;

use serde_json::Value;

use crate::review::types::{GitHubAccount, GitHubRepo, ReviewRequests};
use crate::util::host::Host;

use super::parse::str_field;
use super::run::{redact_gh_error, run_gh, READ_TIMEOUT, WRITE_TIMEOUT};

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

/// Parse `gh auth status` for github.com: one `Logged in to github.com
/// account <login> (<source>)` line per account (older gh: `as <login>`),
/// each followed by `- Active account: true|false` on gh versions with
/// several accounts. A token source given as a path shows as its file name.
pub(crate) fn parse_auth_status(text: &str) -> GitHubAccount {
    let mut logins: Vec<(String, Option<String>, bool)> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.split("Logged in to github.com ").nth(1) {
            let rest = rest
                .strip_prefix("account ")
                .or_else(|| rest.strip_prefix("as "))
                .unwrap_or(rest);
            let (login, source) = match rest.split_once(" (") {
                Some((login, source)) => (login, Some(source.trim_end_matches(')'))),
                None => (rest, None),
            };
            let source = source.map(|source| {
                source
                    .rsplit(['/', '\\'])
                    .next()
                    .unwrap_or(source)
                    .to_string()
            });
            logins.push((login.trim().to_string(), source, false));
        } else if line.contains("Active account: true") {
            if let Some(last) = logins.last_mut() {
                last.2 = true;
            }
        }
    }
    let active = logins
        .iter()
        .find(|(_, _, active)| *active)
        .or_else(|| logins.first());
    GitHubAccount {
        login: active.map(|(login, _, _)| login.clone()),
        token_source: active.and_then(|(_, source, _)| source.clone()),
        logins: logins.iter().map(|(login, _, _)| login.clone()).collect(),
    }
}

pub(crate) fn account(host: &Host, dir: &Path) -> Result<GitHubAccount, String> {
    let output = run_gh(
        host,
        dir,
        &["auth", "status", "--hostname", "github.com"],
        READ_TIMEOUT,
    )?;
    // gh writes the report to stdout or stderr depending on its version, and
    // exits non-zero when any account has a problem: read both, always.
    Ok(parse_auth_status(&format!(
        "{}\n{}",
        output.stdout, output.stderr
    )))
}

pub(crate) fn switch_account(host: &Host, dir: &Path, login: &str) -> Result<(), String> {
    let valid = !login.is_empty()
        && !login.starts_with('-')
        && login
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-');
    if !valid {
        return Err("That is not a GitHub login".to_string());
    }
    let output = run_gh(
        host,
        dir,
        &[
            "auth",
            "switch",
            "--hostname",
            "github.com",
            "--user",
            login,
        ],
        WRITE_TIMEOUT,
    )?;
    if !output.ok {
        return Err(redact_gh_error(
            "Could not switch the GitHub account",
            &output,
        ));
    }
    Ok(())
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

    #[test]
    fn reads_the_active_account_and_its_token_source() {
        let status = "github.com\n  \u{2713} Logged in to github.com account pingu-codes (keyring)\n  - Active account: false\n  - Git operations protocol: https\n\n  \u{2713} Logged in to github.com account work-login (GH_TOKEN)\n  - Active account: true\n";
        let account = parse_auth_status(status);
        assert_eq!(account.login.as_deref(), Some("work-login"));
        assert_eq!(account.token_source.as_deref(), Some("GH_TOKEN"));
        assert_eq!(account.logins, ["pingu-codes", "work-login"]);
    }

    #[test]
    fn reads_an_older_gh_and_a_signed_out_one() {
        let older = "github.com\n  \u{2713} Logged in to github.com as pingu-codes (/Users/me/.config/gh/hosts.yml)\n";
        let account = parse_auth_status(older);
        assert_eq!(account.login.as_deref(), Some("pingu-codes"));
        assert_eq!(account.token_source.as_deref(), Some("hosts.yml"));

        let signed_out =
            "You are not logged into any GitHub hosts. To log in, run: gh auth login\n";
        assert_eq!(parse_auth_status(signed_out), GitHubAccount::default());
    }
}
