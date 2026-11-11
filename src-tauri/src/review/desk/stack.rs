//! A stack as it is on disk: each open PR's git branch and how many commits
//! it adds, how far the base has moved on, the stack's worktree, and the
//! restack plan.
//!
//! The stack itself is GitHub's (`StackInput`, built by the frontend from the
//! inbox and the native stack listing); nothing here infers one. A git branch
//! with no local copy reads at its PR head, fetched under `refs/pingex/pr/`.
//!
//! The restack plan replays each open git branch, bottom first, onto the one
//! below it, and the bottom one onto the base's tip. A git branch's own
//! commits start after the tip of the git branch below it (or, for the bottom
//! one, after the merged PR below it or the fork point from the base).
//! Expected conflicts come from one trial merge of the whole stack onto the
//! base, each file blamed on the lowest git branch that changes it.

use std::collections::HashMap;
use std::path::Path;

use super::ReviewDesk;
use crate::git::{objects, stack as git, worktrees};
use crate::review::stack_types::{
    PrBaseInput, PrBehind, RestackPlan, RestackPlanStep, StackBranch, StackInput, StackInputPr,
    StackOverview, StackWorktree,
};
use crate::review::worktree_purpose::WorktreePurpose;

/// The base's newest commits the Stack page lists.
const BASE_COMMITS_SHOWN: usize = 5;
/// The commits listed per git branch, for the split picker.
const BRANCH_COMMITS_SHOWN: usize = 50;

/// Where one open git branch's commits start, and what it builds on.
#[derive(Clone, Debug)]
pub(super) struct Layer {
    pub(super) number: i64,
    pub(super) head_ref: String,
    pub(super) tip: String,
    /// The last commit below this git branch's own commits.
    pub(super) upstream: String,
    /// Whether a local git branch exists.
    pub(super) local: bool,
}

impl ReviewDesk {
    pub(super) fn remote(&self) -> Result<String, String> {
        let slug = self.github.repo()?;
        objects::remote_for_slug(&self.host, self.dir(), &slug)?
            .ok_or_else(|| format!("No remote in this repository points at {slug}"))
    }

    /// Make sure `sha` is here, fetching `base_ref` if it is not.
    pub(super) fn ensure_base(&self, base_ref: &str, sha: &str) -> Result<String, String> {
        if crate::git::checkouts::has_commit(&self.host, self.dir(), sha)? {
            return Ok(sha.to_string());
        }
        let remote = self.remote()?;
        // Forced: the base may have been force-pushed since the last fetch.
        let spec = format!("+refs/heads/{base_ref}:refs/remotes/{remote}/{base_ref}");
        let output = crate::git::run::run_git_network(
            &self.host,
            self.dir(),
            &["fetch", "--no-tags", "--quiet", &remote, &spec],
        )?;
        if !output.ok {
            return Err(crate::git::run::redact_git_error(
                &format!("Could not fetch {base_ref}"),
                &output,
            ));
        }
        if crate::git::checkouts::has_commit(&self.host, self.dir(), sha)? {
            return Ok(sha.to_string());
        }
        // GitHub has moved on since the inbox synced: use what was fetched.
        objects::resolve(
            &self.host,
            self.dir(),
            &format!("refs/remotes/{remote}/{base_ref}"),
        )?
        .ok_or_else(|| format!("Could not read {base_ref}"))
    }

    /// Fetch the PR heads of `prs` that are not here, local git branch or
    /// not: `catch_up_branches` compares a local tip with GitHub's head.
    pub(super) fn ensure_heads(&self, prs: &[&StackInputPr]) -> Result<(), String> {
        let heads: Vec<(i64, &str)> = prs
            .iter()
            .map(|pr| (pr.number, pr.head_sha.as_str()))
            .collect();
        self.ensure_head_shas(&heads)
    }

