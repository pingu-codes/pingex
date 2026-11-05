//! Per-repository settings: the setup command, review checkout policy,
//! worktree directory and PR source. Keyed by the project's root path as
//! stored for projects. A repository with no row reads as the defaults.

use serde::{Deserialize, Serialize};
use turso::{params, Database};

use super::db;
use crate::util::time::unix_secs;

/// When a review checkout may be created for an action that needs one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ReviewCheckoutPolicy {
    /// Confirm the first time, until "Don't ask again for <repository>".
    #[default]
    Ask,
    /// Create one without asking.
    WhenNeeded,
    /// Never create one.
    Never,
}

impl ReviewCheckoutPolicy {
    fn as_str(self) -> &'static str {
        match self {
            Self::Ask => "ask",
            Self::WhenNeeded => "when_needed",
            Self::Never => "never",
        }
    }

    fn parse(value: &str) -> Self {
        match value {
            "when_needed" => Self::WhenNeeded,
            "never" => Self::Never,
            _ => Self::Ask,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RepositorySettings {
    pub(crate) repo_key: String,
    /// Runs once in a new review checkout, e.g. `bun install`.
    pub(crate) setup_command: Option<String>,
    pub(crate) review_checkout_policy: ReviewCheckoutPolicy,
    /// "Don't ask again for <repository>".
    pub(crate) skip_checkout_confirm: bool,
    /// Where new worktrees go; `None` is the app default.
    pub(crate) worktree_dir: Option<String>,
    /// `origin`, `upstream`, `both` or `none`.
    pub(crate) pr_source: String,
    pub(crate) updated_at: i64,
}

impl RepositorySettings {
    pub(crate) fn defaults(repo_key: &str) -> Self {
        Self {
            repo_key: repo_key.to_string(),
            setup_command: None,
            review_checkout_policy: ReviewCheckoutPolicy::Ask,
            skip_checkout_confirm: false,
            worktree_dir: None,
            pr_source: "origin".to_string(),
            updated_at: 0,
        }
    }
}

pub(crate) async fn read_repository_settings(
    database: &Database,
    repo_key: &str,
) -> Result<RepositorySettings, String> {
    let connection = db::conn(database)?;
    let stored = db::one(
        &connection,
        "SELECT setup_command, review_checkout_policy, skip_checkout_confirm, worktree_dir,
                pr_source, updated_at
         FROM repository_settings WHERE repo_key = ?",
        params![repo_key],
        |row| {
            Ok(RepositorySettings {
                repo_key: repo_key.to_string(),
                setup_command: db::opt_text(row, 0)?,
                review_checkout_policy: ReviewCheckoutPolicy::parse(&db::text(row, 1)?),
                skip_checkout_confirm: db::flag(row, 2)?,
                worktree_dir: db::opt_text(row, 3)?,
                pr_source: db::text(row, 4)?,
                updated_at: db::int(row, 5)?,
            })
        },
    )
    .await?;
    Ok(stored.unwrap_or_else(|| RepositorySettings::defaults(repo_key)))
}

pub(crate) async fn write_repository_settings(
    database: &Database,
    settings: &RepositorySettings,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "INSERT INTO repository_settings(repo_key, setup_command, review_checkout_policy,
             skip_checkout_confirm, worktree_dir, pr_source, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(repo_key) DO UPDATE SET
             setup_command = excluded.setup_command,
             review_checkout_policy = excluded.review_checkout_policy,
             skip_checkout_confirm = excluded.skip_checkout_confirm,
             worktree_dir = excluded.worktree_dir,
             pr_source = excluded.pr_source,
             updated_at = excluded.updated_at",
        params![
            settings.repo_key.as_str(),
            settings.setup_command.as_deref(),
            settings.review_checkout_policy.as_str(),
            i64::from(settings.skip_checkout_confirm),
            settings.worktree_dir.as_deref(),
            settings.pr_source.as_str(),
            unix_secs()
        ],
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::open;

    #[tokio::test]
    async fn a_repository_without_settings_reads_as_the_defaults_until_saved() {
        let directory = tempfile::tempdir().unwrap();
        let database = open(directory.path()).await.unwrap();
        let settings = read_repository_settings(&database, "/r").await.unwrap();
        assert_eq!(settings.review_checkout_policy, ReviewCheckoutPolicy::Ask);
        assert!(!settings.skip_checkout_confirm);

        let mut changed = settings.clone();
        changed.review_checkout_policy = ReviewCheckoutPolicy::WhenNeeded;
        changed.setup_command = Some("bun install".into());
        write_repository_settings(&database, &changed)
            .await
            .unwrap();

        let read = read_repository_settings(&database, "/r").await.unwrap();
        assert_eq!(
            read.review_checkout_policy,
            ReviewCheckoutPolicy::WhenNeeded
        );
        assert_eq!(read.setup_command.as_deref(), Some("bun install"));
        assert_eq!(
            read_repository_settings(&database, "/other")
                .await
                .unwrap()
                .review_checkout_policy,
            ReviewCheckoutPolicy::Ask
        );
    }
}
