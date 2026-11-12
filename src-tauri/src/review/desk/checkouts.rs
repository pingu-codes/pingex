//! A PR read with nothing checked out: its commits are fetched into
//! Pingex-owned refs (`refs/pingex/pr/<n>/head`) and read by SHA, and the
//! working tree is never touched. Checking a PR out is `check_out.rs`.

use super::{pr_number, ReviewDesk};
use crate::git::objects;
use crate::review::diff::parse_git_diff;
use crate::review::types::{FetchedPr, PrFileContents, ShaDiff};

impl ReviewDesk {
    /// Fetch PR heads into `refs/pingex/pr/<n>/head`, in one round trip, from
    /// the remote that points at the repository on GitHub.
    pub(crate) async fn fetch_prs(&self, numbers: Vec<i64>) -> Result<Vec<FetchedPr>, String> {
        self.blocking("Fetching pull requests failed", move |desk| {
            let numbers = numbers
                .into_iter()
                .map(pr_number)
                .collect::<Result<Vec<_>, _>>()?;
            let slug = desk.github.repo()?;
            let remote = objects::remote_for_slug(&desk.host, desk.dir(), &slug)?
                .ok_or_else(|| format!("No remote in this repository points at {slug}"))?;
            Ok(
                objects::fetch_pr_heads(&desk.host, desk.dir(), &remote, &numbers)?
                    .into_iter()
                    .map(|fetched| FetchedPr {
                        number: i64::from(fetched.number),
                        head_sha: fetched.sha,
                    })
                    .collect(),
            )
        })
        .await
    }

    /// The PR diff GitHub shows, `head` against its merge base with `base`,
    /// read locally by SHA. Fetch the PR first.
    pub(crate) async fn diff(&self, base_sha: String, head_sha: String) -> Result<ShaDiff, String> {
        self.blocking("Diff failed", move |desk| {
            let patch = objects::diff_shas(&desk.host, desk.dir(), &base_sha, &head_sha)?;
            Ok(ShaDiff {
                files: parse_git_diff(&patch.text),
                truncated: patch.truncated,
            })
        })
        .await
    }

    /// One file as it is at commit `sha`, without checking anything out.
    pub(crate) async fn file(&self, sha: String, path: String) -> Result<PrFileContents, String> {
        self.blocking("Reading the file failed", move |desk| {
            let blob = objects::read_blob(&desk.host, desk.dir(), &sha, &path)?;
            Ok(PrFileContents {
                text: blob.text,
                size: i64::try_from(blob.size).unwrap_or(i64::MAX),
                truncated: blob.truncated,
            })
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::fixture;

    #[tokio::test]
    async fn a_fetched_pr_is_read_by_sha_without_touching_the_working_tree() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(7, "feature", "src/lib.rs");

        let fetched = fixture.desk.fetch_prs(vec![7]).await.unwrap();
        assert_eq!(fetched[0].head_sha.as_deref(), Some(pr.head_sha.as_str()));

        let diff = fixture
            .desk
            .diff(pr.base_sha.clone(), pr.head_sha.clone())
            .await
            .unwrap();
        let paths: Vec<&str> = diff.files.iter().map(|file| file.path.as_str()).collect();
        assert_eq!(paths, ["src/lib.rs"]);
        let file = fixture
            .desk
            .file(pr.head_sha.clone(), "src/lib.rs".into())
            .await
            .unwrap();
        assert_eq!(file.text.as_deref(), Some("pub fn one() {}\n"));

        assert_eq!(fixture.repo.git(&["status", "--porcelain"]), "");
        assert!(!fixture.repo.repo.join("src/lib.rs").exists());
    }

    #[tokio::test]
    async fn a_pr_number_that_is_not_one_is_refused() {
        let Some(fixture) = fixture().await else {
            return;
        };
        assert!(fixture.desk.fetch_prs(vec![0]).await.is_err());
        assert!(fixture.desk.fetch_prs(vec![-4]).await.is_err());
    }
}
