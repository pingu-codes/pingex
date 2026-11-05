//! PRs-only repositories: GitHub repositories added to the review desk with
//! nothing cloned. A cloned repository is a project and needs no row here;
//! cloning or linking a PRs-only repository removes its row.

use turso::{params, Database};

use super::db;
use crate::util::time::unix_secs;

/// Add `slug` (`owner/name`); adding it again keeps its first date.
pub(crate) async fn add_prs_only_repository(database: &Database, slug: &str) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "INSERT INTO review_prs_only_repos(slug, added_at) VALUES (?, ?)
         ON CONFLICT(slug) DO NOTHING",
        params![slug.to_lowercase(), unix_secs()],
    )
    .await
}

/// Every PRs-only repository's slug, in the order they were added.
pub(crate) async fn prs_only_repositories(database: &Database) -> Result<Vec<String>, String> {
    let connection = db::conn(database)?;
    db::rows(
        &connection,
        "SELECT slug FROM review_prs_only_repos ORDER BY added_at, slug",
        (),
        |row| db::text(row, 0),
    )
    .await
}

pub(crate) async fn remove_prs_only_repository(
    database: &Database,
    slug: &str,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "DELETE FROM review_prs_only_repos WHERE slug = ?",
        params![slug.to_lowercase()],
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn adds_lists_and_removes_prs_only_repositories() {
        let dir = tempfile::tempdir().unwrap();
        let database = crate::storage::open(dir.path()).await.unwrap();

        add_prs_only_repository(&database, "tomasz/bun-tools")
            .await
            .unwrap();
        add_prs_only_repository(&database, "Tomasz/Bun-Tools")
            .await
            .unwrap();
        add_prs_only_repository(&database, "openai/codex")
            .await
            .unwrap();
        let mut slugs = prs_only_repositories(&database).await.unwrap();
        slugs.sort();
        assert_eq!(slugs, ["openai/codex", "tomasz/bun-tools"]);

        remove_prs_only_repository(&database, "tomasz/bun-tools")
            .await
            .unwrap();
        assert_eq!(
            prs_only_repositories(&database).await.unwrap(),
            ["openai/codex"]
        );
    }
}
