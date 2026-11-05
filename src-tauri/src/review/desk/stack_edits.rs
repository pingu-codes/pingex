//! Stack edits: push the stack, reorder it, fold a git branch into its
//! parent, take one out, split one, rename one, add one, and drop git
//! branches whose PRs merged.
//!
//! Every edit that rewrites history is a stack edit run (`restack::Run`)
//! driven atomically in the stack's worktree, then one atomic, leased push of
//! what moved, then the retargets GitHub needs so each PR's base is the git
//! branch below it again. A conflict or a refused push puts every git branch
//! back where it was. Pushing leases against the head GitHub has, and
//! nothing replays or pushes a local git branch that lacks GitHub's head
//! (`catch_up_branches`), so a git branch someone else pushed to is never
//! overwritten.

use std::path::Path;

use super::restack::{Parent, Run, Step};
use super::ReviewDesk;
use crate::git::stack::{self as git, LeasedPush};
use crate::review::stack_types::{
    PushReport, RestackStatus, SplitRequest, StackInput, StackInputPr,
};
use crate::review::types::NewPr;

impl ReviewDesk {
    /// The leased pushes that bring GitHub up to the local tips of `prs`.
    fn pushes_for(&self, prs: &[&StackInputPr]) -> Result<Vec<LeasedPush>, String> {
        let mut pushes = Vec::new();
        for pr in prs {
            let Some(tip) = git::branch_tip(&self.host, self.dir(), &pr.head_ref)? else {
                continue;
            };
            if tip != pr.head_sha {
                pushes.push(LeasedPush {
                    branch: pr.head_ref.clone(),
                    lease: Some(pr.head_sha.clone()),
                });
            }
        }
        Ok(pushes)
    }

    /// Push the stack's open git branches (or only `only`'s) that differ
    /// from GitHub, in one atomic push leased on GitHub's heads.
    pub(crate) async fn push_stack(
        &self,
        input: StackInput,
        only: Option<i64>,
    ) -> Result<PushReport, String> {
        self.blocking("Pushing the stack failed", move |desk| {
            let open = input.open_prs();
            let chosen: Vec<&StackInputPr> = open
                .into_iter()
                .filter(|pr| only.is_none_or(|number| pr.number == number))
                .collect();
            desk.ensure_heads(&chosen)?;
            let pushes = git::locked(&desk.host, desk.dir(), || {
                desk.catch_up_branches(&chosen)?;
                let pushes = desk.pushes_for(&chosen)?;
                git::push_leased(&desk.host, desk.dir(), &desk.remote()?, &pushes)?;
                Ok(pushes)
            })?;
            Ok(PushReport {
                pushed: pushes.into_iter().map(|push| push.branch).collect(),
            })
        })
        .await
    }

    /// Run `run`, then push what moved; on any failure put everything back.
    fn edit_and_push(
        &self,
        input: &StackInput,
        run: &mut Run,
        extra: impl FnOnce(&ReviewDesk) -> Result<(), String>,
    ) -> Result<(), String> {
        self.drive_atomically(run)?;
        let outcome = (|| {
            extra(self)?;
            let pushes = self.pushes_for(&input.open_prs())?;
            git::push_leased(&self.host, self.dir(), &self.remote()?, &pushes)
        })();
        if let Err(error) = outcome {
            let _ = self.restore(run);
            return Err(error);
        }
        Ok(())
    }

    /// Start an edit: the base fetched, the worktree checked, every open git
    /// branch recorded.
    fn begin_edit(
        &self,
        input: &StackInput,
    ) -> Result<(String, Run, Vec<super::stack::Layer>), String> {
        self.ensure_heads(&input.open_prs())?;
        let onto = self.ensure_base(&input.base_ref, &input.base_sha)?;
        self.catch_up_branches(&input.open_prs())?;
        let (worktree, return_branch) = self.edit_worktree(input)?;
        let snapshot = self.snapshot(input)?;
        let layers = self.layers(input, &onto, None)?;
        let run = Run {
            stack: input.number,
            worktree,
            return_branch,
            onto_ref: input.base_ref.clone(),
            snapshot,
            steps: Vec::new(),
        };
        Ok((onto, run, layers))
    }

    fn position(input: &StackInput, number: i64) -> Result<usize, String> {
        input
            .open_prs()
            .iter()
            .position(|pr| pr.number == number)
            .ok_or_else(|| format!("#{number} is not an open PR in this stack"))
    }

