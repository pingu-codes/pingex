//! Pushing a PR worktree back to the PR. A managed worktree's `pr/<n>` branch
//! has no upstream, so this is the way its commits reach the PR's head branch,
//! which may be in a fork. It never forces: if the branch moved, the push is
//! rejected and the user decides.

use std::path::Path;

use super::ReviewDesk;
use crate::git::run::{redact_git_error, run_git_network};
use crate::git::{checkouts as git, objects};
use crate::storage;

impl ReviewDesk {
    /// Push the current branch of PR `number`'s worktree to the PR's head
    /// branch. What may be pushed is read from GitHub now, not from what the
    /// page last saw.
    pub(crate) async fn push_to_pr(&self, number: i64) -> Result<(), String> {
        let key = self.repo_key();
        let stored = storage::read_checkout(&self.database, &key, number)
            .await?
            .ok_or_else(|| "This pull request has no worktree".to_string())?;
        let path = stored.path;
        let pushed = self
            .blocking("Pushing failed", move |desk| {
                desk.push_worktree(number, &path)
            })
            .await?;
        // What this worktree holds is on the PR now, whatever the page knows.
        storage::update_checkout_head(&self.database, &key, number, &pushed).await
    }

    /// Returns the commit pushed.
    fn push_worktree(&self, number: i64, path: &str) -> Result<String, String> {
        let dir = Path::new(path);
        let summary = self.github.pr_detail(number)?.summary;
        let head_repo = summary
            .head_repo
            .ok_or_else(|| "This pull request's head repository is gone".to_string())?;
        let base_repo = self.github.repo()?;
        if !head_repo.eq_ignore_ascii_case(&base_repo)
            && !summary.maintainer_can_modify
            && summary.author != self.github.inbox()?.viewer
        {
            return Err(format!(
                "{} hasn't allowed maintainers to edit this pull request, so its branch can't be pushed to",
                summary.author
            ));
        }
        if git::branch_of(&self.host, dir)?.is_none() {
            return Err("Check out a branch before pushing".to_string());
        }
        let head = git::head_of(&self.host, dir)?
            .ok_or_else(|| "This worktree has no commits to push".to_string())?;
        let remote = objects::remote_for_slug(&self.host, dir, &head_repo)?
            .unwrap_or_else(|| format!("https://github.com/{head_repo}.git"));
        let refspec = format!("HEAD:refs/heads/{}", summary.head_ref);
        let output = run_git_network(&self.host, dir, &["push", &remote, &refspec])?;
        if output.ok {
            return Ok(head);
        }
        let stderr = output.stderr.to_lowercase();
        if stderr.contains("rejected") || stderr.contains("fetch first") {
            return Err(
                "The pull request's branch has moved. Bring in its new commits, then push again"
                    .to_string(),
            );
        }
        Err(redact_git_error(
            "Could not push to the pull request",
            &output,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{fixture, Fixture, SLUG};
    use crate::review::checkout_types::CheckoutTarget;
    use crate::review::types::{InboxPr, PrDetail, PrSummary};

    fn target(pr: &InboxPr) -> CheckoutTarget {
        CheckoutTarget {
            number: pr.number,
            head_ref: pr.head_ref.clone(),
            head_sha: pr.head_sha.clone(),
            head_repo: Some(SLUG.to_string()),
            ..CheckoutTarget::default()
        }
    }

    /// PR `pr` on the fake GitHub, with its head in `head_repo`.
    fn put_detail(fixture: &Fixture, pr: &InboxPr, head_repo: Option<&str>, can_modify: bool) {
        fixture.github.put_detail(
            pr.number,
            PrDetail {
                summary: PrSummary {
                    number: pr.number,
                    author: pr.author.clone(),
                    head_ref: pr.head_ref.clone(),
                    head_repo: head_repo.map(str::to_string),
                    maintainer_can_modify: can_modify,
                    ..PrSummary::default()
                },
                head_sha: pr.head_sha.clone(),
                ..PrDetail::default()
            },
        );
    }

    /// A worktree for `pr` holding one new commit: its path and that commit.
    async fn worktree_with_a_commit(fixture: &Fixture, pr: &InboxPr) -> (String, String) {
        let path = fixture
            .desk
            .ensure_pr_worktree(target(pr))
            .await
            .unwrap()
            .path
            .unwrap();
        let dir = std::path::Path::new(&path);
        std::fs::write(dir.join("fix.rs"), "fixed").unwrap();
        fixture.repo.git_in(dir, &["add", "fix.rs"]);
        fixture.repo.git_in(dir, &["commit", "-q", "-m", "fix"]);
        let commit = fixture.repo.git_in(dir, &["rev-parse", "HEAD"]);
        (path, commit)
    }

    fn remote_branch(fixture: &Fixture, path: &str, branch: &str) -> String {
        let reference = format!("refs/heads/{branch}");
        fixture.repo.git_in(
            std::path::Path::new(path),
            &["ls-remote", "origin", &reference],
        )
    }

    #[tokio::test]
    async fn pushing_updates_the_prs_head_branch_and_leaves_no_stray_branch() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(50, "feature", "src/lib.rs");
        put_detail(&fixture, &pr, Some(SLUG), false);
        let (path, commit) = worktree_with_a_commit(&fixture, &pr).await;

        fixture.desk.push_to_pr(50).await.unwrap();

        assert!(remote_branch(&fixture, &path, "feature").starts_with(&commit));
        assert_eq!(remote_branch(&fixture, &path, "pr/50"), "");
        // Pushed work is safe, so nothing holds the worktree back any more.
        fixture.desk.remove_pr_worktree(50).await.unwrap();
    }

    #[tokio::test]
    async fn a_fork_that_allows_maintainer_edits_is_pushed_to_directly() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(51, "feature", "src/lib.rs");
        put_detail(&fixture, &pr, Some("jo-r/desk"), true);
        let (path, commit) = worktree_with_a_commit(&fixture, &pr).await;
        // The fork's URL reaches the same bare repository in this fixture.
        let bare = fixture
            .repo
            .git(&["config", "--get-regexp", r"^url\..*\.insteadof$"]);
        let bare = bare
            .split_whitespace()
            .next()
            .unwrap()
            .trim_start_matches("url.")
            .trim_end_matches(".insteadof")
            .to_string();
        fixture.repo.git(&[
            "config",
            "--add",
            &format!("url.{bare}.insteadOf"),
            "https://github.com/jo-r/desk.git",
        ]);

        fixture.desk.push_to_pr(51).await.unwrap();

        assert!(remote_branch(&fixture, &path, "feature").starts_with(&commit));
    }

    #[tokio::test]
    async fn a_fork_that_refuses_maintainer_edits_is_not_pushed_to() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(52, "feature", "src/lib.rs");
        put_detail(&fixture, &pr, Some("jo-r/desk"), false);
        let (path, _) = worktree_with_a_commit(&fixture, &pr).await;
        let before = remote_branch(&fixture, &path, "feature");

        let error = fixture.desk.push_to_pr(52).await.unwrap_err();

        assert!(error.contains("hasn't allowed maintainers"), "{error}");
        assert_eq!(remote_branch(&fixture, &path, "feature"), before);
    }

    #[tokio::test]
    async fn a_pr_whose_head_repository_is_gone_says_so() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(53, "feature", "src/lib.rs");
        put_detail(&fixture, &pr, None, true);
        worktree_with_a_commit(&fixture, &pr).await;

        let error = fixture.desk.push_to_pr(53).await.unwrap_err();

        assert!(error.contains("gone"), "{error}");
    }

    #[tokio::test]
    async fn a_moved_branch_is_never_overwritten() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(54, "feature", "src/lib.rs");
        put_detail(&fixture, &pr, Some(SLUG), false);
        let (path, _) = worktree_with_a_commit(&fixture, &pr).await;
        // The author pushes first.
        let dir = fixture.repo.repo.clone();
        fixture.repo.switch("feature");
        let theirs = fixture.repo.commit("src/theirs.rs", "theirs", "theirs");
        fixture
            .repo
            .git_in(&dir, &["push", "-q", "origin", "feature"]);
        fixture.repo.switch("main");

        let error = fixture.desk.push_to_pr(54).await.unwrap_err();

        assert!(error.contains("moved"), "{error}");
        assert!(remote_branch(&fixture, &path, "feature").starts_with(&theirs));
    }
}
