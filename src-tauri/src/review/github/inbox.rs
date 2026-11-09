//! A repository's PR inbox: one paginated GraphQL query for every open PR and
//! the review, check and thread state the inbox shows.
//!
//! The parse is a pure function over JSON, tested against a recorded
//! fixture; only `fetch_inbox` touches the network.

use std::collections::BTreeMap;
use std::sync::LazyLock;
use std::time::Duration;

use serde_json::Value;

use super::parse::str_field;
use super::run::{gh_graphql, repo_name_with_owner, Gh, GqlVar};
use crate::review::types::{GitHubPerson, InboxPr, InboxRead, InboxReview, RateLimit};
use crate::util::host::Host;
use crate::util::ttl_cache::TtlCache;

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
        number title url isDraft createdAt updatedAt author{login avatarUrl(size:40) ... on User{name}}\
        headRefName headRefOid baseRefName baseRefOid headRepository{nameWithOwner}\
        maintainerCanModify reviewDecision additions deletions changedFiles \
        reviewRequests(first:20){nodes{requestedReviewer{\
          ... on User{login name avatarUrl(size:40)} ... on Bot{login avatarUrl(size:40)} \
          ... on Mannequin{login avatarUrl(size:40)} ... on Team{slug name avatarUrl(size:40)}\
        }}}\
        latestOpinionatedReviews(first:20){nodes{state author{login avatarUrl(size:40) ... on User{name}} commit{oid}}}\
        commits(last:1){nodes{commit{statusCheckRollup{state} author{user{login name avatarUrl(size:40)}}}}}\
        reviewThreads(first:100){nodes{isResolved}}\
      }\
    }\
  }\
}";

/// At most 200 open PRs per repository.
const MAX_INBOX_PAGES: usize = 8;

/// `owner/name` per repository folder, since `gh repo view` is itself a
/// network call. A folder's GitHub repository rarely changes, but a renamed
/// remote should show up within the hour rather than never.
static SLUGS: LazyLock<TtlCache<(Host, String), (String, String)>> =
    LazyLock::new(|| TtlCache::new(Duration::from_secs(60 * 60), 256));

pub(crate) fn repo_slug(gh: &Gh<'_>) -> Result<(String, String), String> {
    SLUGS.get_or_try((gh.host.clone(), gh.host.path_string(gh.dir)), || {
        repo_name_with_owner(gh)
    })
}