    /// The git branch PR `index` (in `order`, bottom first) should merge into.
    fn base_at(input: &StackInput, order: &[&StackInputPr], index: usize) -> String {
        match index.checked_sub(1) {
            Some(below) => order[below].head_ref.clone(),
            None => input.base_ref.clone(),
        }
    }

    /// Put the open PRs in `order` (bottom first): rebase each onto the one
    /// now below it, push, and retarget the PRs whose base changed.
    ///
    /// GitHub merges a PR as soon as its head is in its base, and a PR that
    /// moves down would be in that state while the push lands (its old base
    /// is rewritten on top of it). So every PR that moves goes onto the
    /// stack's base before the push, and onto its new parent after it.
    pub(crate) async fn reorder_stack(
        &self,
        input: StackInput,
        order: Vec<i64>,
    ) -> Result<(), String> {
        let base_ref = input.base_ref.clone();
        let retargets = self
            .blocking("Reordering the stack failed", move |desk| {
                let open = input.open_prs();
                let mut wanted = order.clone();
                wanted.sort_unstable();
                let mut have: Vec<i64> = open.iter().map(|pr| pr.number).collect();
                have.sort_unstable();
                if wanted != have {
                    return Err(
                        "The new order must list every open PR in the stack once".to_string()
                    );
                }
                let reordered: Vec<&StackInputPr> = order
                    .iter()
                    .filter_map(|number| open.iter().find(|pr| pr.number == *number).copied())
                    .collect();
                // (PR, new base, old base) for each PR whose base changes.
                let moves: Vec<(i64, String, String)> = reordered
                    .iter()
                    .enumerate()
                    .filter_map(|(index, pr)| {
                        let old = open
                            .iter()
                            .position(|candidate| candidate.number == pr.number)?;
                        let (now, before) = (
                            Self::base_at(&input, &reordered, index),
                            Self::base_at(&input, &open, old),
                        );
                        (now != before).then_some((pr.number, now, before))
                    })
                    .collect();
                git::locked(&desk.host, desk.dir(), || {
                    let (_, mut run, layers) = desk.begin_edit(&input)?;
                    let fork = layers
                        .first()
                        .map(|layer| layer.upstream.clone())
                        .unwrap_or_default();
                    run.steps = order
                        .iter()
                        .enumerate()
                        .map(|(index, number)| {
                            let layer = layers
                                .iter()
                                .find(|layer| layer.number == *number)
                                .expect("checked above");
                            Step {
                                number: *number,
                                head_ref: layer.head_ref.clone(),
                                upstream: layer.upstream.clone(),
                                parent: match index {
                                    0 => Parent::Fixed(fork.clone()),
                                    _ => Parent::Step(index - 1),
                                },
                                status: RestackStatus::Waiting,
                                commits: 0,
                                conflicts: Vec::new(),
                            }
                        })
                        .collect();
                    let parked = std::cell::RefCell::new(Vec::new());
                    let outcome = desk.edit_and_push(&input, &mut run, |desk| {
                        for (number, _, before) in &moves {
                            if *before != input.base_ref {
                                desk.github.retarget(*number, &input.base_ref)?;
                                parked.borrow_mut().push((*number, before.clone()));
                            }
                        }
                        Ok(())
                    });
                    if outcome.is_err() {
                        // Nothing moved: the parked PRs go back where they were.
                        for (number, before) in parked.into_inner() {
                            let _ = desk.github.retarget(number, &before);
                        }
                    }
                    outcome
                })?;
                Ok(moves
                    .into_iter()
                    .filter(|(_, now, _)| *now != input.base_ref)
                    .map(|(number, now, _)| (number, now))
                    .collect::<Vec<_>>())
            })
            .await?;
        self.retarget_all(retargets, &base_ref).await
    }

    async fn retarget_all(
        &self,
        retargets: Vec<(i64, String)>,
        base_ref: &str,
    ) -> Result<(), String> {
        // Onto the base first, so no PR is ever based on a git branch that is
        // itself still based on it.
        let mut retargets = retargets;
        retargets.sort_by_key(|(_, base)| base != base_ref);
        for (number, base) in retargets {
            self.retarget(number, base).await?;
        }
        Ok(())
    }

