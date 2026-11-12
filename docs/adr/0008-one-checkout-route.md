---
status: accepted
---

# One route onto disk: Check out a PR or Stack into its worktree

Supersedes ADR 0005 point 4, and ADR 0007 points 1 to 3, 7, the switching
and read-only parts of 9, and 10.

A trace of every way a PR or Stack reached disk or a thread found six routes
with different rules. The backend didn't enforce the worktree policy and
trusted the client for whether a PR was mine, which stack it was in and its
head. A stack's worktree switched branch three ways. Threads started three
ways. `pendingLinks` could file a thread under the wrong PR. Reconciling the
routes would have kept all six, so all but one go.

1. **A PR is either not checked out or checked out in its worktree.** A lone
   PR's worktree is its own; a PR in a stack is always in the Stack worktree.
   No other place counts: a branch checked out by hand is not the PR's until
   it is adopted (point 4). `review_checkouts` holds a row per checked-out PR,
   and a stack's open PRs each have one naming the stack and its worktree.
   Reads (`review_pr_checkout`, `review_pr_checkouts`) write nothing; repair
   happens on the way to a change.

2. **Only an explicit Check out makes one.** `review_check_out(number)` and
   `review_check_out_stack(stack)` are the only commands that put a PR on
   disk, and a stack's PR checks out the whole stack. There is no policy
   setting, no step plan and no per-action confirmation. Actions that need the
   worktree (Work on this PR, Fix CI, Run locally, Open in editor, Push,
   Restack, Push stack, agent reviews, Ask and Hand to agent, Review whole
   stack) are disabled with "Check out first" until it exists. In the
   frontend, `review/prOnDisk.ts` is the only caller of the checkout, adopt
   and switch commands, which `bun run check` holds (`scripts/check-seams.ts`).

3. **The desk works out the PR.** Commands take a number. `target_for` reads
   the head and author from the last inbox sync (GitHub's PR detail when the
   sync hasn't seen it), whether it is mine from the repository's account, and
   the stack from GitHub's stack listing, read when the PR is checked out.
   `CheckoutTarget` stays as the type agent review requests carry, and is no
   longer sent to check anything out.

4. **A branch in use is offered, never taken.** When the git branch the
   worktree would be on is checked out somewhere, Check out answers
   `BranchInUse` with the path, and the frontend asks whether to adopt it
   (`AdoptWorktreeDialog`). `review_adopt_worktree` records the row as
   `adopted`. The main clone may be adopted. Pingex never switches,
   fast-forwards or removes an adopted worktree; cleanup only forgets it.

5. **Every worktree Pingex makes is the same kind.** It goes under the
   repository's worktree directory, else `<home>/worktrees/<repository>/`,
   named after its branch, and carries the `pingex pr worktree` lock. The
   branch still depends on whose PR it is, because that is what keeps fork
   branch names from colliding: my same-repository PR, and a stack's, on its
   real branch with an upstream; anyone else's, and any fork, on `pr/<n>` with
   none. The worktree listing marks only the `pr/<n>` ones as someone else's,
   so the sidebar's nesting is unchanged.

6. **Cleanup treats them all alike.** Seven days after a PR closes, or after
   every PR of a stack has, the worktree Pingex made goes if it holds no work,
   with the proof ADR 0007 point 6 describes. Only `pr/<n>` branches are
   deleted with it. If tracking or the row write fails right after
   `git worktree add`, the new worktree is removed again.

7. **Every PR thread runs in the worktree.** `startPrThread` in
   `prOnDisk.ts` is the only way one starts: in the worktree, writable,
   named from its opening words, with the worktree's context, and filed under
   the PR before its first turn. A stack's worktree on another PR's branch
   switches first, waiting for threads running there. The No checkout step and
   its read-only threads are gone; "Ask only" remains a per-turn read-only
   choice. A thread started in a PR or Stack worktree any other way (a draft,
   "New thread here", a plan handoff) is filed from the worktree's context as
   it starts. A link that can't be written deletes the turnless thread.
   `review_link_thread` records the start tips again when linking
   fast-forwarded the worktree.

8. **A stack's worktree switches PR one way**, `review_switch_stack_branch`.
   It refuses changes, a rebase going on, an adopted worktree and the main
   clone, and it fetches a missing PR head before taking the repository's
   lock, which the fetch takes too.

## Considered options

- **Reconcile the six routes behind one plan.** Keeps every entry point and
  every place a rule can drift.
- **Adopt a branch in use silently**, as `checkout_state` did. That made a
  colleague's hand checkout, or the main clone, a PR's worktree without anyone
  saying so, and Pingex then moved it.
- **Keep No checkout for diff reviews.** A second kind of PR thread, with its
  own sandbox rule, is what let threads start three ways.
- **Cache the stack listing in the backend.** Check out is explicit and rare;
  a cache of mutable GitHub state would need its own invalidation.

## Consequences

- An agent review, and Ask agent, need the PR checked out. A re-run on new
  commits waits for a PR that isn't.
- The migration marks every stored worktree that wasn't a `pr/<n>` worktree
  as adopted. Those, including stack and own-PR worktrees Pingex made before,
  are never removed by cleanup.
- `review_checkouts.step` and `review_pr_links.step` stay for old rows; new
  rows write `worktree`. The repository settings' policy columns stay unread.
- Checking out a PR reads GitHub's stack listing once.