    /// Fetch, in one go, the PR heads of `(number, head_sha)` that are not
    /// here.
    fn ensure_head_shas(&self, heads: &[(i64, &str)]) -> Result<(), String> {
        let mut missing = Vec::new();
        for (number, sha) in heads {
            if !crate::git::checkouts::has_commit(&self.host, self.dir(), sha)? {
                missing.push(super::pr_number(*number)?);
            }
        }
        if !missing.is_empty() {
            objects::fetch_pr_heads(&self.host, self.dir(), &self.remote()?, &missing)?;
        }
        Ok(())
    }

    /// Make sure no local git branch of `prs` is missing commits GitHub has,
    /// before anything replays or pushes it: pushing a stale tip, even
    /// leased, would overwrite them. A git branch has GitHub's head when its
    /// tip, or a tip it had before a rewrite such as a restack, contains it
    /// (`integrates`). Otherwise, one behind GitHub that no worktree has
    /// checked out moves up to GitHub's head; one checked out, or one with
    /// commits of its own as well, is refused until pulled. Call after
    /// `ensure_heads`, holding the repository's lock.
    pub(super) fn catch_up_branches(&self, prs: &[&StackInputPr]) -> Result<(), String> {
        let mut checked_out: Option<Vec<(String, Option<String>)>> = None;
        for pr in prs {
            let Some(tip) = git::branch_tip(&self.host, self.dir(), &pr.head_ref)? else {
                continue;
            };
            if tip == pr.head_sha {
                continue;
            }
            if !crate::git::checkouts::has_commit(&self.host, self.dir(), &pr.head_sha)? {
                return Err(format!(
                    "Could not fetch GitHub's head of {}. Sync, then try again",
                    pr.head_ref
                ));
            }
            if git::integrates(&self.host, self.dir(), &pr.head_ref, &pr.head_sha)? {
                // Ahead of GitHub, or rewritten from it: commits to push.
                continue;
            }
            if !git::contains(&self.host, self.dir(), &tip, &pr.head_sha)? {
                return Err(format!(
                    "GitHub has commits on {} that are not here, and it has commits of its own. Pull them into it, then try again",
                    pr.head_ref
                ));
            }
            let worktrees = match &mut checked_out {
                Some(worktrees) => worktrees,
                None => checked_out.insert(worktrees::worktree_branches(&self.host, self.dir())?),
            };
            if let Some((path, _)) = worktrees
                .iter()
                .find(|(_, branch)| branch.as_deref() == Some(pr.head_ref.as_str()))
            {
                return Err(format!(
                    "{} is behind GitHub. Pull it in {path}, then try again",
                    pr.head_ref
                ));
            }
            git::fast_forward(&self.host, self.dir(), &pr.head_ref, &tip, &pr.head_sha)?;
        }
        Ok(())
    }

    /// The local tip of `pr`'s git branch, or its PR head.
    fn tip_of(&self, pr: &StackInputPr) -> Result<(String, bool), String> {
        Ok(
            match git::branch_tip(&self.host, self.dir(), &pr.head_ref)? {
                Some(tip) => (tip, true),
                None => (pr.head_sha.clone(), false),
            },
        )
    }

    /// The open git branches from `from` up (all of them by default), with
    /// where each one's own commits start. `onto` is the base's tip.
    pub(super) fn layers(
        &self,
        input: &StackInput,
        onto: &str,
        from: Option<i64>,
    ) -> Result<Vec<Layer>, String> {
        let open = input.open_prs();
        let start = match from {
            Some(number) => open
                .iter()
                .position(|pr| pr.number == number)
                .ok_or_else(|| format!("#{number} is not an open PR in this stack"))?,
            None => 0,
        };
        let mut layers: Vec<Layer> = Vec::new();
        let mut below: Option<String> = None;
        for (index, pr) in open.iter().enumerate() {
            let (tip, local) = self.tip_of(pr)?;
            let upstream = match &below {
                Some(lower) => self.own_start(&tip, lower)?,
                None => self.bottom_start(input, pr, &tip, onto)?,
            };
            below = Some(tip.clone());
            if index >= start {
                layers.push(Layer {
                    number: pr.number,
                    head_ref: pr.head_ref.clone(),
                    tip,
                    upstream,
                    local,
                });
            }
        }
        Ok(layers)
    }

