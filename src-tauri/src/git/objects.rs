//! Reading pull requests from the object database, by SHA.
//!
//! A PR's commits are fetched into hidden refs (`refs/pingex/pr/<n>/head`)
//! without touching any working tree, and every read after that names commits
//! by SHA rather than by branch. A SHA never changes what it points at, so a
//! read is correct whenever it ran and can be cached forever; a force-push
//! shows up as a new SHA, never as a silently different answer.
//!
//! The head a reviewer last looked at is pinned under
//! `refs/pingex/pr/<n>/seen/<sha>`, so its commits survive a force-push and
//! `git gc` and "changes since your review" still has both sides.
//!
//! Ref writes go through `update-ref --stdin` transactions under the
//! repository's common-dir lock. Rationale: `docs/adr/0004-git-and-github-io.md`.

use std::path::Path;

use super::run::{
    common_dir_of, lock_for_common_dir, run_git, run_git_capped, run_git_network,
    run_git_with_input, READ_TIMEOUT,
};
use crate::util::host::Host;

/// Where Pingex keeps a repository's PR refs. Outside `refs/heads` and
/// `refs/remotes`, so they never show as branches, and never pushed.
pub(crate) const PR_REF_ROOT: &str = "refs/pingex/pr";

/// The largest patch or blob read into memory; bigger ones are truncated.
pub(crate) const MAX_READ_BYTES: usize = 8 * 1024 * 1024;

pub(crate) fn head_ref(number: u32) -> String {
    format!("{PR_REF_ROOT}/{number}/head")
}

fn seen_prefix(number: u32) -> String {
    format!("{PR_REF_ROOT}/{number}/seen/")
}

/// Refuse anything but a full hex object name. Reads are keyed on SHAs, and a
/// value starting with `-` would otherwise reach git as an option.
pub(crate) fn require_sha(value: &str) -> Result<&str, String> {
    let ok = matches!(value.len(), 40 | 64) && value.bytes().all(|b| b.is_ascii_hexdigit());
    if ok {
        Ok(value)
    } else {
        Err("Expected a full commit SHA".to_string())
    }
}

fn require_remote(remote: &str) -> Result<&str, String> {
    let ok = !remote.is_empty()
        && !remote.starts_with('-')
        && remote
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'/'));
    if ok {
        Ok(remote)
    } else {
        Err("That is not a remote name".to_string())
    }
}

/// `owner/name` from a GitHub remote URL, in any of the spellings git accepts:
/// `https://github.com/o/n(.git)`, `git@github.com:o/n.git`, an SSH host
/// alias (`git@github-personal:o/n.git`) or `ssh://git@host/o/n`.
pub(crate) fn url_slug(url: &str) -> Option<String> {
    let url = url.trim().trim_end_matches('/');
    let url = url.strip_suffix(".git").unwrap_or(url);
    let path = match url.split_once("://") {
        Some((_, rest)) => rest.split_once('/')?.1,
        None => url.rsplit_once(':')?.1,
    };
    let mut parts = path.rsplitn(3, '/');
    let name = parts.next()?;
    let owner = parts.next()?;
    (!owner.is_empty() && !name.is_empty()).then(|| format!("{owner}/{name}").to_lowercase())
}

/// The remote whose URL points at GitHub repository `slug` (`owner/name`):
/// where a PR's `refs/pull/<n>/head` lives. `None` when no remote does, for
/// example a fork clone with no `upstream`.
pub(crate) fn remote_for_slug(
    host: &Host,
    repo: &Path,
    slug: &str,
) -> Result<Option<String>, String> {
    let output = run_git(
        host,
        repo,
        &["config", "--null", "--get-regexp", r"^remote\..*\.url$"],
        READ_TIMEOUT,
    )?;
    // Exit 1: no remotes at all.
    if !output.ok && output.code != Some(1) {
        return Err("Could not read the repository's remotes".to_string());
    }
    let wanted = slug.to_lowercase();
    // `--null`: each entry is `remote.<name>.url\n<url>\0`.
    Ok(output.stdout.split('\0').find_map(|entry| {
        let (key, url) = entry.split_once('\n')?;
        let name = key.strip_prefix("remote.")?.strip_suffix(".url")?;
        (url_slug(url).as_deref() == Some(wanted.as_str())).then(|| name.to_string())
    }))
}

/// Run `work` holding the repository's mutation lock.
fn with_lock<T>(
    host: &Host,
    repo: &Path,
    work: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let common = common_dir_of(host, repo)?;
    let lock = lock_for_common_dir(&common);
    let _guard = lock.lock().expect("git common-dir lock poisoned");
    work()
}

