//! Restacking: replaying a stack's git branches, bottom first, in the
//! stack's worktree, with the snapshot → act → verify → restore transaction
//! of ADR 0004.
//!
//! Each git branch is its own `git rebase --onto <new parent> <upstream>
//! <branch>`, rather than one `rebase --update-refs`, so progress reads per
//! git branch and "restack from here" is the same run started higher up.
//! Before the first one, every git branch's tip is recorded. A conflict stops
//! the run with the rebase in progress and the run stored, so the user can
//! resolve it (in a thread or an editor) and continue, or abort; abort puts
//! every git branch back at its recorded tip in one `update-ref` transaction,
//! and the worktree back on the git branch it had.
//!
//! Stack edits (reorder, fold, remove) drive the same steps atomically: a
//! conflict there restores everything and reports what conflicted.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::stack::Layer;
use super::ReviewDesk;
use crate::git::stack::{self as git, Replay};
use crate::review::stack_types::{RestackBranch, RestackProgress, RestackStatus, StackInput};
use crate::storage;

/// What a step replays onto.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum Parent {
    /// A commit fixed when the run was planned.
    Fixed(String),
    /// Wherever step `n` of this run left its git branch.
    Step(usize),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Step {
    pub(super) number: i64,
    pub(super) head_ref: String,
    /// The last commit below the git branch's own commits.
    pub(super) upstream: String,
    pub(super) parent: Parent,
    pub(super) status: RestackStatus,
    pub(super) commits: i64,
    pub(super) conflicts: Vec<String>,
}

/// A run of steps in one worktree, and how to undo it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Run {
    pub(super) stack: i64,
    pub(super) worktree: String,
    /// The git branch the worktree had when the run started.
    pub(super) return_branch: Option<String>,
    pub(super) onto_ref: String,
    /// Every git branch the run may move, at its tip before it started.
    pub(super) snapshot: Vec<(String, String)>,
    pub(super) steps: Vec<Step>,
}

impl Run {
    pub(super) fn progress(&self) -> RestackProgress {
        RestackProgress {
            stack: self.stack,
            worktree: self.worktree.clone(),
            onto_ref: self.onto_ref.clone(),
            branches: self
                .steps
                .iter()
                .map(|step| RestackBranch {
                    number: step.number,
                    head_ref: step.head_ref.clone(),
                    status: step.status,
                    commits: step.commits,
                    conflicts: step.conflicts.clone(),
                })
                .collect(),
            done: self.steps.iter().all(|step| {
                matches!(
                    step.status,
                    RestackStatus::Replayed | RestackStatus::Unchanged
                )
            }),
        }
    }

    /// Steps over `layers`, the bottom one onto `first_parent` and each
    /// other onto the step below it.
    pub(super) fn steps_for(layers: &[Layer], first_parent: String) -> Vec<Step> {
        layers
            .iter()
            .enumerate()
            .map(|(index, layer)| Step {
                number: layer.number,
                head_ref: layer.head_ref.clone(),
                upstream: layer.upstream.clone(),
                parent: match index {
                    0 => Parent::Fixed(first_parent.clone()),
                    _ => Parent::Step(index - 1),
                },
                status: RestackStatus::Waiting,
                commits: 0,
                conflicts: Vec::new(),
            })
            .collect()
    }
}

impl ReviewDesk {
    pub(super) async fn stored_restack(&self, stack: i64) -> Result<Option<Run>, String> {
        let payload = storage::read_restack(&self.database, &self.repo_key(), stack).await?;
        Ok(payload.and_then(|payload| serde_json::from_str(&payload).ok()))
    }

    async fn store_restack(&self, run: &Run) -> Result<(), String> {
        let payload = serde_json::to_string(run).map_err(|error| error.to_string())?;
        storage::write_restack(&self.database, &self.repo_key(), run.stack, &payload).await
    }

