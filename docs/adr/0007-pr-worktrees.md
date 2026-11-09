---
status: accepted
---

# A PR is either not on disk or in a real worktree

Supersedes ADR 0005.

Review checkouts were detached, read-only checkouts. That suited reading a
colleague's PR, but agents often need to do real work on a PR, especially your
own: build it, run it, measure it, try a fix. The concept tied together three
separate questions, and answered each one badly:

- **Where is the code?** There were three steps, and the middle one was only
  good for reading.
- **What may the agent do?** The step decided, and `mine` was never consulted,
  so an agent on your own PR was sandboxed. The promise didn't hold either:
  composer follow-ups and Run locally threads skipped it.
- **Where do commits go?** Nowhere. A detached checkout had no way to take a
  commit, and promotion, the branch chip and cleanup could all lose one.

So the steps are now No checkout and Worktree, and the three questions have
separate answers.

1. **Where the code is: a real worktree on a local branch** at the PR's head.
   No checkout stays for diff reviews and Ask agent, which need nothing on disk.

2. **What the agent may do: the thread's permissions.** Read-only applies only
   at No checkout, because the agent then runs in the user's own clone. A run
   nobody chose permissions for, such as a queued review, gets `workspace-write`
   with on-request approval, and never inherits the composer's Full Access.

3. **Where commits go: decided by who owns the PR.**

   | PR | Local branch | `git push` | Location and cleanup |
   | --- | --- | --- | --- |
   | Mine, same repository | The PR's branch, upstream set | Updates the PR | The repository's worktree directory; the user's, never removed |
   | A stack's, whoever wrote it | The PR's branch | As the stack's push does | As above |
   | Anyone else's, or any fork | `pr/<n>`, no upstream | Has nowhere to go | `<home>/pr-worktrees/<repository>/pr-<n>`; Pingex-managed |

   Stacks stay on real branches because restack and stack push work by branch
   name. `pr/<n>` also ends two old problems: a fork's `main` matching the
   user's local `main`, and a stale local branch being used unchecked.

4. **Safety moved to the push.** `ReviewDesk::push_to_pr` runs a non-force
   `git push <head remote> HEAD:refs/heads/<head ref>`, reading from GitHub at
   that moment whether it may. The app's buttons confirm every push to someone
   else's branch, and `git push` from the Git tab refuses to publish a managed
   worktree's `pr/<n>` to `origin`.

5. **Nothing discards work.** A worktree moves only by fast-forward: on a
   branch, clean, in a linked worktree, and contained by the new head. Otherwise
   it stays and `PrCheckout` reports `ahead`, `behind` and `dirty`. The main
   clone is never moved. The branch chip refuses to strand commits made on no
   branch. A branch with commits the PR lacks is used as it is, never reset.

6. **Cleanup proves before it removes.** Seven days after a PR closes, a managed
   worktree goes only if it has no uncommitted or untracked files and no commits
   outside its last-seen PR head and the remotes. Anything unproven, including a
   last-seen head git can't find, keeps it. Removal doesn't use `--force`, so git
   refuses a tree that changed since the check. Gitignored build output doesn't
   count as work. The PR's refs are pruned at close, so `review_checkouts.head_sha`
   keeps the last-seen head.

7. **The lock is still the marker.** A managed worktree is locked with the reason
   `pingex pr worktree` (`WorktreeEntry::is_managed_pr_worktree`), which needs no
   second store and keeps `worktree prune` off it. Managed worktrees now appear
   in the Worktrees list with their PR, because they can hold real work and must
   be findable, but stay out of the sidebar nesting, where their threads already
   file under the PR.

8. **Existing review checkouts are adopted in place** the first time they are
   read or pruned (`ReviewDesk::adopt_legacy`). A detached one gets `pr/<n>` at
   its current HEAD, keeping changes and commits, and is relocked. Paths don't
   change, so threads keep their cwd. Stored `review_checkout` and
   `reviewCheckout` values read as Worktree.

9. **A stack shares one worktree.** A stack's worktree sits on one of its git
   branches, and every open PR has a local branch tracking GitHub's. Checking out
   a stack (`ReviewDesk::ensure_stack_worktree`) makes those branches and, if the
   stack has no worktree, one on the top PR's branch. One per PR isn't an option:
   a restack rebases each branch inside one directory, and git refuses a branch
   checked out in another worktree. It also moves no work: a branch that has
   diverged from GitHub, or is checked out and behind it, stays put and is
   reported.

   A stack's PR is therefore often in a worktree that isn't on its own branch,
   which `PrCheckout.on_pr_branch` reports. An action that needs the PR's code
   switches the worktree first, after asking (`StepConfirm::SwitchBranch`), and
   refuses when it has uncommitted changes or a rebase going on. A thread started
   without that step, such as Ask agent, gets a read-only sandbox there. Fast-forwarding
   and Push to PR refuse a worktree on another branch: either would otherwise
   move a PR's branch onto its neighbour's commits.

10. **A same-repository PR's branch, checked out anywhere, is that PR's worktree,
    whoever wrote the PR.** The location rule is one function,
    `pr_checkouts::locate`, which the inbox, PR page and Stack page all read
    through. A branch you checked out by hand on a colleague's PR used to read as
    No checkout, so the indicator would have been wrong. A fork's branch name still
    says nothing about a local branch of that name.

## Considered options

- **Keep the step and just lift read-only on my own PRs.** Leaves detached HEAD,
  so commits still have nowhere to go, and leaves colleagues' PRs unworkable.
- **Use the real branch name for everyone's PR.** More familiar, but puts a
  colleague's branch one `git push` away and brings back the fork collisions.
- **Queued reviews inherit the composer's mode.** A batch from the inbox could
  then run under Full Access without anyone choosing that.
- **One worktree per stack PR.** Lets several PRs be open side by side, but
  restack, reorder and every other stack edit need the branches free.

## Consequences

- **A commit under Codex's `workspace-write` is expected to need approval.**
  Codex resolves a linked worktree's `gitdir` pointer and protects its target
  (`codex-rs/protocol/src/permissions.rs`), so writing files is free but git
  metadata isn't. A queued run that commits waits on an approval. The live test
  `a_pr_worktree_agent_writes_freely_commits_with_approval_and_cannot_push_unasked`
  pins this; run `bun run test:e2e:codex` to confirm it against a real agent.
- **Agent shell pushes are gated by the harness, not by Pingex.** Codex's
  `workspace-write` has no network by default and Claude's `acceptEdits` prompts
  on Bash. Under Full Access only the missing upstream and the prompt stand in
  the way of `git push <url> HEAD:<branch>`, which is what Full Access means.
- Claude's `acceptEdits` prompting on Bash means a queued "Run tests" review can
  wait on an approval.
- A repository with submodules can't have a managed worktree removed without
  `--force`, so it stays in the Worktrees list for the user to remove.
- `pr-worktrees/` isn't scanned by Codex-home discovery, so these never become
  projects. Adopted worktrees stay under `review-checkouts/`.
- `ReviewCheckoutPolicy` and the `review_checkouts` table keep their names to
  avoid a migration. The setting is shown as "PR worktrees".
- `git worktree add --lock --reason` needs git 2.33 or newer.
- A stack PR already in a worktree of its own stays there and is shown as such,
  but a restack can't rebase its branch until that worktree moves off it.
- Switching a stack's worktree changes the files under any thread running there.
  The confirmation says so; nothing stops it.