/// What fetching one PR's head produced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FetchedHead {
    pub(crate) number: u32,
    /// The head SHA now at `refs/pingex/pr/<n>/head`; `None` when the remote
    /// has no such PR.
    pub(crate) sha: Option<String>,
}

/// Fetch the heads of `numbers` from `remote` into `refs/pingex/pr/<n>/head`,
/// in one network round trip. `remote` must be the remote for the PR's *base*
/// repository; GitHub publishes `refs/pull/<n>/head` there for fork PRs too.
///
/// One missing PR fails a batched fetch outright, so on failure each number
/// is retried alone and reported with `sha: None` if it is still refused.
pub(crate) fn fetch_pr_heads(
    host: &Host,
    repo: &Path,
    remote: &str,
    numbers: &[u32],
) -> Result<Vec<FetchedHead>, String> {
    let remote = require_remote(remote)?;
    if numbers.is_empty() {
        return Ok(Vec::new());
    }
    let refspec = |n: u32| format!("+refs/pull/{n}/head:{}", head_ref(n));
    let fetch = |specs: Vec<String>| -> Result<bool, String> {
        let mut args = vec![
            "fetch",
            "--no-tags",
            "--no-write-fetch-head",
            "--quiet",
            remote,
        ];
        args.extend(specs.iter().map(String::as_str));
        let output = run_git_network(host, repo, &args)?;
        if output.ok {
            return Ok(true);
        }
        if output.stderr.contains("couldn't find remote ref") {
            return Ok(false);
        }
        Err(super::run::redact_classified(
            "Could not fetch the pull requests",
            &output,
        ))
    };
    with_lock(host, repo, || {
        if !fetch(numbers.iter().map(|&n| refspec(n)).collect())? {
            for &n in numbers {
                fetch(vec![refspec(n)])?;
            }
        }
        Ok(())
    })?;
    numbers
        .iter()
        .map(|&number| {
            Ok(FetchedHead {
                number,
                sha: resolve(host, repo, &head_ref(number))?,
            })
        })
        .collect()
}

/// The SHA a ref points at, or `None` when it does not exist. Branches on the
/// exit code, not stderr.
pub(crate) fn resolve(host: &Host, repo: &Path, refname: &str) -> Result<Option<String>, String> {
    let spec = format!("{refname}^{{commit}}");
    let output = run_git(
        host,
        repo,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            "--end-of-options",
            &spec,
        ],
        READ_TIMEOUT,
    )?;
    match output.code {
        Some(0) => Ok(Some(output.stdout.trim().to_string())),
        Some(1) => Ok(None),
        _ => Err("Could not read the repository".to_string()),
    }
}

/// Whether `ancestor` is reachable from `descendant`.
pub(crate) fn is_ancestor(
    host: &Host,
    repo: &Path,
    ancestor: &str,
    descendant: &str,
) -> Result<bool, String> {
    let (ancestor, descendant) = (require_sha(ancestor)?, require_sha(descendant)?);
    let output = run_git(
        host,
        repo,
        &["merge-base", "--is-ancestor", ancestor, descendant],
        READ_TIMEOUT,
    )?;
    match output.code {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Err("Those commits are not in this repository yet".to_string()),
    }
}

/// A patch read from git, possibly cut at `MAX_READ_BYTES`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Patch {
    pub(crate) text: String,
    pub(crate) truncated: bool,
}

fn read_patch(host: &Host, repo: &Path, args: &[&str]) -> Result<Patch, String> {
    let output = run_git_capped(host, repo, args, MAX_READ_BYTES)?;
    if !output.ok && !output.truncated {
        return Err("Those commits are not in this repository yet".to_string());
    }
    let mut text = output.stdout;
    if output.truncated {
        if let Some(cut) = text.rfind('\n') {
            text.truncate(cut + 1);
        }
    }
    Ok(Patch {
        text,
        truncated: output.truncated,
    })
}

/// The PR diff GitHub shows: `head` against its merge base with `base`.
/// External diff drivers and textconv filters are off, so the patch is the
/// same whatever the user's gitattributes and config say.
pub(crate) fn diff_shas(host: &Host, repo: &Path, base: &str, head: &str) -> Result<Patch, String> {
    let (base, head) = (require_sha(base)?, require_sha(head)?);
    read_patch(
        host,
        repo,
        &[
            "diff",
            "--merge-base",
            "--no-ext-diff",
            "--no-textconv",
            "-M",
            base,
            head,
            "--",
        ],
    )
}