    /// The worktree a stack edit runs in: the stack's, clean, with no
    /// rebase of its own going on.
    pub(super) fn edit_worktree(
        &self,
        input: &StackInput,
    ) -> Result<(String, Option<String>), String> {
        let worktree = self.stack_worktree(input)?.ok_or_else(|| {
            "The stack needs a worktree first: check one of its git branches out".to_string()
        })?;
        if git::rebase_in_progress(&self.host, Path::new(&worktree.path)) {
            return Err(format!("A rebase is already going on in {}", worktree.path));
        }
        if !worktree.clean {
            return Err(format!(
                "{} has uncommitted changes. Commit or stash them first",
                worktree.path
            ));
        }
        Ok((worktree.path, worktree.branch))
    }

    /// Record every open git branch's tip, creating local git branches for
    /// PRs that have none.
    pub(super) fn snapshot(&self, input: &StackInput) -> Result<Vec<(String, String)>, String> {
        let mut snapshot = Vec::new();
        for pr in input.open_prs() {
            let tip = match git::branch_tip(&self.host, self.dir(), &pr.head_ref)? {
                Some(tip) => tip,
                None => {
                    git::create_branch(&self.host, self.dir(), &pr.head_ref, &pr.head_sha)?;
                    pr.head_sha.clone()
                }
            };
            snapshot.push((pr.head_ref.clone(), tip));
        }
        Ok(snapshot)
    }

    fn parent_sha(&self, run: &Run, parent: &Parent) -> Result<String, String> {
        match parent {
            Parent::Fixed(sha) => Ok(sha.clone()),
            Parent::Step(index) => {
                let below = &run.steps[*index].head_ref;
                git::branch_tip(&self.host, self.dir(), below)?
                    .ok_or_else(|| format!("{below} is gone"))
            }
        }
    }

    /// Run every waiting step in order. Stops at the first conflict.
    pub(super) fn drive(&self, run: &mut Run) -> Result<(), String> {
        let dir = Path::new(&run.worktree).to_path_buf();
        for index in 0..run.steps.len() {
            if run.steps[index].status != RestackStatus::Waiting {
                continue;
            }
            let new_parent = self.parent_sha(run, &run.steps[index].parent.clone())?;
            let step = &run.steps[index];
            if new_parent == step.upstream {
                let tip =
                    git::branch_tip(&self.host, self.dir(), &step.head_ref)?.unwrap_or_default();
                let commits = git::count_between(&self.host, self.dir(), &step.upstream, &tip)?;
                let step = &mut run.steps[index];
                step.status = RestackStatus::Unchanged;
                step.commits = commits;
                continue;
            }
            let (upstream, head_ref) = (step.upstream.clone(), step.head_ref.clone());
            match git::replay_onto(&self.host, &dir, &new_parent, &upstream, &head_ref)? {
                Replay::Done => {
                    let commits =
                        git::count_between(&self.host, self.dir(), &new_parent, &head_ref)?;
                    let step = &mut run.steps[index];
                    step.status = RestackStatus::Replayed;
                    step.commits = commits;
                }
                Replay::Conflict(files) => {
                    let step = &mut run.steps[index];
                    step.status = RestackStatus::Conflict;
                    step.conflicts = files;
                    return Ok(());
                }
            }
        }
        self.finish(run)
    }

    /// Put the worktree back on the git branch it had.
    fn finish(&self, run: &Run) -> Result<(), String> {
        let dir = Path::new(&run.worktree);
        if let Some(branch) = &run.return_branch {
            if git::branch_tip(&self.host, self.dir(), branch)?.is_some() {
                git::switch(&self.host, dir, branch)?;
            }
        }
        Ok(())
    }

    /// Undo everything `run` did: stop its rebase, put every git branch at
    /// its snapshot and the worktree back on its git branch.
    pub(super) fn restore(&self, run: &Run) -> Result<(), String> {
        let dir = Path::new(&run.worktree);
        git::abort_replay(&self.host, dir)?;
        git::detach(&self.host, dir)?;
        git::set_branches(&self.host, self.dir(), &run.snapshot)?;
        match &run.return_branch {
            Some(branch) => git::switch(&self.host, dir, branch),
            None => git::reset_hard(&self.host, dir),
        }
    }

