//! One read of a repository's refs: branch tips, remote-tracking tips, each
//! branch's upstream and each remote's default branch, from a single
//! `for-each-ref`.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use crate::git::objects::{is_ancestor, resolve};
use crate::git::run::{run_git, run_git_with_input, READ_TIMEOUT};
use crate::util::host::Host;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Refs {
    /// Local branch name to tip.
    pub(super) heads: BTreeMap<String, String>,
    /// `remote/name` to tip.
    pub(super) remotes: HashMap<String, String>,
    /// Local branch name to its upstream, as `remote/name`.
    pub(super) upstream: HashMap<String, String>,
    /// Remote name to its default branch, as `remote/name`
    /// (`refs/remotes/<remote>/HEAD`).
    pub(super) remote_heads: HashMap<String, String>,
}

impl Refs {
    pub(super) fn parse(stdout: &str) -> Self {
        let mut refs = Refs::default();
        for line in stdout.lines() {
            let mut fields = line.split('\0');
            let (Some(name), Some(sha)) = (fields.next(), fields.next()) else {
                continue;
            };
            let upstream = fields.next().unwrap_or("");
            let symref = fields.next().unwrap_or("");
            if let Some(branch) = name.strip_prefix("refs/heads/") {
                refs.heads.insert(branch.to_string(), sha.to_string());
                if let Some(up) = upstream.strip_prefix("refs/remotes/") {
                    refs.upstream.insert(branch.to_string(), up.to_string());
                }
            } else if let Some(remote_ref) = name.strip_prefix("refs/remotes/") {
                match remote_ref.strip_suffix("/HEAD") {
                    Some(remote) => {
                        if let Some(target) = symref.strip_prefix("refs/remotes/") {
                            refs.remote_heads
                                .insert(remote.to_string(), target.to_string());
                        }
                    }
                    None => {
                        refs.remotes.insert(remote_ref.to_string(), sha.to_string());
                    }
                }
            }
        }
        refs
    }

    /// The tip of `name` as a pull request's base would name it: the
    /// remote's copy first (what GitHub compares with), else the local
    /// branch. Returns the label and the SHA.
    pub(super) fn base_tip(&self, remote: Option<&str>, name: &str) -> Option<(String, String)> {
        if let Some(remote) = remote {
            let key = format!("{remote}/{name}");
            if let Some(sha) = self.remotes.get(&key) {
                return Some((key, sha.clone()));
            }
        }
        self.heads
            .get(name)
            .map(|sha| (name.to_string(), sha.clone()))
    }

    /// A branch's tip: the local branch, else the remote's copy.
    pub(super) fn branch_tip(&self, remote: Option<&str>, name: &str) -> Option<String> {
        self.heads.get(name).cloned().or_else(|| {
            remote.and_then(|remote| self.remotes.get(&format!("{remote}/{name}")).cloned())
        })
    }

    /// The remote a branch's base most likely lives on: the current branch's
    /// upstream remote, else `origin`, else the first remote known.
    pub(super) fn preferred_remote(&self, branch: Option<&str>) -> Option<String> {
        let from_upstream = branch
            .and_then(|branch| self.upstream.get(branch))
            .and_then(|up| up.split_once('/'))
            .map(|(remote, _)| remote.to_string());
        if from_upstream.is_some() {
            return from_upstream;
        }
        let remotes = self
            .remotes
            .keys()
            .filter_map(|key| key.split_once('/').map(|(remote, _)| remote));
        let mut first: Option<&str> = None;
        for remote in remotes {
            if remote == "origin" {
                return Some("origin".into());
            }
            first = match first {
                Some(known) if known <= remote => Some(known),
                _ => Some(remote),
            };
        }
        first.map(str::to_string)
    }

    /// The default branch, as a short name (`main`), and the ref that names
    /// it: the remote's `HEAD`, else `main`/`master` on the remote, else
    /// locally.
    pub(super) fn default_branch(&self, remote: Option<&str>) -> Option<(String, String, String)> {
        if let Some(remote) = remote {
            if let Some(target) = self.remote_heads.get(remote) {
                if let Some(sha) = self.remotes.get(target) {
                    let short = target.strip_prefix(&format!("{remote}/")).unwrap_or(target);
                    return Some((short.to_string(), target.clone(), sha.clone()));
                }
            }
        }
        for name in ["main", "master"] {
            if let Some((label, sha)) = self.base_tip(remote, name) {
                return Some((name.to_string(), label, sha));
            }
        }
        None
    }
}

