//! Where PRs are on disk, as the inbox, PR page and Stack page show it.
//!
//! `locate` is the one rule. A PR is at its stored worktree; else in any
//! worktree on its own git branch, when that branch is really the PR's; else,
//! for a stack's PR, in the worktree of a neighbouring git branch, because a
//! stack shares one worktree. A worktree in the middle of a rebase is detached,
//! so only a neighbour's row still says it is the stack's (`candidates`).
//! `pr_checkouts` answers it for many PRs at once and changes nothing: the inbox
//! calls it on every focus.

use std::collections::HashMap;
use std::path::Path;

use super::steps::is_managed_branch;
use super::ReviewDesk;
use crate::git::checkouts as git;
use crate::git::worktrees::{self, STATUS_PARALLELISM};
use crate::review::checkout_types::{CheckoutTarget, PrCheckout};
use crate::review::requirements::CheckoutStep;
use crate::storage::{self, StoredCheckout};
use crate::util::host::Host;

/// How a PR came to be at a location.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Source {
    /// Its `review_checkouts` row.
    Stored,
    /// A worktree on its own git branch.
    OwnBranch,
    /// A worktree on another git branch of its stack.
    StackBranch,
}

/// Where a PR is: see the module comment. `stored` is the PR's row's path when
/// the directory exists. `same_repo` says the PR's head is in this repository;
/// a fork's branch name says nothing about a local branch of that name.
pub(super) fn locate<'a>(
    target: &CheckoutTarget,
    stored: Option<&'a str>,
    listed: &'a [(String, Option<String>)],
    same_repo: bool,
) -> Option<(&'a str, Source)> {
    if let Some(path) = stored {
        return Some((path, Source::Stored));
    }
    let on_branch = |wanted: &str| {
        listed
            .iter()
            .find(|(_, branch)| branch.as_deref() == Some(wanted))
            .map(|(path, _)| path.as_str())
    };
    if same_repo || target.stack.is_some() {
        if let Some(path) = on_branch(&target.head_ref) {
            return Some((path, Source::OwnBranch));
        }
    }
    listed
        .iter()
        .find(|(_, branch)| {
            branch
                .as_ref()
                .is_some_and(|branch| target.stack_branches.contains(branch))
        })
        .map(|(path, _)| (path.as_str(), Source::StackBranch))
}

/// The worktrees `locate` looks through: those the rows of `target`'s stack
/// neighbours name, in the form they were stored, then what git lists.
pub(super) fn candidates<'a>(
    target: &CheckoutTarget,
    rows: impl Iterator<Item = &'a StoredCheckout>,
    listed: &[(String, Option<String>)],
    host: &Host,
) -> Vec<(String, Option<String>)> {
    rows.filter(|row| {
        row.pr_number != target.number
            && row
                .git_branch
                .as_ref()
                .is_some_and(|branch| target.stack_branches.contains(branch))
            && host.is_dir(&row.path)
    })
    .map(|row| (row.path.clone(), row.git_branch.clone()))
    .chain(listed.iter().cloned())
    .collect()
}

/// What is true of a worktree directory, whichever PR asks.
#[derive(Clone, Debug)]
pub(super) struct WorktreeFacts {
    pub(super) head: Option<String>,
    pub(super) branch: Option<String>,
    pub(super) dirty: bool,
    pub(super) managed: bool,
    pub(super) main_checkout: bool,
}

pub(super) fn read_facts(host: &Host, dir: &Path) -> Result<WorktreeFacts, String> {
    Ok(WorktreeFacts {
        head: git::head_of(host, dir)?,
        branch: git::branch_of(host, dir)?,
        dirty: !git::is_clean(host, dir).unwrap_or(true),
        managed: git::is_managed(host, dir).unwrap_or(false),
        main_checkout: !git::is_linked_worktree(host, dir).unwrap_or(true),
    })
}

/// `target` at the worktree `facts` describes.
pub(super) fn checkout_at(
    target: &CheckoutTarget,
    path: String,
    facts: WorktreeFacts,
    (on_pr_branch, ahead, behind): (bool, i64, i64),
    stored_branch: Option<String>,
    setup_pending: Option<String>,
) -> PrCheckout {
    PrCheckout {
        number: target.number,
        step: CheckoutStep::Worktree,
        path: Some(path),
        head_sha: facts.head,
        git_branch: facts.branch.or(stored_branch),
        setup_pending,
        ahead,
        behind,
        dirty: facts.dirty,
        managed: facts.managed,
        on_pr_branch,
        main_checkout: facts.main_checkout,
    }
}

