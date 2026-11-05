//! A repository as the review desk sees it: a project plus the GitHub
//! repositories that feed its inbox, or a PRs-only repository with no clone.
//!
//! Which remote feeds the inbox is the repository's PR source
//! (`storage::PrSource`). A repository with nothing stored takes the default
//! for its remotes: a fork's `upstream`, else `origin`. These rules are pure
//! over the remotes so the desk and its tests share them.

use serde::Serialize;

use crate::git::remotes::GitRemote;
use crate::storage::PrSource;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) enum RepositoryState {
    /// A clone with a GitHub remote feeding the inbox.
    Cloned,
    /// A GitHub repository with nothing cloned.
    PrsOnly,
    /// A folder no GitHub remote feeds (none exists, or the PR source is
    /// None): it works as a project, with no inbox.
    NoRemote,
}

/// A GitHub repository whose PRs reach a repository's inbox.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PrFeed {
    /// The remote it comes from; empty for a PRs-only repository.
    pub(crate) remote: String,
    /// `owner/name`.
    pub(crate) slug: String,
    /// Show only PRs that ask for the viewer or that the viewer wrote: a
    /// fork's upstream, where everyone else's PRs open too.
    pub(crate) only_mine: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Repository {
    /// What its settings are stored under: the clone's path, or the slug of a
    /// PRs-only repository.
    pub(crate) key: String,
    pub(crate) name: String,
    /// The first feed's `owner/name`; empty with no feed.
    pub(crate) slug: String,
    /// The clone's folder; `None` for a PRs-only repository.
    pub(crate) path: Option<String>,
    pub(crate) state: RepositoryState,
    pub(crate) feeds: Vec<PrFeed>,
}

impl Repository {
    /// A clone at `path`, fed per `source`.
    pub(crate) fn clone_at(path: &str, remotes: &[GitRemote], source: PrSource) -> Self {
        let feeds = feeds(remotes, source);
        Self {
            key: path.to_string(),
            name: folder_name(path).to_string(),
            slug: feeds
                .first()
                .map(|feed| feed.slug.clone())
                .unwrap_or_default(),
            path: Some(path.to_string()),
            state: if feeds.is_empty() {
                RepositoryState::NoRemote
            } else {
                RepositoryState::Cloned
            },
            feeds,
        }
    }

    pub(crate) fn prs_only(slug: &str) -> Self {
        Self {
            key: slug.to_string(),
            name: folder_name(slug).to_string(),
            slug: slug.to_string(),
            path: None,
            state: RepositoryState::PrsOnly,
            feeds: vec![PrFeed {
                remote: String::new(),
                slug: slug.to_string(),
                only_mine: false,
            }],
        }
    }
}

/// What Add repository reads from a local folder before adding it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FolderFacts {
    /// The repository's top level, which is what gets added.
    pub(crate) root: String,
    pub(crate) name: String,
    /// The checked-out git branch; `None` on a detached HEAD.
    pub(crate) branch: Option<String>,
    pub(crate) linked_worktrees: i64,
    pub(crate) remotes: Vec<GitRemote>,
    /// The stored PR source, or the default for these remotes.
    pub(crate) pr_source: PrSource,
    /// A lockfile found at the top level, e.g. `bun.lock`.
    pub(crate) lockfile: Option<String>,
    /// The setup command the lockfile suggests.
    pub(crate) setup_command: Option<String>,
}

/// A folder Pingex already knows that is a clone of a GitHub repository.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct KnownClone {
    /// The slug it was looked up by, as given.
    pub(crate) slug: String,
    pub(crate) path: String,
    /// The remote that points at the repository.
    pub(crate) remote: String,
}

/// Lockfiles, most specific first, and the install command each implies.
pub(crate) const LOCKFILES: [(&str, &str); 7] = [
    ("bun.lock", "bun install"),
    ("bun.lockb", "bun install"),
    ("pnpm-lock.yaml", "pnpm install"),
    ("yarn.lock", "yarn install"),
    ("package-lock.json", "npm install"),
    ("uv.lock", "uv sync"),
    ("Gemfile.lock", "bundle install"),
];

/// The last segment of a host path or slug.
pub(crate) fn folder_name(path: &str) -> &str {
    path.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(path)
}