    /// Drive `run` as one stack edit: on a conflict or failure, restore and
    /// fail.
    pub(super) fn drive_atomically(&self, run: &mut Run) -> Result<(), String> {
        let outcome = self.drive(run);
        let conflict = run
            .steps
            .iter()
            .find(|step| step.status == RestackStatus::Conflict)
            .map(|step| {
                format!(
                    "{} conflicts in {}",
                    step.head_ref,
                    step.conflicts.join(", ")
                )
            });
        match (outcome, conflict) {
            (Ok(()), None) => Ok(()),
            (Ok(()), Some(conflict)) => {
                self.restore(run)?;
                Err(format!("{conflict}. Nothing moved"))
            }
            (Err(error), _) => {
                let _ = self.restore(run);
                Err(error)
            }
        }
    }

    /// Restack the stack's open git branches from `from` up (all of them by
    /// default) onto the base's tip, in the stack's worktree.
    pub(crate) async fn restack(
        &self,
        input: StackInput,
        from: Option<i64>,
    ) -> Result<RestackProgress, String> {
        if let Some(run) = self.stored_restack(input.number).await? {
            return Err(format!(
                "A restack of this stack is stopped at a conflict in {}. Continue or abort it first",
                run.progress().conflict().map(|branch| branch.head_ref.as_str()).unwrap_or("a git branch")
            ));
        }
        let run = self
            .blocking("Restacking failed", move |desk| {
                desk.ensure_heads(&input.open_prs())?;
                let onto = desk.ensure_base(&input.base_ref, &input.base_sha)?;
                git::locked(&desk.host, desk.dir(), || {
                    desk.catch_up_branches(&input.open_prs())?;
                    let (worktree, return_branch) = desk.edit_worktree(&input)?;
                    let snapshot = desk.snapshot(&input)?;
                    let layers = desk.layers(&input, &onto, from)?;
                    let first_parent = match layers.first() {
                        Some(first) => {
                            let open = input.open_prs();
                            let position = open
                                .iter()
                                .position(|pr| pr.number == first.number)
                                .unwrap_or(0);
                            match position.checked_sub(1).and_then(|below| open.get(below)) {
                                Some(below) => {
                                    git::branch_tip(&desk.host, desk.dir(), &below.head_ref)?
                                        .unwrap_or_else(|| below.head_sha.clone())
                                }
                                None => onto.clone(),
                            }
                        }
                        None => onto.clone(),
                    };
                    let mut run = Run {
                        stack: input.number,
                        worktree,
                        return_branch,
                        onto_ref: input.base_ref.clone(),
                        snapshot,
                        steps: Run::steps_for(&layers, first_parent),
                    };
                    if let Err(error) = desk.drive(&mut run) {
                        let _ = desk.restore(&run);
                        return Err(error);
                    }
                    Ok(run)
                })
            })
            .await?;
        self.settle(run).await
    }

    /// Store a run stopped at a conflict, or forget a finished one.
    async fn settle(&self, run: Run) -> Result<RestackProgress, String> {
        let progress = run.progress();
        if progress.done {
            storage::delete_restack(&self.database, &self.repo_key(), run.stack).await?;
        } else {
            self.store_restack(&run).await?;
        }
        Ok(progress)
    }

