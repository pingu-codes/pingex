//! A repository's PR inbox: one paginated GraphQL query for every open PR and
//! the review, check and thread state the inbox shows.
//!
//! The parse is a pure function over JSON, tested against a recorded
//! fixture; only `fetch_inbox` touches the network.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use serde_json::Value;

use super::parse::str_field;
use super::run::{gh_graphql, repo_name_with_owner, GqlVar};
use crate::review::types::{InboxPr, InboxRead, InboxReview, RateLimit};
use crate::util::host::Host;

/// Paginated on `pullRequests` (see `gh_graphql`). The inner connections are
/// capped rather than paginated: 20 reviewers and requests, and 100 threads
/// per PR for the unresolved count.
///
/// 25 PRs a page: measured on cli/cli (Nov 2026), 25 took about 4s and 50
/// about 7s, against GitHub's ~10s limit. `mergeStateStatus` is left out on
/// purpose: GitHub computes mergeability per PR and it pushed 50 PRs past the
/// limit (502). "Behind the base" is counted locally from the fetched refs.
pub(crate) const INBOX_QUERY: &str = "query($owner:String!,$repo:String!,$endCursor:String){\
  viewer{login}\
  rateLimit{cost remaining resetAt}\
  repository(owner:$owner,name:$repo){\
    pullRequests(states:OPEN,first:25,after:$endCursor,orderBy:{field:UPDATED_AT,direction:DESC}){\
      pageInfo{hasNextPage endCursor}\
      nodes{\
        number title url isDraft createdAt updatedAt author{login}\
        headRefName headRefOid baseRefName baseRefOid headRepository{nameWithOwner}\
        maintainerCanModify reviewDecision additions deletions changedFiles\
        reviewRequests(first:20){nodes{requestedReviewer{\
          ... on User{login} ... on Bot{login} ... on Mannequin{login} ... on Team{slug}\
        }}}\
        latestOpinionatedReviews(first:20){nodes{state author{login}}}\
        commits(last:1){nodes{commit{statusCheckRollup{state}}}}\
        reviewThreads(first:100){nodes{isResolved}}\
      }\
    }\
  }\
}";

/// At most 200 open PRs per repository.
const MAX_INBOX_PAGES: usize = 8;

/// `owner/name` per repository folder, since `gh repo view` is itself a
/// network call and a folder's GitHub repository does not change while the
/// app runs.
fn slug_cache() -> &'static Mutex<HashMap<String, (String, String)>> {
    static CACHE: OnceLock<Mutex<HashMap<String, (String, String)>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

pub(crate) fn repo_slug(host: &Host, dir: &Path) -> Result<(String, String), String> {
    let key = format!("{host:?}:{}", host.path_string(dir));
    if let Some(slug) = slug_cache()
        .lock()
        .ok()
        .and_then(|cache| cache.get(&key).cloned())
    {
        return Ok(slug);
    }
    let slug = repo_name_with_owner(host, dir)?;
    if let Ok(mut cache) = slug_cache().lock() {
        cache.insert(key, slug.clone());
    }
    Ok(slug)
}

pub(crate) fn fetch_inbox(host: &Host, dir: &Path) -> Result<InboxRead, String> {
    let (owner, name) = repo_slug(host, dir)?;
    let vars = [("owner", GqlVar::Str(&owner)), ("repo", GqlVar::Str(&name))];
    let pages = gh_graphql(host, dir, INBOX_QUERY, &vars, MAX_INBOX_PAGES)?;
    let mut read = parse_inbox(&pages.pages);
    read.repo = format!("{owner}/{name}");
    read.complete = pages.complete;
    Ok(read)
}

fn opt_str(value: &Value, pointer: &str) -> Option<String> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

fn int(value: &Value, key: &str) -> i64 {
    value.get(key).and_then(Value::as_i64).unwrap_or(0)
}

fn nodes<'a>(value: &'a Value, pointer: &str) -> impl Iterator<Item = &'a Value> {
    value
        .pointer(pointer)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
}