fn github_remote<'a>(remotes: &'a [GitRemote], name: &str) -> Option<&'a GitRemote> {
    remotes
        .iter()
        .find(|remote| remote.name == name && remote.slug.is_some())
}

/// The PR source a clone starts with: `upstream` when it has a GitHub
/// upstream (a fork), else `origin` when that is on GitHub, else None.
pub(crate) fn default_pr_source(remotes: &[GitRemote]) -> PrSource {
    if github_remote(remotes, "upstream").is_some() {
        PrSource::Upstream
    } else if github_remote(remotes, "origin").is_some() {
        PrSource::Origin
    } else {
        PrSource::None
    }
}

/// The feeds `source` selects from `remotes`. A selected remote that is not
/// on GitHub feeds nothing.
pub(crate) fn feeds(remotes: &[GitRemote], source: PrSource) -> Vec<PrFeed> {
    let names: &[&str] = match source {
        PrSource::Origin => &["origin"],
        PrSource::Upstream => &["upstream"],
        PrSource::Both => &["origin", "upstream"],
        PrSource::None => &[],
    };
    names
        .iter()
        .filter_map(|name| github_remote(remotes, name))
        .map(|remote| PrFeed {
            remote: remote.name.clone(),
            slug: remote.slug.clone().unwrap_or_default(),
            only_mine: remote.name == "upstream",
        })
        .collect()
}

/// `source` widened so that `remote` feeds the inbox too.
pub(crate) fn source_with(source: PrSource, remote: &str) -> Result<PrSource, String> {
    Ok(match (remote, source) {
        ("origin", PrSource::None | PrSource::Origin) => PrSource::Origin,
        ("upstream", PrSource::None | PrSource::Upstream) => PrSource::Upstream,
        ("origin" | "upstream", _) => PrSource::Both,
        _ => {
            return Err(format!(
                "PRs come from a remote named origin or upstream, not {remote}"
            ))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn remote(name: &str, slug: Option<&str>) -> GitRemote {
        GitRemote {
            name: name.into(),
            url: String::new(),
            slug: slug.map(str::to_string),
        }
    }

    #[test]
    fn a_fork_defaults_to_its_upstream_and_shows_only_my_prs_there() {
        let remotes = [
            remote("origin", Some("pingu-codes/codex-mirror")),
            remote("upstream", Some("openai/codex")),
        ];
        assert_eq!(default_pr_source(&remotes), PrSource::Upstream);
        let repository = Repository::clone_at("/code/codex-mirror", &remotes, PrSource::Upstream);
        assert_eq!(repository.state, RepositoryState::Cloned);
        assert_eq!(repository.slug, "openai/codex");
        assert!(repository.feeds[0].only_mine);

        let both = feeds(&remotes, PrSource::Both);
        let found: Vec<(&str, bool)> = both
            .iter()
            .map(|feed| (feed.slug.as_str(), feed.only_mine))
            .collect();
        assert_eq!(
            found,
            [("pingu-codes/codex-mirror", false), ("openai/codex", true)]
        );
    }

    #[test]
    fn a_clone_with_no_github_remote_or_no_source_has_no_inbox() {
        let local = [remote("origin", None)];
        assert_eq!(default_pr_source(&local), PrSource::None);
        let repository = Repository::clone_at("/code/notes", &local, PrSource::Origin);
        assert_eq!(repository.state, RepositoryState::NoRemote);
        assert_eq!(repository.name, "notes");

        let github = [remote("origin", Some("o/n"))];
        let repository = Repository::clone_at("/code/n", &github, PrSource::None);
        assert_eq!(repository.state, RepositoryState::NoRemote);
    }

    #[test]
    fn linking_a_remote_widens_the_source_to_include_it() {
        assert_eq!(source_with(PrSource::None, "origin"), Ok(PrSource::Origin));
        assert_eq!(
            source_with(PrSource::Upstream, "origin"),
            Ok(PrSource::Both)
        );
        assert_eq!(
            source_with(PrSource::Origin, "upstream"),
            Ok(PrSource::Both)
        );
        assert_eq!(source_with(PrSource::Both, "upstream"), Ok(PrSource::Both));
        assert!(source_with(PrSource::Origin, "mirror").is_err());
    }
}
