//! Per-repository settings: where worktrees go, the setup command, when a
//! review checkout may be created, and which remote feeds the inbox.
//!
//! Keyed by `repo_key`: the project's root path as stored for projects, or
//! the GitHub `owner/name` of a PRs-only repository, which has no folder. A
//! repository without a row reads as the defaults.

use serde::{Deserialize, Serialize};
use turso::{params, Database};

use super::db;
use crate::util::time::unix_secs;

/// When a review checkout may be created for an action that needs one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ReviewCheckoutPolicy {
    /// Confirm first, unless "Don't ask again" was chosen.
    #[default]
    Ask,
    /// Create it without asking.
    WhenNeeded,
    /// Never create one; such actions are unavailable.
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

/// Which remote's GitHub repository feeds the inbox.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) enum PrSource {
    #[default]
    Origin,
    /// A fork's upstream, where its PRs open.
    Upstream,
    Both,
    /// No inbox: threads and worktrees only.
    None,
}

impl PrSource {
    fn as_str(self) -> &'static str {
        match self {
            Self::Origin => "origin",
            Self::Upstream => "upstream",
            Self::Both => "both",
            Self::None => "none",
        }
    }

    fn parse(value: &str) -> Self {
        match value {
            "upstream" => Self::Upstream,
            "both" => Self::Both,
            "none" => Self::None,
            _ => Self::Origin,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RepositorySettings {
    pub(crate) repo_key: String,
    /// Run once in a new review checkout or worktree, e.g. `bun install`.
    pub(crate) setup_command: Option<String>,
    pub(crate) review_checkout_policy: ReviewCheckoutPolicy,
    /// "Don't ask again for <repository>" on the checkout confirmation.
    pub(crate) skip_checkout_confirm: bool,
    /// Where new worktrees go; `None` for the app default.
    pub(crate) worktree_dir: Option<String>,
    pub(crate) pr_source: PrSource,
    /// Unix seconds; 0 for a repository with nothing stored.
    pub(crate) updated_at: i64,
}

/// `None` for an empty or blank value.
fn non_blank(value: &Option<String>) -> Option<&str> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// The stored settings for `repo_key`, or the defaults.
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
                pr_source: PrSource::parse(&db::text(row, 4)?),
                updated_at: db::int(row, 5)?,
            })
        },
    )
    .await?;
    Ok(stored.unwrap_or_else(|| RepositorySettings {
        repo_key: repo_key.to_string(),
        ..RepositorySettings::default()
    }))
}

/// Store every field of `settings`, blank text as `NULL`.
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
            non_blank(&settings.setup_command),
            settings.review_checkout_policy.as_str(),
            i64::from(settings.skip_checkout_confirm),
            non_blank(&settings.worktree_dir),
            settings.pr_source.as_str(),
            unix_secs()
        ],
    )
    .await
}

/// The GitHub login the user chose for `repo_key`.
pub(crate) async fn read_repository_account(
    database: &Database,
    repo_key: &str,
) -> Result<Option<String>, String> {
    let connection = db::conn(database)?;
    let stored = db::one(
        &connection,
        "SELECT github_login FROM repository_settings WHERE repo_key = ?",
        params![repo_key],
        |row| db::opt_text(row, 0),
    )
    .await?;
    Ok(stored.flatten())
}

/// Choose a GitHub login for `repo_key`, or with `None` clear the choice. Kept
/// apart from `RepositorySettings` so a settings save can't overwrite it. A
/// new row keeps `updated_at` at 0, "nothing stored", so that choosing an
/// account doesn't also freeze the repository's default PR source.
pub(crate) async fn write_repository_account(
    database: &Database,
    repo_key: &str,
    login: Option<&str>,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "INSERT INTO repository_settings(repo_key, github_login, updated_at)
         VALUES (?, ?, 0)
         ON CONFLICT(repo_key) DO UPDATE SET github_login = excluded.github_login",
        params![repo_key, login],
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_chosen_account_survives_a_settings_save_and_can_be_cleared() {
        let dir = tempfile::tempdir().unwrap();
        let database = crate::storage::open(dir.path()).await.unwrap();
        let key = "/code/pingex";
        assert_eq!(read_repository_account(&database, key).await.unwrap(), None);

        write_repository_account(&database, key, Some("pingu-codes"))
            .await
            .unwrap();
        let settings = RepositorySettings {
            repo_key: key.into(),
            setup_command: Some("bun install".into()),
            ..RepositorySettings::default()
        };
        write_repository_settings(&database, &settings)
            .await
            .unwrap();
        assert_eq!(
            read_repository_account(&database, key)
                .await
                .unwrap()
                .as_deref(),
            Some("pingu-codes")
        );

        write_repository_account(&database, key, None)
            .await
            .unwrap();
        assert_eq!(read_repository_account(&database, key).await.unwrap(), None);
        let settings = read_repository_settings(&database, key).await.unwrap();
        assert_eq!(settings.setup_command.as_deref(), Some("bun install"));
    }

    #[tokio::test]
    async fn choosing_an_account_alone_leaves_the_settings_unstored() {
        let dir = tempfile::tempdir().unwrap();
        let database = crate::storage::open(dir.path()).await.unwrap();
        write_repository_account(&database, "/code/pingex", Some("pingu-codes"))
            .await
            .unwrap();
        let settings = read_repository_settings(&database, "/code/pingex")
            .await
            .unwrap();
        assert_eq!(
            settings.updated_at, 0,
            "the default PR source still applies"
        );
    }

    #[tokio::test]
    async fn a_repository_with_nothing_stored_reads_as_the_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let database = crate::storage::open(dir.path()).await.unwrap();

        let settings = read_repository_settings(&database, "/code/pingex")
            .await
            .unwrap();

        assert_eq!(settings.repo_key, "/code/pingex");
        assert_eq!(settings.review_checkout_policy, ReviewCheckoutPolicy::Ask);
        assert_eq!(settings.pr_source, PrSource::Origin);
        assert_eq!(settings.setup_command, None);
        assert!(!settings.skip_checkout_confirm);
    }

    #[tokio::test]
    async fn stores_and_replaces_a_repositorys_settings() {
        let dir = tempfile::tempdir().unwrap();
        let database = crate::storage::open(dir.path()).await.unwrap();
        let settings = RepositorySettings {
            repo_key: "/code/pingex".into(),
            setup_command: Some("bun install".into()),
            review_checkout_policy: ReviewCheckoutPolicy::WhenNeeded,
            skip_checkout_confirm: true,
            worktree_dir: Some("~/worktrees/pingex".into()),
            pr_source: PrSource::Upstream,
            updated_at: 0,
        };
        write_repository_settings(&database, &settings)
            .await
            .unwrap();
        let stored = read_repository_settings(&database, "/code/pingex")
            .await
            .unwrap();
        assert_eq!(stored.setup_command.as_deref(), Some("bun install"));
        assert_eq!(
            stored.review_checkout_policy,
            ReviewCheckoutPolicy::WhenNeeded
        );
        assert_eq!(stored.pr_source, PrSource::Upstream);
        assert!(stored.skip_checkout_confirm && stored.updated_at > 0);

        let cleared = RepositorySettings {
            setup_command: Some("  ".into()),
            review_checkout_policy: ReviewCheckoutPolicy::Never,
            ..stored
        };
        write_repository_settings(&database, &cleared)
            .await
            .unwrap();
        let stored = read_repository_settings(&database, "/code/pingex")
            .await
            .unwrap();
        assert_eq!(stored.setup_command, None);
        assert_eq!(stored.review_checkout_policy, ReviewCheckoutPolicy::Never);
    }
}
