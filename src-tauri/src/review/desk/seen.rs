//! Seen state: the Seen head per PR (a ref, so its commits survive a
//! force-push) and what changed since it.

use super::{pr_number, ReviewDesk};
use crate::git::objects;
use crate::review::diff::parse_git_diff;
use crate::review::types::ChangesSince;

impl ReviewDesk {
    /// What PR `number` changed since its Seen head. `base_sha` is the
    /// current tip of the PR's base git branch, for the range diff after a
    /// rebase.
    pub(crate) async fn changes_since(
        &self,
        number: i64,
        base_sha: String,
        head_sha: String,
    ) -> Result<ChangesSince, String> {
        self.blocking("Reading changes failed", move |desk| {
            let (host, dir) = (&desk.host, desk.dir());
            let number = pr_number(number)?;
            let Some(seen_sha) = objects::seen_head(host, dir, number)? else {
                // Never seen: everything is new.
                let patch = objects::diff_shas(host, dir, &base_sha, &head_sha)?;
                return Ok(ChangesSince {
                    seen_sha: None,
                    head_sha,
                    fast_forward: false,
                    files: parse_git_diff(&patch.text),
                    range_diff: None,
                    truncated: patch.truncated,
                });
            };
            let fast_forward = objects::is_ancestor(host, dir, &seen_sha, &head_sha)?;
            let patch = objects::diff_between(host, dir, &seen_sha, &head_sha)?;
            let range_diff = if fast_forward {
                None
            } else {
                Some(objects::range_diff(host, dir, &base_sha, &seen_sha, &head_sha)?.text)
            };
            Ok(ChangesSince {
                seen_sha: Some(seen_sha),
                head_sha,
                fast_forward,
                files: parse_git_diff(&patch.text),
                range_diff,
                truncated: patch.truncated,
            })
        })
        .await
    }

    /// Pin `sha` as PR `number`'s Seen head.
    pub(crate) async fn mark_seen(&self, number: i64, sha: String) -> Result<(), String> {
        self.blocking("Marking seen failed", move |desk| {
            objects::mark_seen(&desk.host, desk.dir(), pr_number(number)?, &sha)
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::fixture;

    fn paths(changes: &crate::review::types::ChangesSince) -> Vec<&str> {
        changes
            .files
            .iter()
            .map(|file| file.path.as_str())
            .collect()
    }

    #[tokio::test]
    async fn changes_since_cover_only_what_was_pushed_after_the_seen_head() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(9, "feature", "src/one.rs");
        fixture.desk.fetch_prs(vec![9]).await.unwrap();

        let unseen = fixture
            .desk
            .changes_since(9, pr.base_sha.clone(), pr.head_sha.clone())
            .await
            .unwrap();
        assert_eq!(unseen.seen_sha, None);
        assert_eq!(paths(&unseen), ["src/one.rs"]);

        fixture
            .desk
            .mark_seen(9, pr.head_sha.clone())
            .await
            .unwrap();
        let head = fixture.push_to_pr(9, "feature", "src/two.rs");
        fixture.desk.fetch_prs(vec![9]).await.unwrap();

        let since = fixture
            .desk
            .changes_since(9, pr.base_sha.clone(), head)
            .await
            .unwrap();
        assert_eq!(since.seen_sha.as_deref(), Some(pr.head_sha.as_str()));
        assert!(since.fast_forward);
        assert_eq!(paths(&since), ["src/two.rs"]);
    }
}
