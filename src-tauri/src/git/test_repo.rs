//! Real repositories for tests: a working clone and a bare remote that stands
//! in for GitHub, including the `refs/pull/<n>/head` refs GitHub publishes.
//!
//! Every git call here, and every `run_git` in a test build, runs with the
//! developer's own git configuration shut out (`TEST_GIT_ENV`). Without that,
//! settings like `commit.gpgsign = true` make `git commit` block on a
//! passphrase prompt it can never receive. Never isolate with
//! `std::env::set_var`: it changes the environment of every test running in
//! parallel.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Isolation applied on top of `run::GIT_ENV` in test builds.
pub(crate) const TEST_GIT_ENV: [(&str, &str); 6] = [
    ("GIT_CONFIG_GLOBAL", "/dev/null"),
    ("GIT_CONFIG_SYSTEM", "/dev/null"),
    ("GIT_AUTHOR_NAME", "Test"),
    ("GIT_AUTHOR_EMAIL", "test@example.com"),
    ("GIT_COMMITTER_NAME", "Test"),
    ("GIT_COMMITTER_EMAIL", "test@example.com"),
];

pub(crate) fn git_available() -> bool {
    Command::new("git")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// Run git in `dir` with the test isolation, returning whether it succeeded.
/// For setup steps whose output does not matter.
pub(crate) fn git_ok(dir: &Path, args: &[&str]) -> bool {
    command(dir, args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn command(dir: &Path, args: &[&str]) -> Command {
    let mut command = Command::new("git");
    command.arg("-C").arg(dir).args(args).stdin(Stdio::null());
    for (key, value) in super::run::GIT_ENV.iter().chain(TEST_GIT_ENV.iter()) {
        command.env(key, value);
    }
    command
}

/// A working repository on `main` with one commit, and a bare `origin`.
pub(crate) struct TestRepo {
    /// Holds both repositories; deleted on drop.
    _temp: tempfile::TempDir,
    /// The working clone the code under test reads.
    pub(crate) repo: PathBuf,
    /// The bare remote, playing GitHub.
    pub(crate) remote: PathBuf,
}

impl TestRepo {
    /// `None` when git is not installed, so callers can skip quietly.
    pub(crate) fn new() -> Option<Self> {
        if !git_available() {
            eprintln!("skipping: git not available");
            return None;
        }
        let temp = tempfile::tempdir().expect("temp dir");
        let repo = temp.path().join("repo");
        let remote = temp.path().join("remote.git");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::create_dir_all(&remote).unwrap();
        let fixture = Self {
            _temp: temp,
            repo,
            remote,
        };
        fixture.git_in(&fixture.remote, &["init", "-q", "--bare", "-b", "main"]);
        fixture.git(&["init", "-q", "-b", "main"]);
        fixture.git(&["remote", "add", "origin", fixture.remote.to_str().unwrap()]);
        fixture.commit("README.md", "hello\n", "init");
        fixture.git(&["push", "-q", "origin", "main"]);
        Some(fixture)
    }

    /// Run git in the working repository; panics with stderr on failure and
    /// returns trimmed stdout.
    pub(crate) fn git(&self, args: &[&str]) -> String {
        self.git_in(&self.repo, args)
    }

    pub(crate) fn git_in(&self, dir: &Path, args: &[&str]) -> String {
        let output = command(dir, args).output().expect("git runs");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    /// Write `path`, commit it on the current branch and return the new SHA.
    pub(crate) fn commit(&self, path: &str, contents: &str, message: &str) -> String {
        let file = self.repo.join(path);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(file, contents).unwrap();
        self.git(&["add", "--", path]);
        self.git(&["commit", "-q", "-m", message]);
        self.head()
    }

    pub(crate) fn head(&self) -> String {
        self.git(&["rev-parse", "HEAD"])
    }

    /// Create `name` at the current commit and switch to it.
    pub(crate) fn branch(&self, name: &str) {
        self.git(&["switch", "-q", "-c", name]);
    }

    pub(crate) fn switch(&self, name: &str) {
        self.git(&["switch", "-q", name]);
    }

    /// Publish `branch` as pull request `number`: push it, then point the
    /// remote's `refs/pull/<number>/head` at it the way GitHub does. Returns the
    /// head SHA.
    pub(crate) fn open_pr(&self, number: u32, branch: &str) -> String {
        self.git(&["push", "-q", "--force", "origin", branch]);
        let sha = self.git(&["rev-parse", branch]);
        self.git_in(
            &self.remote,
            &["update-ref", &format!("refs/pull/{number}/head"), &sha],
        );
        sha
    }

    /// Make `origin` read as GitHub repository `slug` (`owner/name`) to code
    /// that matches remote URLs, while fetches still reach the bare remote
    /// through `url.<remote>.insteadOf`.
    pub(crate) fn serve_as_github(&self, slug: &str) {
        let url = format!("https://github.com/{slug}.git");
        let remote = self.remote.to_str().unwrap();
        self.git(&["config", &format!("url.{remote}.insteadOf"), &url]);
        self.git(&["remote", "set-url", "origin", &url]);
    }
}
