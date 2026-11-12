//! The fixture every desk test starts from: a clone whose `origin` reads as
//! GitHub repository `SLUG`, a fake GitHub for it, and a desk over both.

use std::sync::Arc;

use super::ReviewDesk;
use crate::git::test_repo::TestRepo;
use crate::review::checkout_types::{CheckOutOutcome, PrCheckout};
use crate::review::fake::FakeGitHub;
use crate::review::types::InboxPr;
use crate::util::host::Host;

pub(super) const SLUG: &str = "pingex/desk";
pub(super) const VIEWER: &str = "pingu-codes";

pub(super) struct Fixture {
    pub(super) repo: TestRepo,
    pub(super) github: Arc<FakeGitHub>,
    pub(super) desk: ReviewDesk,
    _storage: tempfile::TempDir,
}

/// `None` when git is not installed.
pub(super) async fn fixture() -> Option<Fixture> {
    let repo = TestRepo::new()?;
    repo.serve_as_github(SLUG);
    let github = Arc::new(FakeGitHub::new(SLUG, VIEWER));
    let storage = tempfile::tempdir().unwrap();
    let database = crate::storage::open(storage.path()).await.unwrap();
    let home = storage.path().join("home").display().to_string();
    let desk = ReviewDesk::new(Host::Native, repo.repo.clone(), github.clone(), database)
        .with_home_dir(home);
    Some(Fixture {
        repo,
        github,
        desk,
        _storage: storage,
    })
}

impl Fixture {
    /// Open PR `number` by `jo-r` from a new git branch off `main` with one
    /// commit writing `path`, and list it on the fake GitHub. Leaves `main`
    /// checked out.
    pub(super) fn open_pr(&self, number: u32, branch: &str, path: &str) -> InboxPr {
        let base = self.repo.git(&["rev-parse", "main"]);
        self.repo.branch(branch);
        self.repo.commit(path, "pub fn one() {}\n", "one");
        let head = self.repo.open_pr(number, branch);
        self.repo.switch("main");
        let pr = InboxPr {
            number: i64::from(number),
            title: format!("PR {number}"),
            author: "jo-r".to_string(),
            head_ref: branch.to_string(),
            head_sha: head,
            base_ref: "main".to_string(),
            base_sha: base,
            head_repo: Some(SLUG.to_string()),
            updated_at: "2026-11-05T10:00:00Z".to_string(),
            ..InboxPr::default()
        };
        self.github.put_pr(pr.clone());
        pr
    }

    /// `open_pr`, written by the viewer.
    pub(super) fn open_my_pr(&self, number: u32, branch: &str, path: &str) -> InboxPr {
        let pr = InboxPr {
            author: VIEWER.to_string(),
            ..self.open_pr(number, branch, path)
        };
        self.github.put_pr(pr.clone());
        pr
    }

    /// Add an empty commit to PR `number`'s git branch on GitHub only, as a
    /// teammate pushing, wherever that branch is checked out here. Returns
    /// the new head.
    pub(super) fn github_moves(&self, number: u32, branch: &str) -> String {
        let remote = &self.repo.remote;
        let tip = self
            .repo
            .git_in(remote, &["rev-parse", &format!("refs/heads/{branch}")]);
        let tree = self.repo.git(&["rev-parse", &format!("{tip}^{{tree}}")]);
        let sha = self
            .repo
            .git(&["commit-tree", &tree, "-p", &tip, "-m", "teammate's commit"]);
        self.repo.git(&[
            "push",
            "-q",
            "origin",
            &format!("{sha}:refs/heads/{branch}"),
        ]);
        self.repo.git_in(
            remote,
            &["update-ref", &format!("refs/pull/{number}/head"), &sha],
        );
        sha
    }

    /// List `pr` on the fake GitHub as it is now, e.g. at a new head.
    pub(super) fn relist(&self, pr: &InboxPr) {
        self.github.put_pr(pr.clone());
    }

    /// Sync the inbox, so the desk knows the PRs as GitHub lists them now,
    /// then check PR `number` out.
    pub(super) async fn check_out_now(&self, number: i64) -> CheckOutOutcome {
        self.desk.sync_inbox().await.unwrap();
        self.desk.check_out(number).await.unwrap()
    }

    /// `check_out_now`, which must have checked the PR out.
    pub(super) async fn check_out(&self, number: i64) -> PrCheckout {
        match self.check_out_now(number).await {
            CheckOutOutcome::CheckedOut { checkout, .. } => checkout,
            other => panic!("#{number} was not checked out: {other:?}"),
        }
    }

    /// Add a commit writing `path` to PR `number`'s git branch and publish
    /// it, as the author pushing. Returns the new head.
    pub(super) fn push_to_pr(&self, number: u32, branch: &str, path: &str) -> String {
        self.repo.switch(branch);
        self.repo.commit(path, "pub fn two() {}\n", "two");
        let head = self.repo.open_pr(number, branch);
        self.repo.switch("main");
        head
    }
}