    /// Squash PR `number`'s git branch into the one below it, replay the git
    /// branches above onto the result, push, retarget the PR above onto the
    /// parent and close `number`'s PR.
    pub(crate) async fn fold_into_parent(
        &self,
        input: StackInput,
        number: i64,
    ) -> Result<(), String> {
        let (above, parent) = self
            .blocking("Folding failed", move |desk| {
                let open = input.open_prs();
                let index = Self::position(&input, number)?;
                let parent = index
                    .checked_sub(1)
                    .map(|below| open[below].clone())
                    .ok_or_else(|| {
                        format!(
                            "#{number} is the bottom of the stack: there is no PR to fold it into"
                        )
                    })?;
                let folded = open[index].clone();
                let above = open.get(index + 1).map(|pr| pr.number);
                git::locked(&desk.host, desk.dir(), || {
                    let (_, mut run, layers) = desk.begin_edit(&input)?;
                    let parent_tip = git::branch_tip(&desk.host, desk.dir(), &parent.head_ref)?
                        .unwrap_or_default();
                    if layers[index].upstream != parent_tip {
                        return Err(format!(
                            "{} is not on {}: restack the stack first",
                            folded.head_ref, parent.head_ref
                        ));
                    }
                    let dir = Path::new(&run.worktree).to_path_buf();
                    let message = format!("{} (#{})", folded.title, folded.number);
                    if let Err(error) = git::squash_into(
                        &desk.host,
                        &dir,
                        &parent.head_ref,
                        &folded.head_ref,
                        &message,
                    ) {
                        let _ = desk.restore(&run);
                        return Err(error);
                    }
                    let squashed = git::branch_tip(&desk.host, desk.dir(), &parent.head_ref)?
                        .unwrap_or_default();
                    run.steps = Run::steps_for(&layers[index + 1..], squashed);
                    if run.return_branch.as_deref() == Some(folded.head_ref.as_str()) {
                        run.return_branch = Some(parent.head_ref.clone());
                    }
                    desk.edit_and_push(&input, &mut run, |desk| {
                        git::delete_branch(&desk.host, desk.dir(), &folded.head_ref)
                    })
                })?;
                Ok((above, parent.head_ref))
            })
            .await?;
        if let Some(above) = above {
            self.retarget(above, parent).await?;
        }
        self.close(number).await
    }

    /// Take PR `number` out of the stack, keeping it open: it goes onto the
    /// base, and the PR above it onto the one below.
    pub(crate) async fn remove_from_stack(
        &self,
        input: StackInput,
        number: i64,
    ) -> Result<(), String> {
        let base_ref = input.base_ref.clone();
        let retargets = self
            .blocking("Removing from the stack failed", move |desk| {
                let open = input.open_prs();
                let index = Self::position(&input, number)?;
                git::locked(&desk.host, desk.dir(), || {
                    let (onto, mut run, layers) = desk.begin_edit(&input)?;
                    let below_tip = match index.checked_sub(1) {
                        Some(below) => layers[below].tip.clone(),
                        None => layers[index].upstream.clone(),
                    };
                    let mut steps = vec![Step {
                        number,
                        head_ref: layers[index].head_ref.clone(),
                        upstream: layers[index].upstream.clone(),
                        parent: Parent::Fixed(onto),
                        status: RestackStatus::Waiting,
                        commits: 0,
                        conflicts: Vec::new(),
                    }];
                    for (offset, layer) in layers[index + 1..].iter().enumerate() {
                        steps.push(Step {
                            number: layer.number,
                            head_ref: layer.head_ref.clone(),
                            upstream: layer.upstream.clone(),
                            parent: match offset {
                                0 => Parent::Fixed(below_tip.clone()),
                                _ => Parent::Step(offset),
                            },
                            status: RestackStatus::Waiting,
                            commits: 0,
                            conflicts: Vec::new(),
                        });
                    }
                    run.steps = steps;
                    desk.edit_and_push(&input, &mut run, |_| Ok(()))
                })?;
                let mut retargets = Vec::new();
                if index > 0 {
                    retargets.push((number, input.base_ref.clone()));
                }
                if let Some(above) = open.get(index + 1) {
                    retargets.push((above.number, Self::base_at(&input, &open, index)));
                }
                Ok(retargets)
            })
            .await?;
        self.retarget_all(retargets, &base_ref).await
    }