fn parse_pr(node: &Value) -> InboxPr {
    InboxPr {
        number: int(node, "number"),
        title: str_field(node, "title"),
        author: opt_str(node, "/author/login").unwrap_or_else(|| "ghost".to_string()),
        url: str_field(node, "url"),
        is_draft: node
            .get("isDraft")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        created_at: str_field(node, "createdAt"),
        updated_at: str_field(node, "updatedAt"),
        head_ref: str_field(node, "headRefName"),
        head_sha: str_field(node, "headRefOid"),
        base_ref: str_field(node, "baseRefName"),
        base_sha: str_field(node, "baseRefOid"),
        head_repo: opt_str(node, "/headRepository/nameWithOwner"),
        maintainer_can_modify: node
            .get("maintainerCanModify")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        review_decision: opt_str(node, "/reviewDecision"),
        review_requests: nodes(node, "/reviewRequests/nodes")
            .filter_map(|request| {
                opt_str(request, "/requestedReviewer/login")
                    .or_else(|| opt_str(request, "/requestedReviewer/slug"))
            })
            .collect(),
        reviews: nodes(node, "/latestOpinionatedReviews/nodes")
            .map(|review| InboxReview {
                author: opt_str(review, "/author/login").unwrap_or_else(|| "ghost".to_string()),
                state: str_field(review, "state"),
            })
            .collect(),
        checks: opt_str(node, "/commits/nodes/0/commit/statusCheckRollup/state"),
        unresolved_threads: nodes(node, "/reviewThreads/nodes")
            .filter(|thread| thread.get("isResolved").and_then(Value::as_bool) == Some(false))
            .count() as i64,
        additions: int(node, "additions"),
        deletions: int(node, "deletions"),
        changed_files: int(node, "changedFiles"),
    }
}

/// Parse the pages `gh_graphql` returned for `INBOX_QUERY`.
pub(crate) fn parse_inbox(pages: &[Value]) -> InboxRead {
    let mut read = InboxRead::default();
    for page in pages {
        if let Some(login) = opt_str(page, "/data/viewer/login") {
            read.viewer = login;
        }
        if let Some(limit) = page.pointer("/data/rateLimit") {
            // Each page reports its own cost; the last page's remaining count
            // is the current one.
            let previous = read.rate_limit.as_ref().map_or(0, |limit| limit.cost);
            read.rate_limit = Some(RateLimit {
                cost: previous + int(limit, "cost"),
                remaining: int(limit, "remaining"),
                reset_at: str_field(limit, "resetAt"),
            });
        }
        read.prs
            .extend(nodes(page, "/data/repository/pullRequests/nodes").map(parse_pr));
    }
    read
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Vec<Value> {
        let text = include_str!("../../../../tests/fixtures/github/inbox.json");
        serde_json::from_str(text).expect("fixture is JSON")
    }

    #[test]
    fn parses_every_page_of_the_inbox() {
        let read = parse_inbox(&fixture());
        assert_eq!(read.viewer, "pingu-codes");
        assert_eq!(read.prs.len(), 3, "both pages");
        let rate = read.rate_limit.expect("rate limit");
        assert_eq!(rate.cost, 2, "costs add up across pages");
        assert_eq!(rate.remaining, 4998);

        let first = &read.prs[0];
        assert_eq!(first.number, 415);
        assert_eq!(first.head_sha.len(), 40);
        assert_eq!(first.review_decision.as_deref(), Some("CHANGES_REQUESTED"));
        assert_eq!(first.review_requests, vec!["jo-r", "core-team"]);
        assert_eq!(first.reviews[0].state, "CHANGES_REQUESTED");
        assert_eq!(first.checks.as_deref(), Some("SUCCESS"));
        assert_eq!(first.unresolved_threads, 2);

        let fork = &read.prs[1];
        assert_eq!(fork.head_repo.as_deref(), Some("jo-r/pingex"));
        assert!(fork.maintainer_can_modify);
        assert_eq!(fork.checks, None, "no checks");

        let ghost = &read.prs[2];
        assert_eq!(ghost.author, "ghost", "a deleted account");
        assert_eq!(ghost.head_repo, None, "a deleted fork");
        assert!(ghost.is_draft);
    }
}