    /// Carry on after the conflict a stopped restack is waiting on has been
    /// resolved in its worktree.
    pub(crate) async fn continue_restack(&self, stack: i64) -> Result<RestackProgress, String> {
        let run = self
            .stored_restack(stack)
            .await?
            .ok_or_else(|| "There is no stopped restack to continue".to_string())?;
        let run = self
            .blocking("Continuing the restack failed", move |desk| {
                let mut run = run;
                git::locked(&desk.host, desk.dir(), || {
                    let dir = Path::new(&run.worktree).to_path_buf();
                    let Some(index) = run
                        .steps
                        .iter()
                        .position(|step| step.status == RestackStatus::Conflict)
                    else {
                        desk.drive(&mut run)?;
                        return Ok(run);
                    };
                    if !git::rebase_in_progress(&desk.host, &dir) {
                        // Someone (an agent in the worktree, a terminal)
                        // already continued or aborted the rebase.
                        let parent = desk.parent_sha(&run, &run.steps[index].parent.clone())?;
                        let head_ref = run.steps[index].head_ref.clone();
                        let step = &mut run.steps[index];
                        step.conflicts.clear();
                        if git::contains(&desk.host, desk.dir(), &parent, &head_ref)? {
                            step.status = RestackStatus::Replayed;
                            step.commits =
                                git::count_between(&desk.host, desk.dir(), &parent, &head_ref)?;
                        } else {
                            // Aborted: replay this git branch again.
                            step.status = RestackStatus::Waiting;
                        }
                        desk.drive(&mut run)?;
                        return Ok(run);
                    }
                    match git::continue_replay(&desk.host, &dir)? {
                        Replay::Conflict(files) => {
                            run.steps[index].conflicts = files;
                            return Ok(run);
                        }
                        Replay::Done => {
                            let parent = desk.parent_sha(&run, &run.steps[index].parent.clone())?;
                            let head_ref = run.steps[index].head_ref.clone();
                            let step = &mut run.steps[index];
                            step.status = RestackStatus::Replayed;
                            step.conflicts.clear();
                            step.commits =
                                git::count_between(&desk.host, desk.dir(), &parent, &head_ref)?;
                        }
                    }
                    desk.drive(&mut run)?;
                    Ok(run)
                })
            })
            .await?;
        self.settle(run).await
    }

    /// Stop a restack and put every git branch back where it was.
    pub(crate) async fn abort_restack(&self, stack: i64) -> Result<(), String> {
        let run = self
            .stored_restack(stack)
            .await?
            .ok_or_else(|| "There is no stopped restack to abort".to_string())?;
        self.blocking("Aborting the restack failed", move |desk| {
            git::locked(&desk.host, desk.dir(), || desk.restore(&run))
        })
        .await?;
        storage::delete_restack(&self.database, &self.repo_key(), stack).await
    }
}

#[cfg(test)]
mod tests {
    use super::super::stack_support::StackFixture;
    use crate::review::stack_types::RestackStatus;

    #[tokio::test]
    async fn restacking_replays_every_git_branch_onto_the_new_base_in_the_worktree() {
        let Some(stack) = StackFixture::new().await else {
            return;
        };
        let main = stack.advance_main("main.rs", "the base moved");
        let worktree = stack.worktree_on("feat/claude");

        let progress = stack.desk().restack(stack.input(), None).await.unwrap();

        assert!(progress.done);
        assert!(progress
            .branches
            .iter()
            .all(|branch| branch.status == RestackStatus::Replayed));
        assert!(stack.has_ancestor("feat/driver", &main));
        assert!(stack.has_ancestor("feat/claude", &stack.tip("feat/driver")));
        assert!(stack.has_ancestor("feat/maki", &stack.tip("feat/claude")));
        assert_eq!(
            stack.subjects("feat/claude", "feat/maki"),
            vec!["add maki.rs"]
        );
        // The worktree is back on the git branch it had.
        assert_eq!(
            stack.git_in(&worktree, &["branch", "--show-current"]),
            "feat/claude"
        );
        assert!(stack
            .desk()
            .stack_overview(stack.input())
            .await
            .unwrap()
            .restack
            .is_none());
    }

    #[tokio::test]
    async fn a_conflict_stops_the_restack_and_abort_restores_every_git_branch_exactly() {
        let Some(stack) = StackFixture::new().await else {
            return;
        };
        stack.advance_main("claude.rs", "main's version\n");
        let worktree = stack.worktree_on("feat/maki");
        let before: Vec<String> = ["feat/driver", "feat/claude", "feat/maki"]
            .iter()
            .map(|branch| stack.tip(branch))
            .collect();

        let progress = stack.desk().restack(stack.input(), None).await.unwrap();

        assert!(!progress.done);
        assert_eq!(
            progress
                .branches
                .iter()
                .map(|branch| branch.status)
                .collect::<Vec<_>>(),
            vec![
                RestackStatus::Replayed,
                RestackStatus::Conflict,
                RestackStatus::Waiting
            ]
        );
        assert_eq!(
            progress.conflict().unwrap().conflicts,
            vec!["claude.rs".to_string()]
        );
        // The stopped run survives a reload of the Stack page.
        let overview = stack.desk().stack_overview(stack.input()).await.unwrap();
        assert_eq!(overview.restack.unwrap().conflict().unwrap().number, 415);

        stack.desk().abort_restack(7).await.unwrap();

        let after: Vec<String> = ["feat/driver", "feat/claude", "feat/maki"]
            .iter()
            .map(|branch| stack.tip(branch))
            .collect();
        assert_eq!(after, before);
        assert_eq!(
            stack.git_in(&worktree, &["branch", "--show-current"]),
            "feat/maki"
        );
        assert_eq!(stack.git_in(&worktree, &["status", "--porcelain"]), "");
    }