/// The straight diff between two commits, for "changes since" when the old
/// head is an ancestor of the new one.
pub(crate) fn diff_between(
    host: &Host,
    repo: &Path,
    from: &str,
    to: &str,
) -> Result<Patch, String> {
    let (from, to) = (require_sha(from)?, require_sha(to)?);
    read_patch(
        host,
        repo,
        &[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "-M",
            from,
            to,
            "--",
        ],
    )
}

/// `git range-diff <base> <old> <new>`: commit-by-commit, how the PR's
/// commits changed across a force-push or rebase. `base` is the current tip
/// of the PR's base branch.
pub(crate) fn range_diff(
    host: &Host,
    repo: &Path,
    base: &str,
    old: &str,
    new: &str,
) -> Result<Patch, String> {
    let (base, old, new) = (require_sha(base)?, require_sha(old)?, require_sha(new)?);
    read_patch(host, repo, &["range-diff", "--no-color", base, old, new])
}

/// One file's contents at a commit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Blob {
    /// `None` for a binary file.
    pub(crate) text: Option<String>,
    pub(crate) size: u64,
    pub(crate) truncated: bool,
}

/// `path` as it is in commit `sha`, without checking it out.
pub(crate) fn read_blob(host: &Host, repo: &Path, sha: &str, path: &str) -> Result<Blob, String> {
    let sha = require_sha(sha)?;
    if path.is_empty() || path.starts_with('/') || path.split('/').any(|part| part == "..") {
        return Err("Expected a path inside the repository".to_string());
    }
    let spec = format!("{sha}:{path}");
    let size = run_git(host, repo, &["cat-file", "-s", &spec], READ_TIMEOUT)?;
    if !size.ok {
        return Err("That file is not in this commit".to_string());
    }
    let size: u64 = size.stdout.trim().parse().unwrap_or(0);
    let output = run_git_capped(host, repo, &["cat-file", "blob", &spec], MAX_READ_BYTES)?;
    if !output.ok && !output.truncated {
        return Err("That file is not in this commit".to_string());
    }
    // Git's own heuristic: a NUL in the first 8000 bytes means binary.
    let binary = output.stdout.bytes().take(8000).any(|b| b == 0);
    Ok(Blob {
        text: (!binary).then_some(output.stdout),
        size,
        truncated: output.truncated,
    })
}

/// Refs under `prefix`, as `(refname, sha)`.
fn list_refs(host: &Host, repo: &Path, prefix: &str) -> Result<Vec<(String, String)>, String> {
    let output = run_git(
        host,
        repo,
        &[
            "for-each-ref",
            "--format=%(refname)%00%(objectname)",
            prefix,
        ],
        READ_TIMEOUT,
    )?;
    if !output.ok {
        return Err("Could not read the repository".to_string());
    }
    Ok(output
        .stdout
        .lines()
        .filter_map(|line| {
            let (name, sha) = line.split_once('\0')?;
            Some((name.to_string(), sha.to_string()))
        })
        .collect())
}

/// The head the reviewer last marked seen on PR `number`.
pub(crate) fn seen_head(host: &Host, repo: &Path, number: u32) -> Result<Option<String>, String> {
    Ok(list_refs(host, repo, &seen_prefix(number))?
        .into_iter()
        .next()
        .map(|(_, sha)| sha))
}

fn update_refs(host: &Host, repo: &Path, instructions: &str) -> Result<(), String> {
    let output = run_git_with_input(host, repo, &["update-ref", "--stdin"], instructions)?;
    if output.ok {
        Ok(())
    } else {
        Err("Could not update the pull request's refs".to_string())
    }
}

/// Pin `sha` as the head last seen on PR `number`, replacing any earlier pin,
/// in one atomic transaction.
pub(crate) fn mark_seen(host: &Host, repo: &Path, number: u32, sha: &str) -> Result<(), String> {
    let sha = require_sha(sha)?;
    with_lock(host, repo, || {
        let mut tx = String::from("start\n");
        for (name, _) in list_refs(host, repo, &seen_prefix(number))? {
            tx.push_str(&format!("delete {name}\n"));
        }
        tx.push_str(&format!(
            "update {}{sha} {sha}\nprepare\ncommit\n",
            seen_prefix(number)
        ));
        update_refs(host, repo, &tx)
    })
}

