//! A clone's remotes and the GitHub repository each one points at, and
//! cloning a repository: what Add repository reads from a local folder and
//! does for "Clone".
//!
//! Remote URLs come from `git config`, not `git remote -v`, so they are the
//! URLs as written: `url.<base>.insteadOf` rewrites do not hide a GitHub URL.

use std::path::Path;

use serde::Serialize;

use super::objects::url_slug;
use super::run::{redact_classified, run_git, run_git_network, READ_TIMEOUT};
use crate::util::host::Host;

/// One remote of a clone.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GitRemote {
    pub(crate) name: String,
    pub(crate) url: String,
    /// The GitHub repository it points at, as lowercase `owner/name`; `None`
    /// for a remote elsewhere (a local path, another forge).
    pub(crate) slug: Option<String>,
}

/// `owner/name` when `url` points at GitHub: `github.com`, or an SSH host
/// alias that names it (`git@github-personal:o/n.git`).
pub(crate) fn github_slug(url: &str) -> Option<String> {
    let url = url.trim();
    let host = match url.split_once("://") {
        Some((_, rest)) => rest.split('/').next()?,
        None => url.split_once(':')?.0,
    };
    let host = host.rsplit('@').next()?.to_lowercase();
    host.contains("github").then(|| url_slug(url)).flatten()
}

/// Parse `git config --null --get-regexp ^remote\..*\.url$`: each entry is
/// `remote.<name>.url\n<url>\0`. A remote with several URLs keeps its first.
fn parse_remote_urls(stdout: &str) -> Vec<GitRemote> {
    let mut remotes: Vec<GitRemote> = Vec::new();
    for entry in stdout.split('\0') {
        let Some((key, url)) = entry.split_once('\n') else {
            continue;
        };
        let Some(name) = key
            .strip_prefix("remote.")
            .and_then(|key| key.strip_suffix(".url"))
        else {
            continue;
        };
        if remotes.iter().any(|remote| remote.name == name) {
            continue;
        }
        remotes.push(GitRemote {
            name: name.to_string(),
            url: url.to_string(),
            slug: github_slug(url),
        });
    }
    remotes
}

/// The remotes of the clone at `dir`, in config order.
pub(crate) fn read_remotes(host: &Host, dir: &Path) -> Result<Vec<GitRemote>, String> {
    let output = run_git(
        host,
        dir,
        &["config", "--null", "--get-regexp", r"^remote\..*\.url$"],
        READ_TIMEOUT,
    )?;
    // Exit 1: no remotes at all.
    if !output.ok && output.code != Some(1) {
        return Err("Could not read the repository's remotes".to_string());
    }
    Ok(parse_remote_urls(&output.stdout))
}

/// A folder name a clone may take: one path segment that git cannot read as
/// an option.
fn require_folder_name(name: &str) -> Result<&str, String> {
    let ok = !name.is_empty()
        && !name.starts_with('-')
        && !matches!(name, "." | "..")
        && !name.contains(['/', '\\']);
    if ok {
        Ok(name)
    } else {
        Err("That is not a folder name".to_string())
    }
}

/// Clone `url` into `<parent>/<name>` on `host`, creating `parent` when it is
/// missing. Returns the clone's host path.
pub(crate) fn clone_repository(
    host: &Host,
    parent: &str,
    url: &str,
    name: &str,
) -> Result<String, String> {
    let url = url.trim();
    if url.is_empty() || url.starts_with('-') {
        return Err("That is not a repository URL".to_string());
    }
    let name = require_folder_name(name.trim())?;
    let parent = host.expand_tilde(parent.trim());
    if parent.is_empty() {
        return Err("Choose a folder to clone into".to_string());
    }
    let target = host.join_str(&parent, name);
    if host.to_local(&target).exists() {
        return Err(format!("{target} already exists"));
    }
    std::fs::create_dir_all(host.to_local(&parent))
        .map_err(|_| format!("Could not create {parent}"))?;
    let output = run_git_network(host, Path::new(&parent), &["clone", "-q", "--", url, name])?;
    if !output.ok {
        return Err(redact_classified("Could not clone the repository", &output));
    }
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::test_repo::TestRepo;

    #[test]
    fn reads_which_github_repository_each_remote_points_at() {
        let Some(repo) = TestRepo::new() else {
            return;
        };
        repo.serve_as_github("pingu-codes/codex-mirror");
        repo.git(&[
            "remote",
            "add",
            "upstream",
            "git@github.com:openai/codex.git",
        ]);
        repo.git(&[
            "remote",
            "add",
            "mirror",
            "https://gitlab.com/pingu/codex.git",
        ]);

        let remotes = read_remotes(&Host::Native, &repo.repo).unwrap();

        let found: Vec<(&str, Option<&str>)> = remotes
            .iter()
            .map(|remote| (remote.name.as_str(), remote.slug.as_deref()))
            .collect();
        assert_eq!(
            found,
            [
                ("origin", Some("pingu-codes/codex-mirror")),
                ("upstream", Some("openai/codex")),
                ("mirror", None),
            ]
        );
    }

    #[test]
    fn a_clone_without_remotes_has_none() {
        let Some(repo) = TestRepo::new() else {
            return;
        };
        repo.git(&["remote", "remove", "origin"]);
        assert_eq!(read_remotes(&Host::Native, &repo.repo).unwrap(), []);
    }

    #[test]
    fn recognises_github_urls_in_every_spelling() {
        for url in [
            "https://github.com/o/n.git",
            "https://github.com/o/n",
            "git@github.com:o/n.git",
            "ssh://git@github.com/o/n.git",
            "git@github-personal:o/n.git",
        ] {
            assert_eq!(github_slug(url).as_deref(), Some("o/n"), "{url}");
        }
        for url in [
            "/local/path",
            "https://gitlab.com/o/n.git",
            "git@bitbucket.org:o/n",
        ] {
            assert_eq!(github_slug(url), None, "{url}");
        }
    }

    #[test]
    fn clones_into_a_new_folder_under_the_chosen_parent() {
        let Some(repo) = TestRepo::new() else {
            return;
        };
        let parent = tempfile::tempdir().unwrap();
        let into = parent.path().join("code");
        let into = into.to_str().unwrap();

        let path =
            clone_repository(&Host::Native, into, repo.remote.to_str().unwrap(), "pingex").unwrap();

        assert_eq!(Path::new(&path), parent.path().join("code/pingex"));
        assert!(Path::new(&path).join("README.md").exists());
        let again = clone_repository(&Host::Native, into, repo.remote.to_str().unwrap(), "pingex");
        assert!(again.unwrap_err().contains("already exists"));
    }

    #[test]
    fn refuses_folder_names_git_would_misread() {
        for name in ["", "-x", "..", "a/b"] {
            let result = clone_repository(&Host::Native, "/tmp", "https://github.com/o/n", name);
            assert!(result.is_err(), "{name}");
        }
    }
}
