//! A three-PR stack of mine for the stack tests, bottom first:
//! #412 `feat/driver` (driver.rs) on `main`, #415 `feat/claude` (claude.rs)
//! on it, #418 `feat/maki` (maki.rs) on that. Each git branch is pushed and
//! listed on the fake GitHub with its base.

use std::path::{Path, PathBuf};

use super::test_support::{fixture, Fixture, VIEWER};
use super::ReviewDesk;
use crate::review::stack_types::{StackInput, StackInputPr};
use crate::review::types::InboxPr;

pub(super) const STACK: [(i64, &str, &str); 3] = [
    (412, "feat/driver", "driver.rs"),
    (415, "feat/claude", "claude.rs"),
    (418, "feat/maki", "maki.rs"),
];

pub(super) struct StackFixture {
    pub(super) inner: Fixture,
    /// Merged PRs, by number, as GitHub's stack listing keeps them.
    merged: std::sync::Mutex<Vec<i64>>,
}

impl StackFixture {
    pub(super) async fn new() -> Option<Self> {
        let inner = fixture().await?;
        let mut base = "main".to_string();
        for (number, branch, path) in STACK {
            inner.repo.switch(&base);
            inner.repo.branch(branch);
            inner.repo.commit(
                path,
                &format!("// {path} from #{number}\n"),
                &format!("add {path}"),
            );
            let head = inner.repo.open_pr(number as u32, branch);
            inner.github.put_pr(InboxPr {
                number,
                title: format!("Add {path}"),
                author: VIEWER.to_string(),
                head_ref: branch.to_string(),
                head_sha: head,
                base_ref: base.clone(),
                updated_at: "2026-11-05T10:00:00Z".to_string(),
                ..InboxPr::default()
            });
            base = branch.to_string();
        }
        inner.repo.switch("main");
        Some(Self {
            inner,
            merged: Default::default(),
        })
    }

    pub(super) fn desk(&self) -> &ReviewDesk {
        &self.inner.desk
    }

    pub(super) fn git(&self, args: &[&str]) -> String {
        self.inner.repo.git(args)
    }

    pub(super) fn git_in(&self, dir: &Path, args: &[&str]) -> String {
        self.inner.repo.git_in(dir, args)
    }

    /// The local git branch's tip.
    pub(super) fn tip(&self, branch: &str) -> String {
        self.git(&["rev-parse", &format!("refs/heads/{branch}")])
    }

    /// What the remote (GitHub) has for `branch`.
    pub(super) fn remote_tip(&self, branch: &str) -> String {
        self.git_in(
            &self.inner.repo.remote,
            &["rev-parse", &format!("refs/heads/{branch}")],
        )
    }

    /// Commit `text` to `path` on `main` (as both content and subject) and
    /// push it.
    pub(super) fn advance_main(&self, path: &str, text: &str) -> String {
        self.inner.repo.switch("main");
        let sha = self.inner.repo.commit(path, text, text.trim());
        self.git(&["push", "-q", "origin", "main"]);
        sha
    }

    /// A linked worktree with `branch` checked out.
    pub(super) fn worktree_on(&self, branch: &str) -> PathBuf {
        let path = self
            .inner
            .repo
            .repo
            .parent()
            .unwrap()
            .join(format!("wt-{}", branch.replace('/', "-")));
        self.git(&["worktree", "add", "-q", path.to_str().unwrap(), branch]);
        path
    }

    /// Run `work` in a fresh clone of the remote, as a teammate on another
    /// machine: nothing it does reaches this repository until it fetches.
    pub(super) fn as_teammate(&self, work: impl FnOnce(&Path)) {
        let temp = tempfile::tempdir().unwrap();
        let clone = temp.path().join("teammate");
        let remote = self.inner.repo.remote.to_str().unwrap().to_string();
        self.git(&["clone", "-q", &remote, clone.to_str().unwrap()]);
        work(&clone);
    }

    /// A teammate pushes one commit on top of PR `number`'s git branch, and
    /// GitHub moves the PR head with it. Returns the new head.
    pub(super) fn teammate_pushes(&self, number: i64, branch: &str) -> String {
        let mut head = String::new();
        self.as_teammate(|clone| {
            self.git_in(clone, &["switch", "-q", branch]);
            self.git_in(
                clone,
                &["commit", "-q", "--allow-empty", "-m", "teammate's commit"],
            );
            self.git_in(clone, &["push", "-q", "origin", branch]);
            head = self.git_in(clone, &["rev-parse", "HEAD"]);
        });
        self.git_in(
            &self.inner.repo.remote,
            &["update-ref", &format!("refs/pull/{number}/head"), &head],
        );
        head
    }

    /// Mark PR `number` merged, as GitHub's stack listing then shows it.
    pub(super) fn mark_merged(&self, number: i64) {
        self.merged.lock().unwrap().push(number);
    }

    /// The stack as the inbox would hand it over now.
    pub(super) fn input(&self) -> StackInput {
        let merged = self.merged.lock().unwrap().clone();
        StackInput {
            number: 7,
            name: "driver".to_string(),
            base_ref: "main".to_string(),
            base_sha: self.remote_tip("main"),
            prs: STACK
                .iter()
                .map(|(number, branch, path)| StackInputPr {
                    number: *number,
                    title: format!("Add {path}"),
                    head_ref: branch.to_string(),
                    head_sha: self.remote_tip(branch),
                    open: !merged.contains(number),
                    merged: merged.contains(number),
                })
                .collect(),
        }
    }

    /// Whether `ancestor` is in `branch`'s history.
    pub(super) fn has_ancestor(&self, branch: &str, ancestor: &str) -> bool {
        crate::git::test_repo::git_ok(
            &self.inner.repo.repo,
            &[
                "merge-base",
                "--is-ancestor",
                ancestor,
                &format!("refs/heads/{branch}"),
            ],
        )
    }

    /// The subjects of `branch`'s commits over `base`, oldest first.
    pub(super) fn subjects(&self, base: &str, branch: &str) -> Vec<String> {
        self.git(&[
            "log",
            "--reverse",
            "--format=%s",
            &format!("{base}..{branch}"),
        ])
        .lines()
        .map(str::to_string)
        .collect()
    }
}
