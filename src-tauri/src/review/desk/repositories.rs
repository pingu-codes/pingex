//! Repositories for Add repository and the sidebar: the account's
//! repositories on GitHub, what a local folder's remotes say, linking a clone
//! Pingex already knows, cloning, PRs-only repositories, and per-repository
//! settings.
//!
//! The account-level reads answer for the account, whichever repository the
//! desk was made for. The rest work on the desk's folder (`repo_dir`); for a
//! PRs-only repository that "folder" is its slug, which is also its settings
//! key.

use std::path::Path;

use super::ReviewDesk;
use crate::git::remotes::{self, GitRemote};
use crate::git::run::{run_git, READ_TIMEOUT};
use crate::review::repository::{
    default_pr_source, folder_name, source_with, FolderFacts, KnownClone, Repository, LOCKFILES,
};
use crate::review::types::{GitHubAccount, GitHubRepo, ReviewRequests};
use crate::storage::{self, RepositorySettings};
use crate::util::host::Host;

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

    /// The signed-in GitHub account and where its token comes from.
    pub(crate) async fn github_account(&self) -> Result<GitHubAccount, String> {
        self.blocking("Reading the GitHub account failed", |desk| {
            desk.github.account()
        })
        .await
    }

    /// Make `login`, already signed in, the account Pingex uses.
    pub(crate) async fn switch_github_account(&self, login: String) -> Result<(), String> {
        self.blocking("Switching the GitHub account failed", move |desk| {
            desk.github.switch_account(&login)
        })
        .await
    }

    /// The key the desk's repository settings are stored under.
    fn settings_key(&self) -> String {
        self.host.path_string(self.dir())
    }

    async fn remotes(&self) -> Result<Vec<GitRemote>, String> {
        self.blocking("Reading remotes failed", |desk| {
            remotes::read_remotes(&desk.host, desk.dir())
        })
        .await
    }

    /// The stored settings, with the default PR source for `remotes` when
    /// nothing is stored yet.
    async fn effective_settings(
        &self,
        remotes: &[GitRemote],
    ) -> Result<RepositorySettings, String> {
        let mut settings =
            storage::read_repository_settings(&self.database, &self.settings_key()).await?;
        if settings.updated_at == 0 {
            settings.pr_source = default_pr_source(remotes);
        }
        Ok(settings)
    }

    /// What a folder holds before it is added: its top level, git branch,
    /// linked worktrees, remotes and the PR source they suggest, and a
    /// lockfile's setup command.
    pub(crate) async fn inspect_folder(&self) -> Result<FolderFacts, String> {
        let (root, branch, linked_worktrees) = self
            .blocking("Reading the folder failed", |desk| {
                let (host, dir) = (&desk.host, desk.dir());
                let top = git_line(host, dir, &["rev-parse", "--show-toplevel"])
                    .ok_or_else(|| "This folder is not a Git repository".to_string())?;
                let root = Path::new(&top);
                let branch = git_line(host, root, &["symbolic-ref", "--short", "-q", "HEAD"]);
                let worktrees = git_line(host, root, &["worktree", "list", "--porcelain"])
                    .map(|list| list.lines().filter(|l| l.starts_with("worktree ")).count())
                    .unwrap_or(1);
                Ok((top, branch, worktrees.saturating_sub(1) as i64))
            })
            .await?;
        let at_root = self.at(&root);
        let remotes = at_root.remotes().await?;
        let settings = at_root.effective_settings(&remotes).await?;
        let lockfile = LOCKFILES.iter().find(|(file, _)| {
            self.host
                .to_local(&self.host.join_str(&root, file))
                .exists()
        });
        Ok(FolderFacts {
            name: folder_name(&root).to_string(),
            root,
            branch,
            linked_worktrees,
            remotes,
            pr_source: settings.pr_source,
            lockfile: lockfile.map(|(file, _)| file.to_string()),
            setup_command: lockfile.map(|(_, command)| command.to_string()),
        })
    }

    /// This desk's clone as a repository: cloned, or with no GitHub remote
    /// feeding it.
    pub(crate) async fn repository(&self) -> Result<Repository, String> {
        let remotes = self.remotes().await?;
        let settings = self.effective_settings(&remotes).await?;
        Ok(Repository::clone_at(
            &self.settings_key(),
            &remotes,
            settings.pr_source,
        ))
    }

    pub(crate) async fn repository_settings(&self) -> Result<RepositorySettings, String> {
        // A PRs-only repository's "folder" is its slug: it has no remotes.
        let remotes = self.remotes().await.unwrap_or_default();
        self.effective_settings(&remotes).await
    }

    /// Store `settings` for this desk's repository, whatever key they carry.
    pub(crate) async fn save_repository_settings(
        &self,
        settings: RepositorySettings,
    ) -> Result<(), String> {
        let settings = RepositorySettings {
            repo_key: self.settings_key(),
            ..settings
        };
        storage::write_repository_settings(&self.database, &settings).await
    }

    /// Link this clone to GitHub repository `slug`, so its PRs feed the
    /// inbox: the PR source widens to include the remote that points there.
    pub(crate) async fn link_clone(&self, slug: String) -> Result<Repository, String> {
        let remotes = self.remotes().await?;
        let wanted = slug.to_lowercase();
        let remote = remotes
            .iter()
            .find(|remote| remote.slug.as_deref() == Some(wanted.as_str()))
            .ok_or_else(|| format!("This folder has no remote for {slug}"))?;
        let settings = self.effective_settings(&remotes).await?;
        let pr_source = source_with(settings.pr_source, &remote.name)?;
        self.save_repository_settings(RepositorySettings {
            pr_source,
            ..settings
        })
        .await?;
        self.repository().await
    }

    /// For each of `slugs`, the first of `candidates` (folders Pingex knows)
    /// that is a clone of it; slugs with no clone are left out. Unreadable
    /// folders are skipped.
    pub(crate) async fn find_clones(
        &self,
        slugs: Vec<String>,
        candidates: Vec<String>,
    ) -> Result<Vec<KnownClone>, String> {
        self.blocking("Looking for clones failed", move |desk| {
            let mut found: Vec<KnownClone> = Vec::new();
            for path in candidates {
                let Ok(remotes) = remotes::read_remotes(&desk.host, Path::new(&path)) else {
                    continue;
                };
                for slug in &slugs {
                    let wanted = slug.to_lowercase();
                    let remote = remotes
                        .iter()
                        .find(|remote| remote.slug.as_deref() == Some(wanted.as_str()));
                    if let Some(remote) = remote {
                        if !found.iter().any(|clone| clone.slug == *slug) {
                            found.push(KnownClone {
                                slug: slug.clone(),
                                path: path.clone(),
                                remote: remote.name.clone(),
                            });
                        }
                    }
                }
            }
            Ok(found)
        })
        .await
    }

    /// Clone `url` into `<repo_dir>/<name>` on the desk's Host: this desk is
    /// over the folder to clone into. Returns the clone's path.
    pub(crate) async fn clone_repository(
        &self,
        url: String,
        name: String,
    ) -> Result<String, String> {
        self.blocking("Cloning failed", move |desk| {
            let parent = desk.settings_key();
            remotes::clone_repository(&desk.host, &parent, &url, &name)
        })
        .await
    }

    pub(crate) async fn prs_only_repositories(&self) -> Result<Vec<Repository>, String> {
        let slugs = storage::prs_only_repositories(&self.database).await?;
        Ok(slugs
            .iter()
            .map(|slug| Repository::prs_only(slug))
            .collect())
    }

    pub(crate) async fn add_prs_only_repository(&self, slug: String) -> Result<(), String> {
        let slug = require_slug(&slug)?;
        storage::add_prs_only_repository(&self.database, slug).await
    }

    pub(crate) async fn remove_prs_only_repository(&self, slug: String) -> Result<(), String> {
        storage::remove_prs_only_repository(&self.database, &slug).await
    }

    /// The same desk over another folder.
    fn at(&self, dir: &str) -> ReviewDesk {
        ReviewDesk {
            repo_dir: dir.into(),
            ..self.clone()
        }
    }
}