impl ReviewDesk {
    /// Every worktree with a directory, and the git branch on it.
    pub(super) fn listed_worktrees(&self) -> Result<Vec<(String, Option<String>)>, String> {
        Ok(worktrees::worktree_branches(&self.host, self.dir())?
            .into_iter()
            .filter(|(path, _)| self.host.is_dir(path))
            .collect())
    }

    /// Whether `branch`, checked out somewhere, is `target`'s own. A detached
    /// HEAD counts unless the PR is a stack's: nothing else claims it, whereas
    /// a stack's worktree is detached only for a rebase or a clean-up.
    pub(super) fn is_own_branch(&self, target: &CheckoutTarget, branch: Option<&str>) -> bool {
        let Some(branch) = branch else {
            return target.stack.is_none();
        };
        is_managed_branch(branch, target.number)
            || (branch == target.head_ref
                && (target.stack.is_some() || self.same_repo(target).unwrap_or(false)))
    }

    /// Whether `target` is on its own branch at `dir`, and the commits its
    /// branch there has and lacks against the PR's head. Away from its own
    /// branch, HEAD says nothing about the PR, so the counts come from the
    /// PR's local branch. A PR head not fetched yet can't be counted against.
    pub(super) fn counts(
        &self,
        target: &CheckoutTarget,
        dir: &Path,
        facts: &WorktreeFacts,
        stored_branch: Option<&str>,
    ) -> Result<(bool, i64, i64), String> {
        let own = self.is_own_branch(target, facts.branch.as_deref());
        let tip = if own {
            facts.head.clone()
        } else {
            let branch = stored_branch
                .filter(|branch| self.is_own_branch(target, Some(branch)))
                .unwrap_or(&target.head_ref);
            git::branch_tip(&self.host, self.dir(), branch)?
        };
        let (ahead, behind) = match tip {
            Some(tip) if tip == target.head_sha => (0, 0),
            Some(tip) if git::has_commit(&self.host, dir, &target.head_sha)? => {
                git::ahead_behind(&self.host, dir, &target.head_sha, &tip).unwrap_or((0, 0))
            }
            _ => (0, 0),
        };
        Ok((own, ahead, behind))
    }

    /// Where each of `targets` is on disk. PRs with no location are left out.
    /// Read-only, unlike `checkout_state`: no fetches, no rows written, nothing
    /// adopted. A worktree that can't be read counts as no location.
    pub(crate) async fn pr_checkouts(
        &self,
        targets: Vec<CheckoutTarget>,
    ) -> Result<Vec<PrCheckout>, String> {
        if targets.is_empty() {
            return Ok(Vec::new());
        }
        let key = self.repo_key();
        let rows: HashMap<i64, StoredCheckout> = storage::read_checkouts(&self.database, &key)
            .await?
            .into_iter()
            .map(|row| (row.pr_number, row))
            .collect();
        let setup_command = storage::read_repository_settings(&self.database, &key)
            .await?
            .setup_command;
        self.blocking("Reading the worktrees failed", move |desk| {
            let listed = desk.listed_worktrees()?;
            let located: Vec<(&CheckoutTarget, String, Source)> = targets
                .iter()
                .filter_map(|target| {
                    let stored = rows
                        .get(&target.number)
                        .map(|row| row.path.as_str())
                        .filter(|path| desk.host.is_dir(path));
                    let same_repo = desk.same_repo(target).unwrap_or(false);
                    let places = candidates(target, rows.values(), &listed, &desk.host);
                    locate(target, stored, &places, same_repo)
                        .map(|(path, source)| (target, path.to_string(), source))
                })
                .collect();

            let mut paths: Vec<&str> = located.iter().map(|(_, path, _)| path.as_str()).collect();
            paths.sort_unstable();
            paths.dedup();
            let facts = read_all_facts(&desk.host, &paths);

            Ok(located
                .iter()
                .filter_map(|(target, path, source)| {
                    let found = facts.get(path.as_str())?.clone();
                    let row = (*source == Source::Stored)
                        .then(|| rows.get(&target.number))
                        .flatten();
                    let stored_branch = row.and_then(|row| row.git_branch.clone());
                    let counts = desk
                        .counts(target, Path::new(path), &found, stored_branch.as_deref())
                        .ok()?;
                    let setup_pending =
                        row.filter(|row| !row.setup_done).and(setup_command.clone());
                    Some(checkout_at(
                        target,
                        path.clone(),
                        found,
                        counts,
                        stored_branch,
                        setup_pending,
                    ))
                })
                .collect())
        })
        .await
    }
}

