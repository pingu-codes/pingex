//! The fixture every desk test starts from: a clone whose `origin` reads as
//! GitHub repository `SLUG`, a fake GitHub for it, and a desk over both.

use std::sync::Arc;

use super::ReviewDesk;
use crate::git::test_repo::TestRepo;
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
    let desk = ReviewDesk::new(Host::Native, repo.repo.clone(), github.clone(), database);
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
            updated_at: "2026-11-05T10:00:00Z".to_string(),
            ..InboxPr::default()
        };
        self.github.put_pr(pr.clone());
        pr
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