    /// Where `tip`'s own commits start over `lower`: `lower` itself when it
    /// is an ancestor, else where the two forked.
    fn own_start(&self, tip: &str, lower: &str) -> Result<String, String> {
        if git::contains(&self.host, self.dir(), lower, tip)? {
            return Ok(lower.to_string());
        }
        git::merge_base(&self.host, self.dir(), tip, lower)?
            .ok_or_else(|| "The stack's git branches share no history".to_string())
    }

    /// Where the bottom open git branch's own commits start: after the
    /// merged PR right below it, else at its fork point from the base.
    fn bottom_start(
        &self,
        input: &StackInput,
        bottom: &StackInputPr,
        tip: &str,
        onto: &str,
    ) -> Result<String, String> {
        let index = input
            .prs
            .iter()
            .position(|pr| pr.number == bottom.number)
            .unwrap_or(0);
        if let Some(merged) = index
            .checked_sub(1)
            .and_then(|below| input.prs.get(below))
            .filter(|pr| pr.merged)
        {
            let (merged_tip, _) = self.tip_of(merged)?;
            if crate::git::checkouts::has_commit(&self.host, self.dir(), &merged_tip)? {
                return self.own_start(tip, &merged_tip);
            }
        }
        git::merge_base(&self.host, self.dir(), tip, onto)?
            .ok_or_else(|| "The stack shares no history with its base".to_string())
    }

    /// The worktree one of the stack's open git branches is checked out in.
    pub(super) fn stack_worktree(
        &self,
        input: &StackInput,
    ) -> Result<Option<StackWorktree>, String> {
        let branches: Vec<&str> = input
            .open_prs()
            .iter()
            .map(|pr| pr.head_ref.as_str())
            .collect();
        let Some((path, branch)) = worktrees::worktree_branches(&self.host, self.dir())?
            .into_iter()
            .find(|(_, branch)| {
                branch
                    .as_deref()
                    .is_some_and(|name| branches.contains(&name))
            })
        else {
            return Ok(None);
        };
        let clean = crate::git::checkouts::is_clean(&self.host, Path::new(&path))?;
        Ok(Some(StackWorktree {
            path,
            branch,
            clean,
        }))
    }

    /// The stack's git branches on disk, the base's lead and its worktree.
    pub(crate) async fn stack_overview(&self, input: StackInput) -> Result<StackOverview, String> {
        let stored = self.stored_restack(input.number).await?;
        let purpose = WorktreePurpose::stack(&self.repo_key(), &input);
        let mut overview = self
            .blocking("Reading the stack failed", move |desk| {
                let open = input.open_prs();
                desk.ensure_heads(&open)?;
                let onto = desk.ensure_base(&input.base_ref, &input.base_sha)?;
                let layers = desk.layers(&input, &onto, None)?;
                let mut branches = Vec::new();
                for (layer, pr) in layers.iter().zip(&open) {
                    branches.push(StackBranch {
                        number: layer.number,
                        head_ref: layer.head_ref.clone(),
                        tip: layer.tip.clone(),
                        commits: git::count_between(
                            &desk.host,
                            desk.dir(),
                            &layer.upstream,
                            &layer.tip,
                        )?,
                        unpushed: layer.local && layer.tip != pr.head_sha,
                        local: layer.local,
                        commit_lines: git::commits_between(
                            &desk.host,
                            desk.dir(),
                            &layer.upstream,
                            &layer.tip,
                            BRANCH_COMMITS_SHOWN,
                        )?,
                    });
                }
                let (base_ahead, base_commits) = match layers.first() {
                    Some(bottom) => (
                        git::count_between(&desk.host, desk.dir(), &bottom.tip, &onto)?,
                        git::commits_between(
                            &desk.host,
                            desk.dir(),
                            &bottom.tip,
                            &onto,
                            BASE_COMMITS_SHOWN,
                        )?,
                    ),
                    None => (0, Vec::new()),
                };
                let mut merged_branches = Vec::new();
                for pr in input.prs.iter().filter(|pr| pr.merged) {
                    if git::branch_tip(&desk.host, desk.dir(), &pr.head_ref)?.is_some() {
                        merged_branches.push(pr.number);
                    }
                }
                Ok(StackOverview {
                    number: input.number,
                    base_ahead,
                    base_commits,
                    branches,
                    worktree: desk.stack_worktree(&input)?,
                    merged_branches,
                    restack: None,
                })
            })
            .await?;
        overview.restack = stored.map(|run| run.progress());
        if let Some(worktree) = &overview.worktree {
            self.record_stack_worktree(&worktree.path, &purpose).await?;
        }
        Ok(overview)
    }