    /// Split PR `number` at commit `at`: a new git branch `name` holds its
    /// commits up to `at` as a new PR below it, and `number` keeps the rest.
    /// Nothing is rewritten. Returns the new PR's number.
    pub(crate) async fn split_branch(
        &self,
        input: StackInput,
        number: i64,
        split: SplitRequest,
    ) -> Result<i64, String> {
        let SplitRequest { at, name, title } = split;
        let branch = name.clone();
        let base = self
            .blocking("Splitting failed", move |desk| {
                desk.ensure_heads(&input.open_prs())?;
                let onto = desk.ensure_base(&input.base_ref, &input.base_sha)?;
                let open = input.open_prs();
                let index = Self::position(&input, number)?;
                let layers = desk.layers(&input, &onto, None)?;
                let layer = &layers[index];
                let inside = git::contains(&desk.host, desk.dir(), &at, &layer.tip)?
                    && !git::contains(&desk.host, desk.dir(), &at, &layer.upstream)?;
                if !inside || at == layer.tip {
                    return Err("Pick a commit of this PR below its last one".to_string());
                }
                git::locked(&desk.host, desk.dir(), || {
                    git::create_branch(&desk.host, desk.dir(), &name, &at)?;
                    let push = LeasedPush {
                        branch: name.clone(),
                        lease: None,
                    };
                    if let Err(error) =
                        git::push_leased(&desk.host, desk.dir(), &desk.remote()?, &[push])
                    {
                        let _ = git::delete_branch(&desk.host, desk.dir(), &name);
                        return Err(error);
                    }
                    Ok(())
                })?;
                Ok(Self::base_at(&input, &open, index))
            })
            .await?;
        let created = self
            .create_pr(NewPr {
                base,
                head: branch.clone(),
                title,
                body: String::new(),
                draft: false,
            })
            .await?;
        self.retarget(number, branch).await?;
        Ok(created)
    }

    /// Rename PR `number`'s git branch, on GitHub (its PRs follow) and here.
    pub(crate) async fn rename_stack_branch(
        &self,
        input: StackInput,
        number: i64,
        to: String,
    ) -> Result<(), String> {
        let from = input
            .open_prs()
            .into_iter()
            .find(|pr| pr.number == number)
            .map(|pr| pr.head_ref.clone())
            .ok_or_else(|| format!("#{number} is not an open PR in this stack"))?;
        if crate::git::checkouts::branch_tip(&self.host, self.dir(), &to)?.is_some() {
            return Err(format!("{to} already exists"));
        }
        let (from_github, to_github) = (from.clone(), to.clone());
        self.blocking("Renaming failed", move |desk| {
            desk.github.rename_branch(&from_github, &to_github)
        })
        .await?;
        self.blocking("Renaming failed", move |desk| {
            if git::branch_tip(&desk.host, desk.dir(), &from)?.is_none() {
                return Ok(());
            }
            git::locked(&desk.host, desk.dir(), || {
                git::rename_branch(&desk.host, desk.dir(), &from, &to)?;
                // `branch -m` keeps the upstream's old name, which GitHub no
                // longer has.
                git::follow_renamed_upstream(&desk.host, desk.dir(), &from, &to)
            })
        })
        .await
    }

    /// A new git branch `name` on top of the stack, or below PR `below`
    /// (which is retargeted onto it), pushed and checked out in the stack's
    /// worktree. Its PR opens once it has commits.
    pub(crate) async fn add_stack_branch(
        &self,
        input: StackInput,
        name: String,
        below: Option<i64>,
    ) -> Result<(), String> {
        let retarget = self
            .blocking("Adding the git branch failed", move |desk| {
                desk.ensure_heads(&input.open_prs())?;
                let onto = desk.ensure_base(&input.base_ref, &input.base_sha)?;
                let layers = desk.layers(&input, &onto, None)?;
                let at = match below {
                    Some(number) => layers[Self::position(&input, number)?].upstream.clone(),
                    None => layers.last().map(|layer| layer.tip.clone()).unwrap_or(onto),
                };
                let worktree = desk.stack_worktree(&input)?;
                git::locked(&desk.host, desk.dir(), || {
                    git::create_branch(&desk.host, desk.dir(), &name, &at)?;
                    let push = LeasedPush {
                        branch: name.clone(),
                        lease: None,
                    };
                    if let Err(error) =
                        git::push_leased(&desk.host, desk.dir(), &desk.remote()?, &[push])
                    {
                        let _ = git::delete_branch(&desk.host, desk.dir(), &name);
                        return Err(error);
                    }
                    if let Some(worktree) = worktree.filter(|worktree| worktree.clean) {
                        git::switch(&desk.host, Path::new(&worktree.path), &name)?;
                    }
                    Ok(())
                })?;
                Ok(below.map(|number| (number, name)))
            })
            .await?;
        match retarget {
            Some((number, base)) => self.retarget(number, base).await,
            None => Ok(()),
        }
    }

