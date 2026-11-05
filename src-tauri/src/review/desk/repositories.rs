//! The signed-in account's repositories on GitHub, for Add repository. These
//! answer for the account, whichever repository the desk was made for.

use super::ReviewDesk;
use crate::review::types::{GitHubRepo, ReviewRequests};

impl ReviewDesk {
    pub(crate) async fn my_repositories(&self) -> Result<Vec<GitHubRepo>, String> {
        self.blocking("Listing repositories failed", |desk| {
            desk.github.my_repositories()
        })
        .await
    }

    /// Repositories with PRs asking the account for review, most first.
    pub(crate) async fn repositories_requesting_review(
        &self,
    ) -> Result<Vec<ReviewRequests>, String> {
        self.blocking("Listing review requests failed", |desk| {
            desk.github.repositories_requesting_review()
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::fixture;
    use crate::review::types::{GitHubRepo, ReviewRequests};

    #[tokio::test]
    async fn lists_the_accounts_repositories_and_review_requests() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let repo = GitHubRepo {
            slug: "pingu-codes/pingex".into(),
            default_branch: "main".into(),
            ..GitHubRepo::default()
        };
        let requests = ReviewRequests {
            slug: "jo-r/focus".into(),
            count: 2,
        };
        fixture.github.put_repositories(vec![repo.clone()]);
        fixture.github.put_review_requests(vec![requests.clone()]);

        assert_eq!(fixture.desk.my_repositories().await.unwrap(), [repo]);
        assert_eq!(
            fixture.desk.repositories_requesting_review().await.unwrap(),
            [requests]
        );
    }
}