/// Drop every ref Pingex keeps for PR `number`, once it has closed.
pub(crate) fn prune_pr_refs(host: &Host, repo: &Path, number: u32) -> Result<(), String> {
    with_lock(host, repo, || {
        let refs = list_refs(host, repo, &format!("{PR_REF_ROOT}/{number}/"))?;
        if refs.is_empty() {
            return Ok(());
        }
        let mut tx = String::from("start\n");
        for (name, _) in refs {
            tx.push_str(&format!("delete {name}\n"));
        }
        tx.push_str("prepare\ncommit\n");
        update_refs(host, repo, &tx)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::test_repo::TestRepo;

    const HOST: Host = Host::Native;

    #[test]
    fn reads_the_slug_from_every_remote_spelling() {
        for url in [
            "https://github.com/Pingu-Codes/pingex.git",
            "https://github.com/pingu-codes/pingex",
            "git@github.com:pingu-codes/pingex.git",
            "git@github-personal:pingu-codes/pingex.git",
            "ssh://git@github.com/pingu-codes/pingex.git/",
        ] {
            assert_eq!(
                url_slug(url).as_deref(),
                Some("pingu-codes/pingex"),
                "{url}"
            );
        }
        assert_eq!(url_slug("/local/path"), None);
    }

    #[test]
    fn finds_the_remote_for_a_repository() {
        let Some(fixture) = TestRepo::new() else {
            return;
        };
        fixture.git(&[
            "remote",
            "add",
            "upstream",
            "git@github.com:openai/codex.git",
        ]);
        let found = remote_for_slug(&HOST, &fixture.repo, "OpenAI/Codex").unwrap();
        assert_eq!(found.as_deref(), Some("upstream"));
        assert_eq!(
            remote_for_slug(&HOST, &fixture.repo, "o/missing").unwrap(),
            None
        );
    }

    #[test]
    fn only_full_shas_are_accepted() {
        assert!(require_sha(&"a".repeat(40)).is_ok());
        assert!(require_sha(&"0".repeat(64)).is_ok());
        assert!(require_sha("HEAD").is_err());
        assert!(require_sha("abc123").is_err());
        assert!(require_sha(&format!("--{}", "a".repeat(38))).is_err());
        assert!(require_remote("origin").is_ok());
        assert!(require_remote("--upload-pack=x").is_err());
    }

    /// A PR on the remote, a fork-style head nobody has as a branch locally.
    fn repo_with_pr() -> Option<(TestRepo, String, String)> {
        let fixture = TestRepo::new()?;
        let base = fixture.head();
        fixture.branch("feature");
        fixture.commit("src/lib.rs", "pub fn one() {}\n", "one");
        let head = fixture.open_pr(7, "feature");
        fixture.switch("main");
        fixture.git(&["branch", "-D", "feature"]);
        Some((fixture, base, head))
    }

    #[test]
    fn fetches_pr_heads_into_hidden_refs_without_touching_the_worktree() {
        let Some((fixture, base, head)) = repo_with_pr() else {
            return;
        };
        let fetched = fetch_pr_heads(&HOST, &fixture.repo, "origin", &[7]).unwrap();
        assert_eq!(
            fetched,
            vec![FetchedHead {
                number: 7,
                sha: Some(head.clone())
            }]
        );
        assert_eq!(fixture.head(), base, "HEAD is untouched");
        assert!(
            !fixture.repo.join("src/lib.rs").exists(),
            "nothing checked out"
        );
        assert!(
            fixture.git(&["branch", "--list"]).lines().count() == 1,
            "no branch created"
        );

        let patch = diff_shas(&HOST, &fixture.repo, &base, &head).unwrap();
        assert!(patch.text.contains("+pub fn one() {}"), "{}", patch.text);
        let files = crate::review::diff::parse_git_diff(&patch.text);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "src/lib.rs");
        assert_eq!(files[0].status, "added");

        let blob = read_blob(&HOST, &fixture.repo, &head, "src/lib.rs").unwrap();
        assert_eq!(blob.text.as_deref(), Some("pub fn one() {}\n"));
        assert!(read_blob(&HOST, &fixture.repo, &head, "../etc/passwd").is_err());
        assert!(read_blob(&HOST, &fixture.repo, &head, "missing.rs").is_err());
    }

    #[test]
    fn a_missing_pr_does_not_fail_the_others() {
        let Some((fixture, _, head)) = repo_with_pr() else {
            return;
        };
        let fetched = fetch_pr_heads(&HOST, &fixture.repo, "origin", &[7, 99]).unwrap();
        assert_eq!(fetched[0].sha.as_deref(), Some(head.as_str()));
        assert_eq!(
            fetched[1],
            FetchedHead {
                number: 99,
                sha: None
            }
        );
    }

    #[test]
    fn a_seen_head_survives_a_force_push_and_gc() {
        let Some((fixture, base, first)) = repo_with_pr() else {
            return;
        };
        fetch_pr_heads(&HOST, &fixture.repo, "origin", &[7]).unwrap();
        mark_seen(&HOST, &fixture.repo, 7, &first).unwrap();

        // The author rewrites the PR: same file, different commit.
        fixture.git(&["switch", "-q", "-c", "feature", &base]);
        fixture.commit("src/lib.rs", "pub fn one() -> u8 { 1 }\n", "one, typed");
        let second = fixture.open_pr(7, "feature");
        fixture.switch("main");
        fixture.git(&["branch", "-D", "feature"]);
        fetch_pr_heads(&HOST, &fixture.repo, "origin", &[7]).unwrap();

        fixture.git(&["reflog", "expire", "--expire=now", "--all"]);
        fixture.git(&["gc", "--quiet", "--prune=now"]);

        assert_eq!(
            seen_head(&HOST, &fixture.repo, 7).unwrap().as_deref(),
            Some(first.as_str())
        );
        assert!(!is_ancestor(&HOST, &fixture.repo, &first, &second).unwrap());
        let range = range_diff(&HOST, &fixture.repo, &base, &first, &second).unwrap();
        assert!(range.text.contains("one, typed"), "{}", range.text);
        let blob = read_blob(&HOST, &fixture.repo, &first, "src/lib.rs").unwrap();
        assert_eq!(
            blob.text.as_deref(),
            Some("pub fn one() {}\n"),
            "old commit kept"
        );

        // Marking the new head replaces the old pin.
        mark_seen(&HOST, &fixture.repo, 7, &second).unwrap();
        assert_eq!(
            seen_head(&HOST, &fixture.repo, 7).unwrap().as_deref(),
            Some(second.as_str())
        );
        assert_eq!(
            list_refs(&HOST, &fixture.repo, &seen_prefix(7))
                .unwrap()
                .len(),
            1
        );

        prune_pr_refs(&HOST, &fixture.repo, 7).unwrap();
        assert!(list_refs(&HOST, &fixture.repo, "refs/pingex/")
            .unwrap()
            .is_empty());
    }

    #[test]
    fn a_fast_forward_reads_as_a_plain_diff() {
        let Some((fixture, _, first)) = repo_with_pr() else {
            return;
        };
        fixture.git(&["switch", "-q", "-c", "feature", &first]);
        fixture.commit("src/lib.rs", "pub fn one() {}\npub fn two() {}\n", "two");
        let second = fixture.open_pr(7, "feature");
        fetch_pr_heads(&HOST, &fixture.repo, "origin", &[7]).unwrap();
        assert!(is_ancestor(&HOST, &fixture.repo, &first, &second).unwrap());
        let patch = diff_between(&HOST, &fixture.repo, &first, &second).unwrap();
        assert!(patch.text.contains("+pub fn two() {}"));
        assert!(!patch.text.contains("+pub fn one() {}"));
    }

    #[test]
    fn a_huge_patch_is_truncated_on_a_line_boundary() {
        let Some(fixture) = TestRepo::new() else {
            return;
        };
        let base = fixture.head();
        let line = "x".repeat(99) + "\n";
        let head = fixture.commit("big.txt", &line.repeat(MAX_READ_BYTES / 100 + 1000), "big");
        let patch = diff_shas(&HOST, &fixture.repo, &base, &head).unwrap();
        assert!(patch.truncated);
        assert!(patch.text.len() <= MAX_READ_BYTES);
        assert!(patch.text.ends_with('\n'));
    }

    #[test]
    fn git_messages_stay_english_whatever_the_locale() {
        // `GIT_ENV` pins LC_ALL=C for every call, so a German shell cannot
        // turn "couldn't find remote ref" into something the classifier
        // misses. The run inherits this process's LANG either way.
        let Some((fixture, _, _)) = repo_with_pr() else {
            return;
        };
        let output = run_git_network(
            &HOST,
            &fixture.repo,
            &["fetch", "origin", "refs/pull/404/head"],
        )
        .unwrap();
        assert!(!output.ok);
        assert!(
            output.stderr.contains("couldn't find remote ref"),
            "{}",
            output.stderr
        );
    }
}
