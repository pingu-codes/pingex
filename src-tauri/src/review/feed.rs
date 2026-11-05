//! "Since you last looked": what changed on a repository's PRs between two
//! inbox snapshots. It costs no extra requests and needs no notification
//! permission.

use std::collections::{HashMap, HashSet};

use super::types::{InboxEvent, InboxPr};

fn failing(checks: &Option<String>) -> bool {
    matches!(checks.as_deref(), Some("FAILURE" | "ERROR"))
}

/// What changed between two inbox snapshots, newest PR first as in `next`.
/// `viewer` is the signed-in login, for "your review was requested".
pub(crate) fn diff_snapshots(
    previous: &[InboxPr],
    next: &[InboxPr],
    viewer: &str,
) -> Vec<InboxEvent> {
    let event = |number: i64, kind: &str, actor: Option<&str>, state: Option<&str>| InboxEvent {
        number,
        kind: kind.to_string(),
        actor: actor.map(str::to_string),
        state: state.map(str::to_string),
    };
    let before: HashMap<i64, &InboxPr> = previous.iter().map(|pr| (pr.number, pr)).collect();
    let mut events = Vec::new();
    for pr in next {
        let Some(old) = before.get(&pr.number) else {
            events.push(event(pr.number, "opened", Some(&pr.author), None));
            continue;
        };
        if old.head_sha != pr.head_sha {
            events.push(event(pr.number, "pushed", Some(&pr.author), None));
        }
        let old_reviews: HashMap<&str, &str> = old
            .reviews
            .iter()
            .map(|review| (review.author.as_str(), review.state.as_str()))
            .collect();
        for review in &pr.reviews {
            if old_reviews.get(review.author.as_str()) != Some(&review.state.as_str()) {
                events.push(event(
                    pr.number,
                    "reviewed",
                    Some(&review.author),
                    Some(&review.state),
                ));
            }
        }
        let was_requested = old.review_requests.iter().any(|login| login == viewer);
        if !was_requested && pr.review_requests.iter().any(|login| login == viewer) {
            events.push(event(pr.number, "reviewRequested", None, None));
        }
        if failing(&pr.checks) && !failing(&old.checks) {
            events.push(event(pr.number, "checksFailed", None, None));
        } else if pr.checks.as_deref() == Some("SUCCESS") && failing(&old.checks) {
            events.push(event(pr.number, "checksPassed", None, None));
        }
    }
    let open: HashSet<i64> = next.iter().map(|pr| pr.number).collect();
    events.extend(
        previous
            .iter()
            .filter(|pr| !open.contains(&pr.number))
            .map(|pr| event(pr.number, "closed", None, None)),
    );
    events
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::github::inbox::parse_inbox;
    use serde_json::Value;

    fn fixture() -> Vec<Value> {
        let text = include_str!("../../../tests/fixtures/github/inbox.json");
        serde_json::from_str(text).expect("fixture is JSON")
    }

    #[test]
    fn derives_the_feed_from_two_snapshots() {
        let previous = parse_inbox(&fixture()).prs;
        let mut next = previous.clone();
        next[0].head_sha = "f".repeat(40);
        next[0].reviews[0].state = "APPROVED".into();
        next[1].review_requests.push("pingu-codes".into());
        next[1].checks = Some("FAILURE".into());
        next.remove(2);
        let mut opened = InboxPr {
            number: 430,
            author: "tomasz".into(),
            ..InboxPr::default()
        };
        opened.head_sha = "e".repeat(40);
        next.insert(0, opened);

        let events = diff_snapshots(&previous, &next, "pingu-codes");
        let kinds: Vec<(i64, &str, Option<&str>)> = events
            .iter()
            .map(|event| (event.number, event.kind.as_str(), event.actor.as_deref()))
            .collect();
        assert_eq!(
            kinds,
            vec![
                (430, "opened", Some("tomasz")),
                (415, "pushed", Some("pingu-codes")),
                (415, "reviewed", Some("mira-k")),
                (409, "reviewRequested", None),
                (409, "checksFailed", None),
                (399, "closed", None),
            ]
        );
        assert_eq!(events[2].state.as_deref(), Some("APPROVED"));
        assert!(diff_snapshots(&previous, &previous, "pingu-codes").is_empty());
    }
}
