---
status: accepted
---

# Review checkouts are Pingex-owned detached worktrees, promoted in place

Reviewing a PR sometimes has to run code: tests, the app, an editor. Before
the review desk, that meant checking out the author's git branch by hand,
which left worktrees and installed dependencies nobody meant to keep, and
put a colleague's branch one `git push` away. A PR now moves up three
checkout steps (No checkout, Review checkout, Worktree), and only when an
action needs it (`review/requirements.rs`). This records how the middle step
is built.

1. **A review checkout is a `git worktree add --detach` at the PR head,
   locked with the reason `pingex review checkout`**
   (`src-tauri/src/git/checkouts.rs`). It lives under the Home's
   `review-checkouts/<repository>/`, as `pr-<n>`, or as `stack-<name>` for a
   stack whose PRs share it and switch heads. Detached HEAD means no local
   branch is created and nothing can be pushed by accident. The lock keeps
   `git worktree prune` off it, and the lock reason is how the worktree
   listing flags it (`WorktreeEntry::is_review_checkout`), so the Worktrees
   list and the sidebar nesting leave it out without a second store to keep
   in step with git. Codex-home discovery scans `worktrees/` and
   `worktrees-tmp/` only, so it never becomes a project either.

2. **Pingex records the step per (repository, PR)** in `review_checkouts`,
   with the head it was made at and whether the repository's setup command
   has run there. Setup runs once per checkout directory. A checkout moves
   to a new PR head only while `git status` is clean; a dirty one stays
   where it is and says it is behind. Two facts on disk win over the record:
   a recorded directory that is gone means No checkout, and my own PR whose
   git branch is checked out in a worktree starts at the Worktree step.

3. **Promotion keeps the path.** When an action needs a worktree and the PR
   has a review checkout, the user picks: promote it, or create a new
   worktree. Promoting runs `git switch <branch>` inside the checkout
   (creating the branch at the PR head when it does not exist locally) and
   then unlocks it, so it appears in the Worktrees list. Threads already
   running there keep their cwd and history, which a fresh worktree
   elsewhere could not offer. The unlock comes last, so a failed switch
   leaves a review checkout that still reads as one.

4. **Policy, not prompts, gates creation.** Each repository has a review
   checkout policy (Ask first, Create when needed, Never) and "Don't ask
   again". The desk's `plan_step` turns an action, the PR's current step and
   the policy into the step to go up to and what to confirm, and the
   frontend only follows it. Pushing to someone else's git branch is
   confirmed every time, whatever the policy.

5. **Cleanup follows the PR.** After a sync, the desk compares what it keeps
   with the open PRs: a closed PR's refs go at once, and its review checkout
   7 days later. Worktrees are the user's and are never removed by this.

## Considered options

- **A temp worktree on a local branch (`codex/tmp-*`).** Reuses existing
  code, but creates branches in the user's repository and shows in the
  Worktrees list, which is what we are trying to avoid.
- **A separate clone per review.** Fully isolated, but costs a full fetch and
  disk per PR, and promotion would mean moving work between repositories.
- **Hiding review checkouts by path.** Would need the path classifier to
  know about promotion; the lock reason flips with promotion for free.

## Consequences

- Agents in a review checkout or at No checkout run with the read-only
  sandbox (the `ReadOnlySandbox` capability); a harness without one cannot
  be used there.
- A promoted checkout stays under `review-checkouts/`. It is listed as a
  worktree of its repository but is not a Codex-home project.
- `git worktree add --lock --reason` needs git 2.33 or newer.
