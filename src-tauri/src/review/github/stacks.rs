//! Stacked pull requests as GitHub records them (native stacks, the ones
//! `gh stack` and GitHub's stack UI create), read with
//! `GET repos/{owner}/{repo}/stacks`. No extension is needed to read them.
//!
//! Observed on a repository with stacks (Nov 2026): `pull_requests` runs
//! bottom first; a merged PR stays in the list (`merged_at` set) and the PR
//! above it is retargeted onto the stack's base. A repository without the
//! feature answers 404, which is cached for an hour.

use std::sync::LazyLock;
use std::time::Duration;

use serde_json::Value;

use super::parse::str_field;
use super::reads::merge_paginated_arrays;
use super::run::{redact_gh_error, run_gh, Gh, READ_TIMEOUT};
use crate::review::types::{PrStack, StackListing, StackPr};
use crate::util::ttl_cache::TtlCache;

/// Repositories that answered 404 for stacks in the last hour, by login and
/// `owner/name`, so a repository that gains the feature is asked again. The
/// login is in the key because GitHub also answers 404 to an account that
/// can't see the repository.
static UNSUPPORTED: LazyLock<TtlCache<(String, String), ()>> =
    LazyLock::new(|| TtlCache::new(Duration::from_secs(60 * 60), 256));

pub(crate) fn list_stacks(gh: &Gh<'_>, slug: &str) -> Result<StackListing, String> {
    let key = (gh.login.to_string(), slug.to_string());
    if UNSUPPORTED.get(&key).is_some() {
        return Ok(StackListing::default());
    }
    let endpoint = format!("repos/{slug}/stacks");
    let output = run_gh(gh, &["api", "--paginate", &endpoint], READ_TIMEOUT)?;
    if !output.ok {
        if output.stderr.contains("HTTP 404") {
            UNSUPPORTED.insert(key, ());
            return Ok(StackListing::default());
        }
        return Err(redact_gh_error(
            "Could not read the repository's stacks",
            &output,
        ));
    }
    Ok(StackListing {
        supported: true,
        stacks: parse_stacks(&merge_paginated_arrays(&output.stdout)),
    })
}

/// Parse the stacks listing. Closed stacks are kept; the caller decides
/// whether to show them.
pub(crate) fn parse_stacks(json: &Value) -> Vec<PrStack> {
    let Some(items) = json.as_array() else {
        return Vec::new();
    };
    items
        .iter()
        .map(|stack| PrStack {
            number: stack.get("number").and_then(Value::as_i64).unwrap_or(0),
            base_ref: stack
                .pointer("/base/ref")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            open: stack.get("open").and_then(Value::as_bool).unwrap_or(false),
            prs: stack
                .get("pull_requests")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|pr| StackPr {
                    number: pr.get("number").and_then(Value::as_i64).unwrap_or(0),
                    state: str_field(pr, "state"),
                    is_draft: pr.get("draft").and_then(Value::as_bool).unwrap_or(false),
                    merged: pr.get("merged_at").is_some_and(|merged| !merged.is_null()),
                    head_ref: pr
                        .pointer("/head/ref")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    head_sha: pr
                        .pointer("/head/sha")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                })
                .collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_native_stacks_bottom_first() {
        let json: Value = serde_json::from_str(include_str!(
            "../../../../tests/fixtures/github/stacks.json"
        ))
        .unwrap();
        let stacks = parse_stacks(&json);
        assert_eq!(stacks.len(), 2);
        let stack = &stacks[0];
        assert_eq!(
            (stack.number, stack.base_ref.as_str(), stack.open),
            (7, "main", true)
        );
        let numbers: Vec<i64> = stack.prs.iter().map(|pr| pr.number).collect();
        assert_eq!(numbers, vec![412, 415, 418]);
        assert!(stack.prs[0].merged && stack.prs[0].state == "closed");
        assert!(!stack.prs[1].merged);
        assert!(stack.prs[2].is_draft);
        assert_eq!(stack.prs[1].head_ref, "feat/claude-driver");
        assert!(!stacks[1].open && stacks[1].prs.is_empty());
    }

    #[test]
    fn an_empty_listing_is_no_stacks() {
        assert!(parse_stacks(&serde_json::json!([])).is_empty());
        assert!(parse_stacks(&serde_json::json!({"message": "x"})).is_empty());
    }
}