pub(super) fn read_refs(host: &Host, root: &Path) -> Result<Refs, String> {
    let output = run_git(
        host,
        root,
        &[
            "for-each-ref",
            "--format=%(refname)%00%(objectname)%00%(upstream)%00%(symref)",
            "refs/heads",
            "refs/remotes",
        ],
        READ_TIMEOUT,
    )?;
    if !output.ok {
        return Err("Could not read the repository's branches".to_string());
    }
    Ok(Refs::parse(&output.stdout))
}

/// The git branch checked out, `None` when detached or unborn.
pub(super) fn current_branch(host: &Host, root: &Path) -> Option<String> {
    let output = run_git(host, root, &["symbolic-ref", "-q", "HEAD"], READ_TIMEOUT).ok()?;
    output
        .ok
        .then(|| {
            output
                .stdout
                .trim()
                .strip_prefix("refs/heads/")
                .map(str::to_string)
        })
        .flatten()
}

pub(super) fn head_sha(host: &Host, root: &Path) -> Option<String> {
    resolve(host, root, "HEAD").ok().flatten()
}

pub(super) fn merge_base(host: &Host, root: &Path, a: &str, b: &str) -> Option<String> {
    let output = run_git(host, root, &["merge-base", a, b], READ_TIMEOUT).ok()?;
    let sha = output.stdout.trim();
    (output.ok && !sha.is_empty()).then(|| sha.to_string())
}

pub(super) fn ancestor(host: &Host, root: &Path, ancestor_sha: &str, descendant: &str) -> bool {
    is_ancestor(host, root, ancestor_sha, descendant).unwrap_or(false)
}

/// Whether the commit is still in the object database (not collected).
pub(super) fn has_commit(host: &Host, root: &Path, sha: &str) -> bool {
    let spec = format!("{sha}^{{commit}}");
    run_git(host, root, &["cat-file", "-e", &spec], READ_TIMEOUT)
        .map(|output| output.ok)
        .unwrap_or(false)
}

/// The empty tree's id in this repository's hash, for diffing an unborn HEAD.
pub(super) fn empty_tree(host: &Host, root: &Path) -> Result<String, String> {
    let output = run_git_with_input(host, root, &["hash-object", "-t", "tree", "--stdin"], "")?;
    let sha = output.stdout.trim();
    if !output.ok || sha.is_empty() {
        return Err("Could not read the repository".to_string());
    }
    Ok(sha.to_string())
}

/// `rebase` or `merge` while one is stopped in `root`.
pub(super) fn in_progress(host: &Host, root: &Path) -> Option<String> {
    let output = run_git(
        host,
        root,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "rebase-merge",
            "--git-path",
            "rebase-apply",
            "--git-path",
            "MERGE_HEAD",
            "--git-path",
            "CHERRY_PICK_HEAD",
        ],
        READ_TIMEOUT,
    )
    .ok()?;
    if !output.ok {
        return None;
    }
    let paths: Vec<&str> = output.stdout.lines().collect();
    let exists = |index: usize| {
        paths
            .get(index)
            .is_some_and(|path| host.to_local(path).exists())
    };
    if exists(0) || exists(1) {
        Some("rebase".into())
    } else if exists(2) {
        Some("merge".into())
    } else if exists(3) {
        Some("cherry-pick".into())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_heads_remotes_upstreams_and_remote_heads() {
        let out = "refs/heads/feat\0aaa\0refs/remotes/origin/feat\0\n\
                   refs/heads/main\0bbb\0\0\n\
                   refs/remotes/origin/HEAD\0bbb\0\0refs/remotes/origin/main\n\
                   refs/remotes/origin/main\0bbb\0\0\n\
                   refs/remotes/upstream/trunk\0ccc\0\0\n";
        let refs = Refs::parse(out);
        assert_eq!(refs.heads.get("feat").map(String::as_str), Some("aaa"));
        assert_eq!(
            refs.upstream.get("feat").map(String::as_str),
            Some("origin/feat")
        );
        assert_eq!(
            refs.remote_heads.get("origin").map(String::as_str),
            Some("origin/main")
        );
        assert!(!refs.remotes.contains_key("origin/HEAD"));
        assert_eq!(
            refs.preferred_remote(Some("feat")).as_deref(),
            Some("origin")
        );
        assert_eq!(
            refs.preferred_remote(Some("main")).as_deref(),
            Some("origin")
        );
        assert_eq!(
            refs.default_branch(Some("origin")),
            Some(("main".into(), "origin/main".into(), "bbb".into()))
        );
        assert_eq!(
            refs.default_branch(Some("upstream")),
            Some(("main".into(), "main".into(), "bbb".into()))
        );
    }
}
