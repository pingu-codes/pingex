//! Checking out a whole stack: a local git branch for each open PR, tracking
//! GitHub's, and one worktree on the top one.
//!
//! One worktree, not one per PR: a restack rebases every git branch inside one
//! directory, and git refuses a branch checked out in another worktree. Switching
//! PR switches that worktree (`ensure_pr_worktree`).
//!
//! Nothing here moves work. A local git branch that has diverged from GitHub,
//! or is checked out and behind it, stays where it is and is reported. Safe to
//! run again: it only fills in what is missing.

use super::ReviewDesk;
use crate::git::{checkouts as checkouts_git, stack as git};
use crate::review::checkout_types::CheckoutTarget;
use crate::review::stack_types::{KeptBranch, StackCheckout, StackInput, StackInputPr};

impl ReviewDesk {
    /// Make every open PR's local git branch, then give the stack a worktree
    /// unless it has one.
    pub(crate) async fn ensure_stack_worktree(
        &self,
        input: StackInput,
    ) -> Result<StackCheckout, String> {
        let top = stack_target(&input)?;
        // A stopped restack leaves its worktree detached, where it can't be
        // found by branch, and needs every git branch left free.
        if let Some(run) = self.stored_restack(input.number).await? {
            return Err(format!(
                "A restack is stopped in {}. Continue or abort it first",
                run.worktree
            ));
        }
        let (created_branches, kept, has_worktree) = self
            .blocking("Checking out the stack failed", move |desk| {
                let open = input.open_prs();
                desk.ensure_heads(&open)?;
                let (created, kept) =
                    git::locked(&desk.host, desk.dir(), || desk.local_branches(&open))?;
                let remote = desk.remote()?;
                for pr in &open {
                    checkouts_git::track_remote_branch(
                        &desk.host,
                        desk.dir(),
                        &pr.head_ref,
                        &remote,
                    )?;
                }
                Ok((created, kept, desk.stack_worktree(&input)?.is_some()))
            })
            .await?;
        let checkout = if has_worktree {
            self.synced_checkout(top).await?
        } else {
            self.ensure_pr_worktree(top).await?
        };
        Ok(StackCheckout {
            checkout,
            created: !has_worktree,
            created_branches,
            kept,
        })
    }

    /// Catch each git branch up with GitHub where nothing can be lost, and make
    /// the missing ones. Returns the ones made and the ones left as they are.
    /// Call after `ensure_heads`, holding the repository's lock.
    fn local_branches(
        &self,
        open: &[&StackInputPr],
    ) -> Result<(Vec<String>, Vec<KeptBranch>), String> {
        let mut created = Vec::new();
        let mut kept = Vec::new();
        for pr in open {
            if git::branch_tip(&self.host, self.dir(), &pr.head_ref)?.is_none() {
                git::create_branch(&self.host, self.dir(), &pr.head_ref, &pr.head_sha)?;
                created.push(pr.head_ref.clone());
            } else if let Err(reason) = self.catch_up_branches(&[*pr]) {
                kept.push(KeptBranch {
                    branch: pr.head_ref.clone(),
                    reason,
                });
            }
        }
        Ok((created, kept))
    }
}