    /// Delete the local git branches of the stack's merged PRs. A worktree
    /// on one moves to the bottom open git branch first, or, when another
    /// worktree already has that (git checks a git branch out in one place
    /// only) or the switch fails, is detached where it is. Returns the git
    /// branches deleted.
    pub(crate) async fn delete_merged_branches(
        &self,
        input: StackInput,
    ) -> Result<Vec<String>, String> {
        self.blocking("Deleting merged git branches failed", move |desk| {
            let bottom = input.open_prs().first().map(|pr| pr.head_ref.clone());
            let mut checked_out = crate::git::worktrees::worktree_branches(&desk.host, desk.dir())?;
            git::locked(&desk.host, desk.dir(), || {
                let mut deleted = Vec::new();
                for pr in input.prs.iter().filter(|pr| pr.merged) {
                    if git::branch_tip(&desk.host, desk.dir(), &pr.head_ref)?.is_none() {
                        continue;
                    }
                    for index in 0..checked_out.len() {
                        if checked_out[index].1.as_deref() != Some(pr.head_ref.as_str()) {
                            continue;
                        }
                        let dir = Path::new(&checked_out[index].0).to_path_buf();
                        let free = bottom.as_ref().filter(|bottom| {
                            !checked_out
                                .iter()
                                .any(|(_, branch)| branch.as_deref() == Some(bottom.as_str()))
                        });
                        let moved = match free {
                            Some(bottom) => git::switch(&desk.host, &dir, bottom)
                                .is_ok()
                                .then(|| bottom.clone()),
                            None => None,
                        };
                        if moved.is_none() {
                            git::detach(&desk.host, &dir)?;
                        }
                        checked_out[index].1 = moved;
                    }
                    git::delete_branch(&desk.host, desk.dir(), &pr.head_ref)?;
                    deleted.push(pr.head_ref.clone());
                }
                Ok(deleted)
            })
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::super::stack_support::StackFixture;
    use crate::review::fake::Sent;
    use crate::review::stack_types::SplitRequest;

    fn retargets(sent: &[Sent]) -> Vec<(i64, String)> {
        sent.iter()
            .filter_map(|sent| match sent {
                Sent::Retarget { number, base } => Some((*number, base.clone())),
                _ => None,
            })
            .collect()
    }

    #[tokio::test]
    async fn pushing_the_stack_sends_what_moved_and_refuses_when_github_moved_on() {
        let Some(stack) = StackFixture::new().await else {
            return;
        };
        stack.advance_main("main.rs", "the base moved");
        let worktree = stack.worktree_on("feat/maki");
        stack.desk().restack(stack.input(), None).await.unwrap();

        let report = stack.desk().push_stack(stack.input(), None).await.unwrap();

        assert_eq!(
            report.pushed,
            vec!["feat/driver", "feat/claude", "feat/maki"]
        );
        for branch in report.pushed {
            assert_eq!(stack.remote_tip(&branch), stack.tip(&branch));
        }

        // Someone else pushes to feat/maki, so GitHub's head is no longer
        // the one this push leases on.
        let stale = stack.input();
        let clone = stack.inner.repo.repo.parent().unwrap().join("other");
        let remote = stack.inner.repo.remote.to_str().unwrap().to_string();
        stack.git(&["clone", "-q", &remote, clone.to_str().unwrap()]);
        stack.git_in(&clone, &["switch", "-q", "feat/maki"]);
        stack.git_in(
            &clone,
            &[
                "-c",
                "user.name=T",
                "-c",
                "user.email=t@e",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "theirs",
            ],
        );
        stack.git_in(&clone, &["push", "-q", "origin", "feat/maki"]);
        let theirs = stack.remote_tip("feat/maki");
        stack.git_in(&worktree, &["commit", "-q", "--allow-empty", "-m", "mine"]);

        let error = stack.desk().push_stack(stale, Some(418)).await.unwrap_err();

        assert!(error.contains("GitHub has commits"), "{error}");
        assert_eq!(stack.remote_tip("feat/maki"), theirs);
    }

    #[tokio::test]
    async fn a_local_git_branch_behind_github_moves_up_to_its_head_and_is_never_pushed_over_it() {
        let Some(stack) = StackFixture::new().await else {
            return;
        };
        // A teammate pushes to feat/maki; the local git branch stays behind.
        let theirs = stack.teammate_pushes(418, "feat/maki");

        let report = stack.desk().push_stack(stack.input(), None).await.unwrap();

        assert!(report.pushed.is_empty(), "{:?}", report.pushed);
        assert_eq!(stack.remote_tip("feat/maki"), theirs);
        assert_eq!(stack.tip("feat/maki"), theirs);
    }

    #[tokio::test]
    async fn a_checked_out_git_branch_behind_github_is_refused_until_pulled() {
        let Some(stack) = StackFixture::new().await else {
            return;
        };
        let worktree = stack.worktree_on("feat/maki");
        let mine = stack.tip("feat/maki");
        let theirs = stack.teammate_pushes(418, "feat/maki");

        let error = stack
            .desk()
            .push_stack(stack.input(), None)
            .await
            .unwrap_err();

        assert!(error.contains("Pull"), "{error}");
        assert_eq!(stack.remote_tip("feat/maki"), theirs);
        assert_eq!(stack.tip("feat/maki"), mine);

        // Diverged: a local commit on top of the old head as well.
        stack.git_in(&worktree, &["commit", "-q", "--allow-empty", "-m", "mine"]);
        let error = stack
            .desk()
            .push_stack(stack.input(), None)
            .await
            .unwrap_err();
        assert!(error.contains("not here"), "{error}");
        assert_eq!(stack.remote_tip("feat/maki"), theirs);
    }

    #[tokio::test]
    async fn a_restacked_git_branch_pushes_but_not_over_commits_pushed_since() {
        let Some(stack) = StackFixture::new().await else {
            return;
        };
        stack.advance_main("main.rs", "the base moved");
        stack.worktree_on("feat/claude");
        stack.desk().restack(stack.input(), None).await.unwrap();
        let theirs = stack.teammate_pushes(418, "feat/maki");

        let error = stack
            .desk()
            .push_stack(stack.input(), None)
            .await
            .unwrap_err();

        assert!(error.contains("feat/maki"), "{error}");
        assert_eq!(stack.remote_tip("feat/maki"), theirs);
        // The rewritten git branches below it still push.
        let report = stack
            .desk()
            .push_stack(stack.input(), Some(415))
            .await
            .unwrap();
        assert_eq!(report.pushed, vec!["feat/claude"]);
    }

    #[tokio::test]
    async fn reordering_rebases_each_git_branch_onto_its_new_parent_and_retargets() {
        let Some(stack) = StackFixture::new().await else {
            return;
        };
        stack.worktree_on("feat/maki");
        stack.inner.github.merge_on_push(&stack.inner.repo.remote);

        stack
            .desk()
            .reorder_stack(stack.input(), vec![412, 418, 415])
            .await
            .unwrap();

        assert_eq!(stack.inner.github.merged_by_push(), Vec::<i64>::new());

        assert_eq!(
            stack.subjects("feat/driver", "feat/maki"),
            vec!["add maki.rs"]
        );
        assert_eq!(
            stack.subjects("feat/maki", "feat/claude"),
            vec!["add claude.rs"]
        );
        assert_eq!(stack.remote_tip("feat/claude"), stack.tip("feat/claude"));
        // Onto the base before the push, so no PR's head ever lands in its
        // base, then onto the new parents.
        assert_eq!(
            retargets(&stack.inner.github.sent()),
            vec![
                (418, "main".to_string()),
                (415, "main".to_string()),
                (418, "feat/driver".to_string()),
                (415, "feat/maki".to_string())
            ]
        );
    }

    #[tokio::test]
    async fn swapping_two_prs_never_lets_github_mark_one_merged() {
        let Some(stack) = StackFixture::new().await else {
            return;
        };
        stack.worktree_on("feat/maki");
        stack.inner.github.merge_on_push(&stack.inner.repo.remote);

        stack
            .desk()
            .reorder_stack(stack.input(), vec![415, 412, 418])
            .await
            .unwrap();

        assert_eq!(stack.inner.github.merged_by_push(), Vec::<i64>::new());
        assert_eq!(stack.subjects("main", "feat/claude"), vec!["add claude.rs"]);
        assert_eq!(stack.remote_tip("feat/driver"), stack.tip("feat/driver"));
    }

    #[tokio::test]
    async fn a_reorder_that_conflicts_moves_nothing() {
        let Some(stack) = StackFixture::new().await else {
            return;
        };
        // #418 now edits claude.rs too, so it cannot go below #415.
        stack.git(&["switch", "-q", "feat/maki"]);
        std::fs::write(stack.inner.repo.repo.join("claude.rs"), "maki's edit\n").unwrap();
        stack.git(&["commit", "-q", "-am", "maki edits claude"]);
        stack.git(&["switch", "-q", "main"]);
        stack.worktree_on("feat/maki");
        let before: Vec<String> = ["feat/driver", "feat/claude", "feat/maki"]
            .iter()
            .map(|b| stack.tip(b))
            .collect();

        let error = stack
            .desk()
            .reorder_stack(stack.input(), vec![412, 418, 415])
            .await
            .unwrap_err();

        assert!(error.contains("Nothing moved"), "{error}");
        let after: Vec<String> = ["feat/driver", "feat/claude", "feat/maki"]
            .iter()
            .map(|b| stack.tip(b))
            .collect();
        assert_eq!(after, before);
        assert!(retargets(&stack.inner.github.sent()).is_empty());
    }

    #[tokio::test]
    async fn folding_squashes_into_the_parent_and_closes_the_pr() {
        let Some(stack) = StackFixture::new().await else {
            return;
        };
        let worktree = stack.worktree_on("feat/claude");

        stack
            .desk()
            .fold_into_parent(stack.input(), 415)
            .await
            .unwrap();

        assert_eq!(
            stack.subjects("main", "feat/driver"),
            vec!["add driver.rs", "Add claude.rs (#415)"]
        );
        assert_eq!(
            stack.subjects("feat/driver", "feat/maki"),
            vec!["add maki.rs"]
        );
        assert!(stack.git(&["branch", "--list", "feat/claude"]).is_empty());
        assert_eq!(
            stack.git_in(&worktree, &["branch", "--show-current"]),
            "feat/driver"
        );
        assert_eq!(stack.remote_tip("feat/driver"), stack.tip("feat/driver"));
        let sent = stack.inner.github.sent();
        assert_eq!(retargets(&sent), vec![(418, "feat/driver".to_string())]);
        assert!(sent.contains(&Sent::Close(415)));
    }

    #[tokio::test]
    async fn removing_from_the_stack_retargets_onto_the_base_and_keeps_the_pr_open() {
        let Some(stack) = StackFixture::new().await else {
            return;
        };
        stack.worktree_on("feat/maki");

        stack
            .desk()
            .remove_from_stack(stack.input(), 415)
            .await
            .unwrap();

        assert_eq!(stack.subjects("main", "feat/claude"), vec!["add claude.rs"]);
        assert_eq!(
            stack.subjects("feat/driver", "feat/maki"),
            vec!["add maki.rs"]
        );
        let sent = stack.inner.github.sent();
        assert_eq!(
            retargets(&sent),
            vec![(415, "main".to_string()), (418, "feat/driver".to_string())]
        );
        assert!(!sent.iter().any(|sent| matches!(sent, Sent::Close(_))));
    }

    #[tokio::test]
    async fn splitting_opens_a_pr_for_the_lower_commits_without_rewriting() {
        let Some(stack) = StackFixture::new().await else {
            return;
        };
        let first = stack.tip("feat/claude");
        stack.git(&["switch", "-q", "feat/claude"]);
        stack.git(&["commit", "-q", "--allow-empty", "-m", "second"]);
        stack.git(&["switch", "-q", "main"]);
        let tip = stack.tip("feat/claude");

        let number = stack
            .desk()
            .split_branch(
                stack.input(),
                415,
                SplitRequest {
                    at: first.clone(),
                    name: "feat/claude-base".into(),
                    title: "Claude base".into(),
                },
            )
            .await
            .unwrap();

        assert_eq!(stack.remote_tip("feat/claude-base"), first);
        assert_eq!(stack.tip("feat/claude"), tip);
        let sent = stack.inner.github.sent();
        assert!(sent.iter().any(|sent| matches!(
            sent,
            Sent::CreatePr(pr) if pr.head == "feat/claude-base" && pr.base == "feat/driver"
        )));
        assert_eq!(
            retargets(&sent),
            vec![(415, "feat/claude-base".to_string())]
        );
        assert!(number >= 1000);
    }

    #[tokio::test]
    async fn renaming_renames_on_github_and_here() {
        let Some(stack) = StackFixture::new().await else {
            return;
        };
        let tip = stack.tip("feat/claude");

        stack
            .desk()
            .rename_stack_branch(stack.input(), 415, "feat/claude-code".into())
            .await
            .unwrap();

        assert_eq!(stack.tip("feat/claude-code"), tip);
        assert!(stack.inner.github.sent().contains(&Sent::RenameBranch {
            from: "feat/claude".into(),
            to: "feat/claude-code".into()
        }));
    }

    #[tokio::test]
    async fn a_git_branch_goes_on_top_or_below_a_pr() {
        let Some(stack) = StackFixture::new().await else {
            return;
        };
        let worktree = stack.worktree_on("feat/maki");

        stack
            .desk()
            .add_stack_branch(stack.input(), "feat/next".into(), None)
            .await
            .unwrap();
        assert_eq!(stack.tip("feat/next"), stack.tip("feat/maki"));
        assert_eq!(stack.remote_tip("feat/next"), stack.tip("feat/maki"));
        assert_eq!(
            stack.git_in(&worktree, &["branch", "--show-current"]),
            "feat/next"
        );

        stack.git_in(&worktree, &["switch", "-q", "feat/maki"]);
        stack
            .desk()
            .add_stack_branch(stack.input(), "feat/mid".into(), Some(415))
            .await
            .unwrap();
        assert_eq!(stack.tip("feat/mid"), stack.tip("feat/driver"));
        assert_eq!(
            retargets(&stack.inner.github.sent()),
            vec![(415, "feat/mid".to_string())]
        );
    }

    #[tokio::test]
    async fn renaming_points_the_local_git_branch_at_its_new_upstream() {
        let Some(stack) = StackFixture::new().await else {
            return;
        };
        stack.git(&[
            "branch",
            "--set-upstream-to=origin/feat/claude",
            "feat/claude",
        ]);

        stack
            .desk()
            .rename_stack_branch(stack.input(), 415, "feat/claude-code".into())
            .await
            .unwrap();

        assert_eq!(
            stack.git(&["config", "branch.feat/claude-code.merge"]),
            "refs/heads/feat/claude-code"
        );
        assert_eq!(
            stack.git(&["config", "branch.feat/claude-code.remote"]),
            "origin"
        );
    }

    #[tokio::test]
    async fn a_merged_git_branch_is_deleted_when_the_bottom_one_is_checked_out_elsewhere() {
        let Some(stack) = StackFixture::new().await else {
            return;
        };
        let stack_worktree = stack.worktree_on("feat/claude");
        let merged = stack.worktree_on("feat/driver");
        let at = stack.tip("feat/driver");
        stack.mark_merged(412);

        let deleted = stack
            .desk()
            .delete_merged_branches(stack.input())
            .await
            .unwrap();

        assert_eq!(deleted, vec!["feat/driver"]);
        assert!(stack.git(&["branch", "--list", "feat/driver"]).is_empty());
        assert_eq!(stack.git_in(&merged, &["branch", "--show-current"]), "");
        assert_eq!(stack.git_in(&merged, &["rev-parse", "HEAD"]), at);
        assert_eq!(
            stack.git_in(&stack_worktree, &["branch", "--show-current"]),
            "feat/claude"
        );
    }

    #[tokio::test]
    async fn merged_git_branches_are_deleted_and_their_worktree_moves_up() {
        let Some(stack) = StackFixture::new().await else {
            return;
        };
        let worktree = stack.worktree_on("feat/driver");
        stack.mark_merged(412);

        let deleted = stack
            .desk()
            .delete_merged_branches(stack.input())
            .await
            .unwrap();

        assert_eq!(deleted, vec!["feat/driver"]);
        assert!(stack.git(&["branch", "--list", "feat/driver"]).is_empty());
        assert_eq!(
            stack.git_in(&worktree, &["branch", "--show-current"]),
            "feat/claude"
        );
    }
}