/// The first line of a successful git command's output.
fn git_line(host: &Host, dir: &Path, args: &[&str]) -> Option<String> {
    let output = run_git(host, dir, args, READ_TIMEOUT).ok()?;
    let line = output.stdout.trim();
    (output.ok && !line.is_empty()).then(|| line.to_string())
}

/// `owner/name` with nothing else in it.
fn require_slug(slug: &str) -> Result<&str, String> {
    let slug = slug.trim();
    let valid = |part: &str| {
        !part.is_empty()
            && !part.starts_with(['-', '.'])
            && part
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
    };
    match slug.split_once('/') {
        Some((owner, name)) if valid(owner) && valid(name) => Ok(slug),
        _ => Err("Expected a GitHub repository as owner/name".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{fixture, SLUG};
    use crate::review::repository::RepositoryState;
    use crate::review::types::{GitHubRepo, ReviewRequests};
    use crate::storage::{PrSource, RepositorySettings};

    fn canonical(path: &std::path::Path) -> String {
        std::fs::canonicalize(path)
            .unwrap()
            .to_string_lossy()
            .into_owned()
    }

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

    #[tokio::test]
    async fn reads_a_forks_remotes_git_branch_worktrees_and_lockfile() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let repo = &fixture.repo;
        repo.git(&[
            "remote",
            "add",
            "upstream",
            "https://github.com/openai/codex.git",
        ]);
        repo.commit("bun.lock", "{}\n", "lock");
        let worktree = repo.repo.parent().unwrap().join("linked");
        repo.git(&[
            "worktree",
            "add",
            "-q",
            "-b",
            "side",
            worktree.to_str().unwrap(),
        ]);

        // Inspecting a subfolder reads the whole repository.
        std::fs::create_dir_all(repo.repo.join("src")).unwrap();
        let facts = fixture
            .desk
            .at(repo.repo.join("src").to_str().unwrap())
            .inspect_folder()
            .await
            .unwrap();

        assert_eq!(facts.root, canonical(&repo.repo));
        assert_eq!(facts.branch.as_deref(), Some("main"));
        assert_eq!(facts.linked_worktrees, 1);
        let slugs: Vec<_> = facts.remotes.iter().map(|r| r.slug.as_deref()).collect();
        assert_eq!(slugs, [Some(SLUG), Some("openai/codex")]);
        assert_eq!(facts.pr_source, PrSource::Upstream);
        assert_eq!(facts.lockfile.as_deref(), Some("bun.lock"));
        assert_eq!(facts.setup_command.as_deref(), Some("bun install"));
    }

    #[tokio::test]
    async fn a_folder_outside_git_is_not_a_repository() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let plain = tempfile::tempdir().unwrap();
        let error = fixture
            .desk
            .at(plain.path().to_str().unwrap())
            .inspect_folder()
            .await
            .unwrap_err();
        assert!(error.contains("not a Git repository"), "{error}");
    }

    #[tokio::test]
    async fn a_clone_is_a_repository_fed_by_its_stored_pr_source() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let repository = fixture.desk.repository().await.unwrap();
        assert_eq!(repository.state, RepositoryState::Cloned);
        assert_eq!(repository.slug, SLUG);

        let settings = fixture.desk.repository_settings().await.unwrap();
        fixture
            .desk
            .save_repository_settings(RepositorySettings {
                pr_source: PrSource::None,
                setup_command: Some("bun install".into()),
                ..settings
            })
            .await
            .unwrap();

        let repository = fixture.desk.repository().await.unwrap();
        assert_eq!(repository.state, RepositoryState::NoRemote);
        assert!(repository.feeds.is_empty());
        let stored = fixture.desk.repository_settings().await.unwrap();
        assert_eq!(stored.setup_command.as_deref(), Some("bun install"));
    }

    #[tokio::test]
    async fn finds_and_links_a_clone_pingex_already_knows() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let elsewhere = tempfile::tempdir().unwrap();
        let path = fixture.repo.repo.to_string_lossy().into_owned();
        let candidates = vec![
            elsewhere.path().to_string_lossy().into_owned(),
            "/no/such/folder".to_string(),
            path.clone(),
        ];

        let found = fixture
            .desk
            .find_clones(
                vec!["PINGEX/desk".into(), "someone/else".into()],
                candidates,
            )
            .await
            .unwrap();
        let found: Vec<_> = found
            .iter()
            .map(|clone| {
                (
                    clone.slug.as_str(),
                    clone.path.as_str(),
                    clone.remote.as_str(),
                )
            })
            .collect();
        assert_eq!(found, [("PINGEX/desk", path.as_str(), "origin")]);

        // The clone feeds nothing until it is linked.
        let settings = fixture.desk.repository_settings().await.unwrap();
        fixture
            .desk
            .save_repository_settings(RepositorySettings {
                pr_source: PrSource::None,
                ..settings
            })
            .await
            .unwrap();
        let linked = fixture.desk.link_clone(SLUG.into()).await.unwrap();
        assert_eq!(linked.state, RepositoryState::Cloned);
        assert_eq!(linked.feeds[0].remote, "origin");
        assert!(fixture
            .desk
            .link_clone("someone/else".into())
            .await
            .is_err());
    }

    #[tokio::test]
    async fn clones_a_repository_on_the_desks_host() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let parent = tempfile::tempdir().unwrap();
        let path = fixture
            .desk
            .at(parent.path().to_str().unwrap())
            .clone_repository(
                fixture.repo.remote.to_string_lossy().into_owned(),
                "desk".into(),
            )
            .await
            .unwrap();
        assert!(std::path::Path::new(&path).join("README.md").exists());
    }

    #[tokio::test]
    async fn keeps_prs_only_repositories_until_removed() {
        let Some(fixture) = fixture().await else {
            return;
        };
        fixture
            .desk
            .add_prs_only_repository("tomasz/bun-tools".into())
            .await
            .unwrap();
        assert!(fixture
            .desk
            .add_prs_only_repository("--upload-pack=x".into())
            .await
            .is_err());

        let listed = fixture.desk.prs_only_repositories().await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].state, RepositoryState::PrsOnly);
        assert_eq!(
            (listed[0].name.as_str(), listed[0].path.as_ref()),
            ("bun-tools", None)
        );

        fixture
            .desk
            .remove_prs_only_repository("tomasz/bun-tools".into())
            .await
            .unwrap();
        assert!(fixture
            .desk
            .prs_only_repositories()
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn reports_the_signed_in_account_and_switches_it() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let account = fixture.desk.github_account().await.unwrap();
        assert_eq!(account.login.as_deref(), Some("pingu-codes"));

        fixture
            .desk
            .switch_github_account("other-login".into())
            .await
            .unwrap();
        let account = fixture.desk.github_account().await.unwrap();
        assert_eq!(account.login.as_deref(), Some("other-login"));
    }
}