    /// How far each PR is behind its base, both as GitHub has them: the
    /// commits on `base_sha` that `head_sha` lacks. For a PR in a stack the
    /// base is the PR below's git branch, so this also catches a PR that no
    /// longer sits on the head of the one below it. A PR whose commits
    /// cannot be fetched here counts as unknown, not as an error.
    pub(crate) async fn prs_behind(
        &self,
        inputs: Vec<PrBaseInput>,
    ) -> Result<Vec<PrBehind>, String> {
        self.blocking("Reading the PRs failed", move |desk| {
            let heads: Vec<(i64, &str)> = inputs
                .iter()
                .map(|input| (input.number, input.head_sha.as_str()))
                .collect();
            // One failed batch fetch leaves each PR to its own count below.
            let _ = desk.ensure_head_shas(&heads);
            let mut bases: HashMap<(&str, &str), Option<String>> = HashMap::new();
            Ok(inputs
                .iter()
                .map(|input| {
                    let onto = bases
                        .entry((input.base_ref.as_str(), input.base_sha.as_str()))
                        .or_insert_with(|| desk.ensure_base(&input.base_ref, &input.base_sha).ok())
                        .clone();
                    let behind = onto.and_then(|onto| {
                        git::count_between(&desk.host, desk.dir(), &input.head_sha, &onto).ok()
                    });
                    PrBehind {
                        number: input.number,
                        behind,
                    }
                })
                .collect())
        })
        .await
    }

    /// What restacking (from `from` up, or the whole stack) would do.
    pub(crate) async fn restack_plan(
        &self,
        input: StackInput,
        from: Option<i64>,
    ) -> Result<RestackPlan, String> {
        self.blocking("Planning the restack failed", move |desk| {
            desk.ensure_heads(&input.open_prs())?;
            let onto = desk.ensure_base(&input.base_ref, &input.base_sha)?;
            let layers = desk.layers(&input, &onto, from)?;
            desk.plan_from_layers(&input, &onto, &layers)
        })
        .await
    }

    fn plan_from_layers(
        &self,
        input: &StackInput,
        onto: &str,
        layers: &[Layer],
    ) -> Result<RestackPlan, String> {
        let open = input.open_prs();
        let mut steps = Vec::new();
        let mut parents = Vec::new();
        for (index, layer) in layers.iter().enumerate() {
            let position = open
                .iter()
                .position(|pr| pr.number == layer.number)
                .unwrap_or(0);
            let (onto_label, new_parent) =
                match position.checked_sub(1).and_then(|below| open.get(below)) {
                    Some(below) => (below.head_ref.clone(), self.tip_of(below)?.0),
                    None => (input.base_ref.clone(), onto.to_string()),
                };
            // A layer moves when its parent is new, or when any layer below
            // it moves.
            let below_moves = index > 0
                && !steps
                    .last()
                    .is_some_and(|step: &RestackPlanStep| step.unchanged);
            steps.push(RestackPlanStep {
                number: layer.number,
                head_ref: layer.head_ref.clone(),
                onto: onto_label,
                commits: git::count_between(&self.host, self.dir(), &layer.upstream, &layer.tip)?,
                unchanged: !below_moves && new_parent == layer.upstream,
                conflicts: Vec::new(),
            });
            parents.push(new_parent);
        }
        let first = steps.iter().position(|step| !step.unchanged);
        let (predicted, conflicts) = match first {
            Some(first) => self.predict(&parents[first], &layers[first..])?,
            None => (true, Vec::new()),
        };
        let offset = first.unwrap_or(0);
        for (index, files) in conflicts.into_iter().enumerate() {
            steps[offset + index].conflicts = files;
        }
        let rewrites_pushed = steps
            .iter()
            .filter(|step| !step.unchanged)
            .map(|step| step.number)
            .collect();
        Ok(RestackPlan {
            onto_ref: input.base_ref.clone(),
            steps,
            rewrites_pushed,
            predicted,
            worktree: self.stack_worktree(input)?.map(|worktree| worktree.path),
        })
    }