    #[tokio::test]
    async fn a_resolved_conflict_continues_the_restack_to_the_top() {
        let Some(stack) = StackFixture::new().await else {
            return;
        };
        stack.advance_main("claude.rs", "main's version\n");
        let worktree = stack.worktree_on("feat/maki");
        stack.desk().restack(stack.input(), None).await.unwrap();

        std::fs::write(worktree.join("claude.rs"), "resolved\n").unwrap();
        let progress = stack.desk().continue_restack(7).await.unwrap();

        assert!(progress.done, "{progress:?}");
        assert!(stack.has_ancestor("feat/maki", &stack.tip("feat/claude")));
        assert_eq!(stack.git(&["show", "feat/claude:claude.rs"]), "resolved");
        assert_eq!(
            stack.git_in(&worktree, &["branch", "--show-current"]),
            "feat/maki"
        );
    }

    #[tokio::test]
    async fn a_rebase_continued_outside_pingex_carries_on_from_where_it_got_to() {
        let Some(stack) = StackFixture::new().await else {
            return;
        };
        stack.advance_main("claude.rs", "main's version\n");
        let worktree = stack.worktree_on("feat/maki");
        stack.desk().restack(stack.input(), None).await.unwrap();

        // An agent in the worktree resolves it and continues the rebase itself.
        std::fs::write(worktree.join("claude.rs"), "resolved\n").unwrap();
        stack.git_in(&worktree, &["add", "claude.rs"]);
        stack.git_in(
            &worktree,
            &["-c", "core.editor=true", "rebase", "--continue"],
        );
        let progress = stack.desk().continue_restack(7).await.unwrap();

        assert!(progress.done, "{progress:?}");
        assert_eq!(stack.git(&["show", "feat/claude:claude.rs"]), "resolved");
        assert!(stack.has_ancestor("feat/maki", &stack.tip("feat/claude")));
        assert_eq!(
            stack.git_in(&worktree, &["branch", "--show-current"]),
            "feat/maki"
        );
    }

    #[tokio::test]
    async fn continue_leaves_a_binary_conflict_for_the_user_to_stage() {
        let Some(stack) = StackFixture::new().await else {
            return;
        };
        // #412 and main both change a binary logo and a text file.
        stack.git(&["switch", "-q", "feat/driver"]);
        std::fs::write(stack.inner.repo.repo.join("logo.png"), b"\x00\x01driver").unwrap();
        std::fs::write(stack.inner.repo.repo.join("notes.txt"), "driver's\n").unwrap();
        stack.git(&["add", "logo.png", "notes.txt"]);
        stack.git(&["commit", "-q", "-m", "driver logo"]);
        stack.inner.repo.open_pr(412, "feat/driver");
        stack.git(&["switch", "-q", "main"]);
        std::fs::write(stack.inner.repo.repo.join("logo.png"), b"\x00\x02main").unwrap();
        std::fs::write(stack.inner.repo.repo.join("notes.txt"), "main's\n").unwrap();
        stack.git(&["add", "logo.png", "notes.txt"]);
        stack.git(&["commit", "-q", "-m", "main logo"]);
        stack.git(&["push", "-q", "origin", "main"]);
        let worktree = stack.worktree_on("feat/maki");
        let first = stack.desk().restack(stack.input(), None).await.unwrap();
        assert_eq!(
            first.conflict().unwrap().conflicts,
            vec!["logo.png".to_string(), "notes.txt".to_string()]
        );

        std::fs::write(worktree.join("notes.txt"), "both\n").unwrap();
        let progress = stack.desk().continue_restack(7).await.unwrap();

        assert_eq!(
            progress.conflict().unwrap().conflicts,
            vec!["logo.png".to_string()]
        );
        assert!(stack
            .git_in(&worktree, &["diff", "--name-only", "--diff-filter=U"])
            .lines()
            .any(|path| path == "logo.png"));

        // The user picks the PR's logo and stages it.
        std::fs::write(worktree.join("logo.png"), b"\x00\x01driver").unwrap();
        stack.git_in(&worktree, &["add", "logo.png"]);
        let progress = stack.desk().continue_restack(7).await.unwrap();
        assert!(progress.done, "{progress:?}");
        assert_eq!(
            std::fs::read(worktree.join("logo.png")).unwrap(),
            b"\x00\x01driver"
        );
    }