/// The top open PR, as what the stack's worktree is created for.
fn stack_target(input: &StackInput) -> Result<CheckoutTarget, String> {
    let open = input.open_prs();
    let Some(top) = open.last() else {
        return Err("This stack has no open pull requests".to_string());
    };
    Ok(CheckoutTarget {
        number: top.number,
        head_ref: top.head_ref.clone(),
        head_sha: top.head_sha.clone(),
        mine: false,
        stack: Some(input.name.clone()),
        // A stack's PRs are in its repository.
        head_repo: None,
        stack_branches: open.iter().map(|pr| pr.head_ref.clone()).collect(),
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::super::stack_support::StackFixture;
    use crate::review::checkout_types::StepConfirm;
    use crate::review::requirements::DeskAction;
    use crate::storage;
    use crate::util::host::Host;

    fn upstream(stack: &StackFixture, branch: &str) -> String {
        stack.git(&["config", "--get", &format!("branch.{branch}.remote")])
    }

    /// A fixture whose stack exists only on GitHub, as for a teammate's stack.
    async fn remote_only() -> Option<StackFixture> {
        let stack = StackFixture::new().await?;
        stack.git(&[
            "branch",
            "-q",
            "-D",
            "feat/driver",
            "feat/claude",
            "feat/maki",
        ]);
        Some(stack)
    }

    #[tokio::test]
    async fn checking_out_a_stack_makes_tracking_branches_and_one_worktree_on_top() {
        let Some(stack) = remote_only().await else {
            return;
        };

        let result = stack
            .desk()
            .ensure_stack_worktree(stack.input())
            .await
            .unwrap();

        assert!(result.created);
        assert_eq!(
            result.created_branches,
            vec!["feat/driver", "feat/claude", "feat/maki"]
        );
        assert!(result.kept.is_empty());
        assert_eq!(result.checkout.git_branch.as_deref(), Some("feat/maki"));
        assert!(result.checkout.on_pr_branch);
        assert!(!result.checkout.managed);
        for branch in ["feat/driver", "feat/claude", "feat/maki"] {
            assert_eq!(stack.tip(branch), stack.remote_tip(branch));
            assert_eq!(upstream(&stack, branch), "origin");
        }
        let worktrees = stack.git(&["worktree", "list", "--porcelain"]);
        assert_eq!(worktrees.matches("worktree ").count(), 2, "{worktrees}");
    }

    #[tokio::test]
    async fn checking_out_a_stack_again_changes_nothing() {
        let Some(stack) = remote_only().await else {
            return;
        };
        let first = stack
            .desk()
            .ensure_stack_worktree(stack.input())
            .await
            .unwrap();

        let second = stack
            .desk()
            .ensure_stack_worktree(stack.input())
            .await
            .unwrap();

        assert!(!second.created);
        assert!(second.created_branches.is_empty());
        assert_eq!(second.checkout.path, first.checkout.path);
    }

    #[tokio::test]
    async fn an_existing_worktree_is_kept_on_its_branch_and_missing_branches_are_filled_in() {
        let Some(stack) = StackFixture::new().await else {
            return;
        };
        let worktree = stack.worktree_on("feat/driver");
        stack.git(&["branch", "-q", "-D", "feat/maki"]);

        let result = stack
            .desk()
            .ensure_stack_worktree(stack.input())
            .await
            .unwrap();

        assert!(!result.created);
        assert_eq!(result.created_branches, vec!["feat/maki"]);
        assert_eq!(
            stack.git_in(&worktree, &["branch", "--show-current"]),
            "feat/driver"
        );
    }

    #[tokio::test]
    async fn a_diverged_branch_is_kept_and_reported_and_the_rest_go_ahead() {
        let Some(stack) = StackFixture::new().await else {
            return;
        };
        // A local commit on #415's branch, and a teammate's on GitHub.
        stack.git(&["switch", "-q", "feat/claude"]);
        stack.git(&["commit", "-q", "--allow-empty", "-m", "mine"]);
        stack.git(&["switch", "-q", "main"]);
        let local = stack.tip("feat/claude");
        stack.teammate_pushes(415, "feat/claude");

        let result = stack
            .desk()
            .ensure_stack_worktree(stack.input())
            .await
            .unwrap();

        assert_eq!(result.kept.len(), 1);
        assert_eq!(result.kept[0].branch, "feat/claude");
        assert_eq!(stack.tip("feat/claude"), local);
        assert!(result.created);
    }

    #[tokio::test]
    async fn a_branch_behind_github_fast_forwards() {
        let Some(stack) = StackFixture::new().await else {
            return;
        };
        let head = stack.teammate_pushes(412, "feat/driver");

        let result = stack
            .desk()
            .ensure_stack_worktree(stack.input())
            .await
            .unwrap();

        assert!(result.kept.is_empty());
        assert_eq!(stack.tip("feat/driver"), head);
    }

    #[tokio::test]
    async fn a_stack_with_no_open_prs_is_refused() {
        let Some(stack) = StackFixture::new().await else {
            return;
        };
        for number in [412, 415, 418] {
            stack.mark_merged(number);
        }

        assert!(stack
            .desk()
            .ensure_stack_worktree(stack.input())
            .await
            .is_err());
    }

    #[tokio::test]
    async fn a_restack_works_in_the_worktree_of_a_checked_out_stack() {
        let Some(stack) = remote_only().await else {
            return;
        };
        stack
            .desk()
            .ensure_stack_worktree(stack.input())
            .await
            .unwrap();
        let moved = stack.advance_main("main.rs", "the base moved\n");

        stack.desk().restack(stack.input(), None).await.unwrap();

        for branch in ["feat/driver", "feat/claude", "feat/maki"] {
            assert!(stack.has_ancestor(branch, &moved), "{branch}");
        }
    }

    #[tokio::test]
    async fn the_plan_asks_before_switching_the_stack_worktree_to_another_pr() {
        let Some(stack) = remote_only().await else {
            return;
        };
        stack
            .desk()
            .ensure_stack_worktree(stack.input())
            .await
            .unwrap();

        let plan = stack
            .desk()
            .plan_step(stack.target(412), DeskAction::WorkOnPr)
            .await
            .unwrap();

        assert_eq!(plan.step_up, None);
        assert_eq!(plan.confirm, Some(StepConfirm::SwitchBranch));
        assert!(plan.switch_branch);
        assert_eq!(plan.current.git_branch.as_deref(), Some("feat/maki"));
        assert!(!plan.current.on_pr_branch);
    }

    #[tokio::test]
    async fn checking_out_a_stack_is_refused_while_a_restack_is_stopped() {
        let Some(stack) = remote_only().await else {
            return;
        };
        stack
            .desk()
            .ensure_stack_worktree(stack.input())
            .await
            .unwrap();
        // main and #415 both change claude.rs, so the restack stops there.
        stack.advance_main("claude.rs", "main's version\n");
        stack.desk().restack(stack.input(), None).await.unwrap();

        let refused = stack.desk().ensure_stack_worktree(stack.input()).await;

        assert!(refused.unwrap_err().contains("restack"));
    }

    #[tokio::test]
    async fn a_read_never_asks_to_switch() {
        let Some(stack) = remote_only().await else {
            return;
        };
        stack
            .desk()
            .ensure_stack_worktree(stack.input())
            .await
            .unwrap();

        let plan = stack
            .desk()
            .plan_step(stack.target(412), DeskAction::Read)
            .await
            .unwrap();

        assert!(!plan.switch_branch);
        assert_eq!(plan.confirm, None);
    }

    #[tokio::test]
    async fn working_on_a_lower_pr_switches_the_stack_worktree_and_records_it() {
        let Some(stack) = remote_only().await else {
            return;
        };
        let path = stack
            .desk()
            .ensure_stack_worktree(stack.input())
            .await
            .unwrap()
            .checkout
            .path
            .unwrap();
        let (lower, top) = (stack.tip("feat/driver"), stack.tip("feat/maki"));

        let state = stack
            .desk()
            .ensure_pr_worktree(stack.target(412))
            .await
            .unwrap();

        assert_eq!(state.path.as_deref(), Some(path.as_str()));
        assert_eq!(state.git_branch.as_deref(), Some("feat/driver"));
        assert!(state.on_pr_branch);
        assert_eq!(
            stack.git_in(Path::new(&path), &["branch", "--show-current"]),
            "feat/driver"
        );
        assert_eq!(stack.tip("feat/driver"), lower);
        assert_eq!(stack.tip("feat/maki"), top);
        let row = storage::read_checkout(&stack.desk().database, &stack.desk().repo_key(), 412)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.path, path);
    }

    #[tokio::test]
    async fn a_dirty_stack_worktree_refuses_to_switch_and_stays_put() {
        let Some(stack) = remote_only().await else {
            return;
        };
        let path = stack
            .desk()
            .ensure_stack_worktree(stack.input())
            .await
            .unwrap()
            .checkout
            .path
            .unwrap();
        std::fs::write(Path::new(&path).join("scratch.txt"), "dirty\n").unwrap();

        let refused = stack.desk().ensure_pr_worktree(stack.target(412)).await;

        assert!(refused.is_err());
        assert_eq!(
            stack.git_in(Path::new(&path), &["branch", "--show-current"]),
            "feat/maki"
        );
    }

    #[tokio::test]
    async fn a_stack_worktree_mid_rebase_refuses_to_switch() {
        let Some(stack) = remote_only().await else {
            return;
        };
        let path = stack
            .desk()
            .ensure_stack_worktree(stack.input())
            .await
            .unwrap()
            .checkout
            .path
            .unwrap();
        let dir = Path::new(&path);
        // Stop a rebase at a conflict in maki.rs.
        stack.git(&["switch", "-q", "-c", "side", "feat/claude"]);
        stack.inner.repo.commit("maki.rs", "other\n", "other");
        stack.git(&["switch", "-q", "main"]);
        assert!(!crate::git::test_repo::git_ok(dir, &["rebase", "side"]));
        assert!(crate::git::stack::rebase_in_progress(&Host::Native, dir));

        let refused = stack.desk().ensure_pr_worktree(stack.target(412)).await;

        assert!(refused.unwrap_err().contains("rebase"));
    }
}