    /// One trial merge of `layers` (the moving part of the stack, bottom
    /// first) onto `new_parent`, each conflicted file blamed on the lowest
    /// layer that changes it.
    fn predict(
        &self,
        new_parent: &str,
        layers: &[Layer],
    ) -> Result<(bool, Vec<Vec<String>>), String> {
        let mut blamed = vec![Vec::new(); layers.len()];
        let (Some(bottom), Some(top)) = (layers.first(), layers.last()) else {
            return Ok((true, blamed));
        };
        let Some(files) = git::predict_conflicts(
            &self.host,
            self.dir(),
            &bottom.upstream,
            new_parent,
            &top.tip,
        )?
        else {
            return Ok((false, blamed));
        };
        for file in files {
            for (index, layer) in layers.iter().enumerate() {
                if git::changed_files(&self.host, self.dir(), &layer.upstream, &layer.tip)?
                    .contains(&file)
                {
                    blamed[index].push(file);
                    break;
                }
            }
        }
        Ok((true, blamed))
    }
}

#[cfg(test)]
mod tests {
    use super::super::stack_support::StackFixture;
    use super::{PrBaseInput, PrBehind};

    #[tokio::test]
    async fn the_overview_counts_each_git_branch_the_base_lead_and_the_worktree_state() {
        let Some(stack) = StackFixture::new().await else {
            return;
        };
        stack.advance_main("main.rs", "the base moved");
        let worktree = stack.worktree_on("feat/maki");

        let overview = stack.desk().stack_overview(stack.input()).await.unwrap();

        assert_eq!(
            overview
                .branches
                .iter()
                .map(|branch| (branch.number, branch.commits))
                .collect::<Vec<_>>(),
            vec![(412, 1), (415, 1), (418, 1)]
        );
        assert_eq!(overview.base_ahead, 1);
        assert_eq!(overview.base_commits[0].subject, "the base moved");
        let found = overview.worktree.unwrap();
        assert_eq!(found.branch.as_deref(), Some("feat/maki"));
        assert!(found.clean);

        std::fs::write(worktree.join("scratch.txt"), "dirty\n").unwrap();
        let dirty = stack.desk().stack_overview(stack.input()).await.unwrap();
        assert!(!dirty.worktree.unwrap().clean);
    }

    fn base_input(stack: &StackFixture, number: i64, branch: &str, base: &str) -> PrBaseInput {
        PrBaseInput {
            number,
            head_sha: stack.remote_tip(branch),
            base_ref: base.to_string(),
            base_sha: stack.remote_tip(base),
        }
    }

    fn counts(behind: &[PrBehind]) -> Vec<(i64, Option<i64>)> {
        behind.iter().map(|pr| (pr.number, pr.behind)).collect()
    }

    #[tokio::test]
    async fn a_pr_counts_the_commits_its_base_has_moved_on_by() {
        let Some(stack) = StackFixture::new().await else {
            return;
        };
        stack.advance_main("one.rs", "one");
        stack.advance_main("two.rs", "two");

        let behind = stack
            .desk()
            .prs_behind(vec![
                base_input(&stack, 412, "feat/driver", "main"),
                base_input(&stack, 415, "feat/claude", "feat/driver"),
            ])
            .await
            .unwrap();

        assert_eq!(counts(&behind), vec![(412, Some(2)), (415, Some(0))]);
    }