pub(crate) fn fetch_inbox(gh: &Gh<'_>) -> Result<InboxRead, String> {
    let (owner, name) = repo_slug(gh)?;
    let vars = [("owner", GqlVar::Str(&owner)), ("repo", GqlVar::Str(&name))];
    let pages = gh_graphql(gh, INBOX_QUERY, &vars, MAX_INBOX_PAGES)?;
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

/// A commit's `user.login` names a bot `dependabot[bot]`; everywhere else
/// GitHub's GraphQL API calls it `dependabot`.
fn actor_login(login: &str) -> &str {
    login.strip_suffix("[bot]").unwrap_or(login)
}

/// The account a GraphQL actor names, or the team a review request names.
fn parse_person(actor: &Value) -> Option<GitHubPerson> {
    let (login, is_team) = match opt_str(actor, "/login") {
        Some(login) => (actor_login(&login).to_string(), false),
        None => (opt_str(actor, "/slug")?, true),
    };
    Some(GitHubPerson {
        login,
        name: opt_str(actor, "/name"),
        avatar_url: opt_str(actor, "/avatarUrl"),
        is_team,
    })
}

/// Everyone one PR node names.
fn pr_actors(node: &Value) -> impl Iterator<Item = &Value> {
    let author = node.get("author").into_iter();
    let reviewers =
        nodes(node, "/latestOpinionatedReviews/nodes").filter_map(|review| review.get("author"));
    let requested =
        nodes(node, "/reviewRequests/nodes").filter_map(|request| request.get("requestedReviewer"));
    let committer = node
        .pointer("/commits/nodes/0/commit/author/user")
        .into_iter();
    author.chain(reviewers).chain(requested).chain(committer)
}

/// One entry per login, sorted. A team slug can equal a user's login, and
/// `review_requests` holds them as plain strings; the user wins.
fn collect_people<'a>(actors: impl Iterator<Item = &'a Value>) -> Vec<GitHubPerson> {
    let mut people: BTreeMap<String, GitHubPerson> = BTreeMap::new();
    for person in actors.filter_map(parse_person) {
        match people.get(&person.login) {
            Some(known) if !known.is_team || person.is_team => {}
            _ => {
                people.insert(person.login.clone(), person);
            }
        }
    }
    people.into_values().collect()
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
                commit: opt_str(review, "/commit/oid"),
            })
            .collect(),
        checks: opt_str(node, "/commits/nodes/0/commit/statusCheckRollup/state"),
        head_author: opt_str(node, "/commits/nodes/0/commit/author/user/login")
            .map(|login| actor_login(&login).to_string()),
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
    let mut read = InboxRead {
        people: collect_people(
            pages
                .iter()
                .flat_map(|page| nodes(page, "/data/repository/pullRequests/nodes"))
                .flat_map(pr_actors),
        ),
        ..InboxRead::default()
    };
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
    use serde_json::json;

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
        assert_eq!(first.head_author.as_deref(), Some("pingu-codes"));

        let fork = &read.prs[1];
        assert_eq!(fork.head_repo.as_deref(), Some("jo-r/pingex"));
        assert!(fork.maintainer_can_modify);
        assert_eq!(fork.checks, None, "no checks");

        let ghost = &read.prs[2];
        assert_eq!(ghost.author, "ghost", "a deleted account");
        assert_eq!(ghost.head_repo, None, "a deleted fork");
        assert!(ghost.is_draft);
        assert_eq!(ghost.head_author, None, "no commits");
    }

    #[test]
    fn names_a_bot_the_same_way_on_a_commit_and_a_pr() {
        let read = parse_inbox(&fixture());
        assert_eq!(read.prs[1].head_author.as_deref(), Some("dependabot"));
        let logins: Vec<&str> = read.people.iter().map(|p| p.login.as_str()).collect();
        assert_eq!(logins.iter().filter(|l| **l == "dependabot").count(), 1);
        assert!(!logins.iter().any(|login| login.ends_with("[bot]")));
    }

    #[test]
    fn collects_everyone_a_pr_names_once_across_pages() {
        let people = parse_inbox(&fixture()).people;
        let logins: Vec<&str> = people.iter().map(|p| p.login.as_str()).collect();
        assert_eq!(
            logins,
            ["core-team", "dependabot", "jo-r", "mira-k", "pingu-codes"]
        );
        let person = |login: &str| people.iter().find(|p| p.login == login).expect(login);

        let mira = person("mira-k");
        assert_eq!(mira.name.as_deref(), Some("Mira Kovac"));
        assert!(mira
            .avatar_url
            .as_deref()
            .is_some_and(|url| url.contains("/u/")));
        assert!(!mira.is_team);

        assert!(person("core-team").is_team);
        assert_eq!(person("dependabot").name, None);
        assert!(
            !logins.contains(&"ghost"),
            "a deleted author is not a person"
        );
    }

    #[test]
    fn a_user_wins_over_a_team_with_the_same_name() {
        for actors in [
            [
                json!({"slug": "core", "name": "Core"}),
                json!({"login": "core", "name": "A user"}),
            ],
            [
                json!({"login": "core", "name": "A user"}),
                json!({"slug": "core", "name": "Core"}),
            ],
        ] {
            let people = collect_people(actors.iter());
            assert_eq!(people.len(), 1);
            assert_eq!(people[0].name.as_deref(), Some("A user"));
            assert!(!people[0].is_team);
        }
    }

    #[test]
    fn a_snapshot_stored_before_head_author_still_parses() {
        let stored = serde_json::to_value(vec![InboxPr::default()]).expect("serializes");
        let mut stored = stored.as_array().expect("array").clone();
        stored[0]
            .as_object_mut()
            .expect("object")
            .remove("headAuthor");
        let parsed: Vec<InboxPr> =
            serde_json::from_value(Value::Array(stored)).expect("old snapshot parses");
        assert_eq!(parsed[0].head_author, None);
    }
}