    #[tokio::test]
    async fn a_restack_refuses_a_checked_out_git_branch_behind_github() {
        let Some(stack) = StackFixture::new().await else {
            return;
        };
        stack.advance_main("main.rs", "the base moved");
        stack.worktree_on("feat/maki");
        let theirs = stack.teammate_pushes(418, "feat/maki");
        let before: Vec<String> = ["feat/driver", "feat/claude", "feat/maki"]
            .iter()
            .map(|branch| stack.tip(branch))
            .collect();

        let error = stack.desk().restack(stack.input(), None).await.unwrap_err();

        assert!(error.contains("Pull"), "{error}");
        let after: Vec<String> = ["feat/driver", "feat/claude", "feat/maki"]
            .iter()
            .map(|branch| stack.tip(branch))
            .collect();
        assert_eq!(after, before);
        assert_eq!(stack.remote_tip("feat/maki"), theirs);
    }

    #[tokio::test]
    async fn restacking_from_a_git_branch_leaves_the_ones_below_it_alone() {
        let Some(stack) = StackFixture::new().await else {
            return;
        };
        stack.advance_main("main.rs", "the base moved");
        stack.worktree_on("feat/maki");
        let driver = stack.tip("feat/driver");
        let claude = stack.tip("feat/claude");

        let progress = stack
            .desk()
            .restack(stack.input(), Some(415))
            .await
            .unwrap();

        assert_eq!(
            progress
                .branches
                .iter()
                .map(|branch| (branch.number, branch.status))
                .collect::<Vec<_>>(),
            vec![
                (415, RestackStatus::Unchanged),
                (418, RestackStatus::Unchanged)
            ]
        );
        assert_eq!(stack.tip("feat/driver"), driver);
        assert_eq!(stack.tip("feat/claude"), claude);
    }

    #[tokio::test]
    async fn after_the_bottom_pr_merges_the_rest_restack_onto_the_base_without_its_commits() {
        let Some(stack) = StackFixture::new().await else {
            return;
        };
        // #412 squash-merges into main.
        stack.git(&["switch", "-q", "main"]);
        stack.git(&["merge", "-q", "--squash", "feat/driver"]);
        stack.git(&["commit", "-q", "-m", "Add driver.rs (#412)"]);
        stack.git(&["push", "-q", "origin", "main"]);
        stack.mark_merged(412);
        stack.worktree_on("feat/maki");

        let progress = stack.desk().restack(stack.input(), None).await.unwrap();

        assert!(progress.done);
        assert_eq!(stack.subjects("main", "feat/claude"), vec!["add claude.rs"]);
        assert_eq!(
            stack.subjects("main", "feat/maki"),
            vec!["add claude.rs", "add maki.rs"]
        );
        let overview = stack.desk().stack_overview(stack.input()).await.unwrap();
        assert_eq!(overview.merged_branches, vec![412]);
    }

    #[tokio::test]
    async fn a_restack_without_a_worktree_says_so() {
        let Some(stack) = StackFixture::new().await else {
            return;
        };
        let error = stack.desk().restack(stack.input(), None).await.unwrap_err();
        assert!(error.contains("needs a worktree"), "{error}");
    }
}