/// The facts of each of `paths`, a few at a time. One unreadable worktree
/// leaves just that one out.
fn read_all_facts<'a>(host: &Host, paths: &[&'a str]) -> HashMap<&'a str, WorktreeFacts> {
    let mut facts = HashMap::with_capacity(paths.len());
    for chunk in paths.chunks(STATUS_PARALLELISM) {
        std::thread::scope(|scope| {
            let handles: Vec<_> = chunk
                .iter()
                .map(|path| {
                    let path = *path;
                    scope.spawn(move || (path, read_facts(host, Path::new(path))))
                })
                .collect();
            for handle in handles {
                if let Ok((path, Ok(found))) = handle.join() {
                    facts.insert(path, found);
                }
            }
        });
    }
    facts
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::super::stack_support::StackFixture;
    use super::super::test_support::{fixture, SLUG};
    use super::{locate, Source};
    use crate::review::checkout_types::CheckoutTarget;
    use crate::review::types::{PrDetail, PrSummary};
    use crate::storage;

    fn pr(head_ref: &str, siblings: &[&str]) -> CheckoutTarget {
        CheckoutTarget {
            number: 5,
            head_ref: head_ref.to_string(),
            stack: (!siblings.is_empty()).then(|| "driver".to_string()),
            stack_branches: siblings.iter().map(|branch| branch.to_string()).collect(),
            ..CheckoutTarget::default()
        }
    }

    fn listed(entries: &[(&str, &str)]) -> Vec<(String, Option<String>)> {
        entries
            .iter()
            .map(|(path, branch)| (path.to_string(), Some(branch.to_string())))
            .collect()
    }

    #[test]
    fn a_stored_worktree_wins_over_a_branch_elsewhere() {
        let list = listed(&[("/other", "feat/x")]);
        assert_eq!(
            locate(&pr("feat/x", &[]), Some("/stored"), &list, true),
            Some(("/stored", Source::Stored))
        );
    }

    #[test]
    fn nothing_on_disk_is_no_location() {
        let list = listed(&[("/clone", "main")]);
        assert_eq!(locate(&pr("feat/x", &[]), None, &list, true), None);
    }

    #[test]
    fn a_branch_checked_out_by_hand_counts_whoever_wrote_the_pr() {
        let list = listed(&[("/clone", "main"), ("/by-hand", "feat/x")]);
        assert_eq!(
            locate(&pr("feat/x", &[]), None, &list, true),
            Some(("/by-hand", Source::OwnBranch))
        );
    }

    #[test]
    fn a_forks_branch_name_says_nothing_about_my_local_branch() {
        let list = listed(&[("/clone", "main")]);
        assert_eq!(locate(&pr("main", &[]), None, &list, false), None);
    }

    #[test]
    fn a_stack_pr_is_in_the_worktree_of_a_neighbouring_branch() {
        let siblings = ["feat/a", "feat/b", "feat/c"];
        let list = listed(&[("/clone", "main"), ("/stack", "feat/c")]);
        assert_eq!(
            locate(&pr("feat/a", &siblings), None, &list, true),
            Some(("/stack", Source::StackBranch))
        );
    }

    #[test]
    fn a_stack_pr_prefers_a_worktree_on_its_own_branch() {
        let siblings = ["feat/a", "feat/b"];
        let list = listed(&[("/stack", "feat/b"), ("/own", "feat/a")]);
        assert_eq!(
            locate(&pr("feat/a", &siblings), None, &list, true),
            Some(("/own", Source::OwnBranch))
        );
    }

    #[tokio::test]
    async fn the_batch_reports_each_location_and_writes_nothing() {
        let Some(stack) = StackFixture::new().await else {
            return;
        };
        let worktree = stack.worktree_on("feat/maki");
        let targets = vec![stack.target(412), stack.target(415), stack.target(418)];

        let found = stack.desk().pr_checkouts(targets).await.unwrap();

        assert_eq!(found.len(), 3);
        let on_branch: Vec<(i64, bool)> = found
            .iter()
            .map(|checkout| (checkout.number, checkout.on_pr_branch))
            .collect();
        assert_eq!(on_branch, vec![(412, false), (415, false), (418, true)]);
        for checkout in &found {
            assert_eq!(
                std::fs::canonicalize(checkout.path.as_ref().unwrap()).unwrap(),
                std::fs::canonicalize(&worktree).unwrap()
            );
            assert_eq!(checkout.git_branch.as_deref(), Some("feat/maki"));
            assert!(!checkout.main_checkout);
            assert_eq!((checkout.ahead, checkout.behind), (0, 0));
        }
        let rows = storage::read_checkouts(&stack.desk().database, &stack.desk().repo_key())
            .await
            .unwrap();
        assert!(rows.is_empty(), "wrote {rows:?}");
    }

    #[tokio::test]
    async fn the_batch_says_when_the_location_is_the_main_clone() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(2, "two", "src/two.rs");
        fixture.repo.switch("two");
        let target = CheckoutTarget {
            number: pr.number,
            head_ref: pr.head_ref.clone(),
            head_sha: pr.head_sha.clone(),
            head_repo: Some(SLUG.to_string()),
            ..CheckoutTarget::default()
        };

        let found = fixture.desk.pr_checkouts(vec![target]).await.unwrap();

        assert_eq!(found.len(), 1);
        assert!(found[0].main_checkout);
        assert!(found[0].on_pr_branch);
    }

    #[tokio::test]
    async fn the_batch_leaves_out_a_worktree_whose_directory_is_gone_and_keeps_its_row() {
        let Some(stack) = StackFixture::new().await else {
            return;
        };
        let target = stack.target(418);
        let path = stack
            .desk()
            .ensure_pr_worktree(target.clone())
            .await
            .unwrap()
            .path
            .unwrap();
        std::fs::remove_dir_all(&path).unwrap();

        let found = stack.desk().pr_checkouts(vec![target]).await.unwrap();

        assert!(found.is_empty());
        let row = storage::read_checkout(&stack.desk().database, &stack.desk().repo_key(), 418)
            .await
            .unwrap();
        assert!(row.is_some());
    }

    #[tokio::test]
    async fn someone_elses_pr_on_its_branch_by_hand_is_at_the_worktree_step() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(2, "two", "src/two.rs");
        fixture.repo.switch("two");
        let target = CheckoutTarget {
            number: pr.number,
            head_ref: pr.head_ref.clone(),
            head_sha: pr.head_sha.clone(),
            head_repo: Some(SLUG.to_string()),
            mine: false,
            ..CheckoutTarget::default()
        };

        let state = fixture.desk.checkout_state(target).await.unwrap();

        assert!(state.path.is_some());
        assert!(!state.managed);
    }

    #[tokio::test]
    async fn syncing_a_pr_does_not_move_another_stack_branch_checked_out_in_its_worktree() {
        let Some(stack) = StackFixture::new().await else {
            return;
        };
        let top = stack.target(418);
        let worktree = stack.desk().ensure_pr_worktree(top.clone()).await.unwrap();
        let dir = worktree.path.unwrap();
        stack.git_in(Path::new(&dir), &["switch", "-q", "feat/driver"]);
        let lower = stack.tip("feat/driver");

        stack.desk().synced_checkout(top).await.unwrap();

        assert_eq!(stack.tip("feat/driver"), lower);
    }

    #[tokio::test]
    async fn a_worktree_on_another_stack_branch_counts_from_the_prs_own_branch() {
        let Some(stack) = StackFixture::new().await else {
            return;
        };
        let top = stack.target(418);
        let worktree = stack.desk().ensure_pr_worktree(top.clone()).await.unwrap();
        let dir = worktree.path.unwrap();
        stack.git_in(Path::new(&dir), &["switch", "-q", "feat/driver"]);

        let state = stack.desk().checkout_state(top).await.unwrap();

        // HEAD lacks #418's two upper commits, but #418's own branch is level.
        assert_eq!((state.ahead, state.behind), (0, 0));
    }

    #[tokio::test]
    async fn a_worktree_on_another_stack_branch_is_not_pushed_to_the_pr() {
        let Some(stack) = StackFixture::new().await else {
            return;
        };
        let lower = stack.target(412);
        stack.inner.github.put_detail(
            412,
            PrDetail {
                summary: PrSummary {
                    number: 412,
                    author: "me".to_string(),
                    head_ref: lower.head_ref.clone(),
                    head_repo: lower.head_repo.clone(),
                    ..PrSummary::default()
                },
                head_sha: lower.head_sha.clone(),
                ..PrDetail::default()
            },
        );
        let worktree = stack.desk().ensure_pr_worktree(lower).await.unwrap();
        let dir = worktree.path.unwrap();
        stack.git_in(Path::new(&dir), &["switch", "-q", "feat/maki"]);
        let on_github = stack.remote_tip("feat/driver");

        let pushed = stack.desk().push_to_pr(412).await;

        assert!(pushed.is_err(), "pushed {pushed:?}");
        assert_eq!(stack.remote_tip("feat/driver"), on_github);
    }
}