    #[tokio::test]
    async fn a_push_to_the_pr_below_puts_the_one_above_behind_it() {
        let Some(stack) = StackFixture::new().await else {
            return;
        };
        // Not fetched here: the count has to fetch the new base tip.
        stack.teammate_pushes(415, "feat/claude");

        let behind = stack
            .desk()
            .prs_behind(vec![
                base_input(&stack, 412, "feat/driver", "main"),
                base_input(&stack, 415, "feat/claude", "feat/driver"),
                base_input(&stack, 418, "feat/maki", "feat/claude"),
            ])
            .await
            .unwrap();

        assert_eq!(
            counts(&behind),
            vec![(412, Some(0)), (415, Some(0)), (418, Some(1))]
        );
    }

    #[tokio::test]
    async fn a_head_that_is_not_here_is_fetched_before_counting() {
        let Some(stack) = StackFixture::new().await else {
            return;
        };
        let head = stack.teammate_pushes(412, "feat/driver");
        stack.advance_main("one.rs", "one");

        let behind = stack
            .desk()
            .prs_behind(vec![PrBaseInput {
                number: 412,
                head_sha: head,
                base_ref: "main".to_string(),
                base_sha: stack.remote_tip("main"),
            }])
            .await
            .unwrap();

        assert_eq!(counts(&behind), vec![(412, Some(1))]);
    }

    #[tokio::test]
    async fn a_pr_whose_commits_cannot_be_fetched_is_unknown_not_an_error() {
        let Some(stack) = StackFixture::new().await else {
            return;
        };

        let behind = stack
            .desk()
            .prs_behind(vec![
                PrBaseInput {
                    number: 999,
                    head_sha: "0123456789abcdef0123456789abcdef01234567".to_string(),
                    base_ref: "main".to_string(),
                    base_sha: stack.remote_tip("main"),
                },
                base_input(&stack, 412, "feat/driver", "main"),
            ])
            .await
            .unwrap();

        assert_eq!(counts(&behind), vec![(999, None), (412, Some(0))]);
    }

    #[tokio::test]
    async fn the_plan_lists_every_git_branch_and_blames_the_expected_conflict() {
        let Some(stack) = StackFixture::new().await else {
            return;
        };
        // main and #415 both change claude.rs.
        stack.advance_main("claude.rs", "main's version\n");

        let plan = stack
            .desk()
            .restack_plan(stack.input(), None)
            .await
            .unwrap();

        assert_eq!(
            plan.steps
                .iter()
                .map(|step| (step.number, step.onto.as_str(), step.unchanged))
                .collect::<Vec<_>>(),
            vec![
                (412, "main", false),
                (415, "feat/driver", false),
                (418, "feat/claude", false)
            ]
        );
        assert!(plan.predicted);
        assert_eq!(plan.steps[1].conflicts, vec!["claude.rs".to_string()]);
        assert!(plan.steps[0].conflicts.is_empty());
        assert_eq!(plan.rewrites_pushed, vec![412, 415, 418]);
    }

    #[tokio::test]
    async fn the_stack_reads_after_its_base_was_force_pushed() {
        let Some(stack) = StackFixture::new().await else {
            return;
        };
        stack.advance_main("main.rs", "the base moved");
        stack.git(&["fetch", "-q", "origin"]);
        // Someone rewrites main's last commit and force-pushes it.
        stack.as_teammate(|clone| {
            stack.git_in(clone, &["commit", "-q", "--amend", "-m", "rewritten"]);
            stack.git_in(clone, &["push", "-q", "--force", "origin", "main"]);
        });

        let overview = stack.desk().stack_overview(stack.input()).await.unwrap();

        assert_eq!(overview.base_commits[0].subject, "rewritten");
    }

    #[tokio::test]
    async fn a_stack_already_on_its_base_plans_nothing() {
        let Some(stack) = StackFixture::new().await else {
            return;
        };
        let plan = stack
            .desk()
            .restack_plan(stack.input(), None)
            .await
            .unwrap();
        assert!(plan.steps.iter().all(|step| step.unchanged));
        assert!(plan.rewrites_pushed.is_empty());
    }
}
