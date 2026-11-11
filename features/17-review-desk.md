# Review desk

Pull requests, stacks and the checkouts they need, in one place. Designed in
the "Pingex review desk" canvas
(https://claude.ai/artifact/67heHF9b6TSYXzwoz6DcRC). Vocabulary (**Review
desk**, **Inbox**, **Seen head**, **Stack**, **PR worktree**) is in
`GLOSSARY.md`. How git and GitHub are read and written is decided in
`docs/adr/0004-git-and-github-io.md`.

## Status

- **Read layer (done):** the inbox, stacks, PR refs, diffs and files by SHA,
  "changes since", and the Seen head.
- **Writes (next):** restack, push
  stack.
- **Desk UI:** the Pull requests inbox (`src/lib/inbox/`) is in. The Stack
  page, the review page and agent reviews are next.

## Screens (canvas boards)

| Board | What it shows |
| --- | --- |
| Pull requests | The inbox, grouped by repository and by stack. Filters are Needs me, Mine, Review requested, Everyone, Drafts, CI failing and Unresolved, plus the Author and Assignee pickers (see "People filters" below). It also shows the "Since you last looked" feed and the agent reviews that are queued or running. |
| Stack | The stack tree from the base up. Each branch shows its PR, state, commit count and threads. The stack's worktree (with Check out stack when it has none), the selected PR's conversation and reviewers, and "Threads on this PR" sit alongside. |
| Review | Collapsible files, viewed state, inline human comments and agent findings (drafts only you see), the pending review, and the next PRs up. |
| Start review | Code (no checkout, or a PR worktree), the agent harness and focus, drafts on or off, and re-run on new commits. |
| Context menus | Menus for a PR row, a stack branch, a sidebar worktree, a thread, a review comment, an agent finding, a changed file, a branch chip and a status badge. |
| Implicit interactions | Row states, multi-select, line comments, drag-to-reorder a stack, restack progress and conflicts, cleanup after a merge, toasts, hover details and the keyboard. |
| Model | The three checkout steps, what each action needs, and where threads are grouped. |
| Adding repositories | From GitHub (PRs only, clone, or link an existing clone), from a local folder (which remote PRs come from, plus per-repository defaults), and the sidebar states. |

## People filters

The Author and Assignee pickers on the Pull requests inbox (`PersonFilter`,
logic in `inbox.ts`) each tick any number of people. A PR matches when it is by
any ticked author and assigned to any ticked assignee; Assignee also offers
Unassigned. Clicking the author's or an assignee's avatar on a row, or the
author's name, ticks or unticks them. Each picker lists me first, then by how
many PRs the other filters leave each person, counted without that picker's
own choice.

With someone ticked:

- The tab stays where it was, and the tab counts are the ticked people's PRs in
  each tab. With no one ticked they are `inbox.counts`.
- A stack with a matching PR shows whole, and the PRs the filters don't keep
  are dimmed but stay usable. This applies only to grouped stacks; with Group
  by stack off, or a stack of several authors, only the matches show.
- An empty tab offers its matches in Everyone, and to clear the people filters.
- Past the 200 PR sync cap, a note says older PRs can't match.

Assignees come from the inbox query's `assignees` connection. It includes the
Copilot coding agent, as `Copilot` (checked Nov 2026).

## Getting around

Back retraces the way a page was reached, from the inbox, a Stack page, another
PR, a thread or a project. Each step is a view on the trail in
`navigation.svelte.ts`. Going to another PR from Up next or a list leaves a step;
j/k through the queue replaces the PR being read, so Back leaves the queue at
once. Any other navigation (the sidebar, opening a thread) starts the trail
again. Where the page's Back has scrolled away with the topbar, a floating one
replaces it, so going back doesn't lose the place in the diff.

Each page keeps what it had open in `app/places.ts`, in memory, by repository
and PR or stack number, and opens as it was left however it is reached: the
inbox its filter, chips, text, people filters, selection and scroll; a PR its tab, comment
filter, folded files and scroll; a Stack page its selected PR, tree view and
scroll.

A PR open in a stack always shows on the Stack page, selected, so the stack is
beside it however it was opened. `openReview` decides from the last sync; with
none yet, the PR opens on its own page. The tree folds to a strip of the stack's
PRs, and the choice is stored. A restack unfolds it to show its progress.

## Checkout steps

A PR moves up one step at a time, and only when an action needs it. Each
button names the step it would create.

| Step | On disk | Adds | Implemented by |
| --- | --- | --- | --- |
| No checkout | Hidden refs only | Read, comment, approve, merge, diff review, stack tree | `review_fetch_prs`, `review_diff`, `review_file`, `review_changes_since` |
| Worktree | A real worktree on a local branch (ADR 0007) | Build, run, test, measure, edit and commit; push, restack, agents fixing review comments | `review_ensure_pr_worktree`, `review_ensure_stack_worktree`, `review_push_to_pr`; where each PR is: `review_pr_checkouts` |

## Worktree context

A thread started from a draft in a managed PR worktree or a Stack worktree tells the agent where it is. The backend builds the text (`review/worktree_context.rs`, command `review_worktree_context`): for a stack, its open PRs bottom first, the branch checked out now, and the rule to put each change on the PR it belongs to and restack the PRs above; for a managed PR worktree, whose PR it holds and not to push. It reads the stored Worktree purpose and the directory's branch, never GitHub, and gives nothing when the worktree has since been put on a branch outside the stack.

The composer shows a pill for it on a draft, on by default (`ComposerPrefs.worktreeContext`), and only when the thread starts in that worktree: choosing a temporary or permanent worktree withdraws it. ThreadView appends the text to the first message inside `<worktree_context>`, after the user's words, so the title and the naming seed are the user's alone. The bubble folds it under a "Worktree context" chip (`thread/worktreeContext.ts` owns the tag). Agent reviews, conflict threads and plan threads send their own prompts and get none.

## Where the code goes

| Piece | Where | Extend it by |
| --- | --- | --- |
| GitHub provider port | `src-tauri/src/review/provider.rs` (`GitHubProvider`) | Adding the operation to the trait, to `github::GhCli`, and to `fake::FakeGitHub` (with a `Sent` variant for a write). Only `review/github/` names `gh`. |
| Review desk service | `src-tauri/src/review/desk/` (`ReviewDesk`), one file per area | Adding an `impl ReviewDesk` block in the area's file, or a new file for a new area. Tests use `desk/test_support.rs`: a `TestRepo` whose `origin` reads as GitHub, plus `FakeGitHub`. |
| Commands | `src-tauri/src/review/commands.rs` | A thin wrapper that builds the desk with `desk(...)` and forwards. |
| What each action needs | `src-tauri/src/review/requirements.rs`, exported as `DESK_ACTION_STEPS`; labels and tooltips in `src/lib/review/actions.ts` | A `DeskAction` variant, then `bun run typegen` and its label. |
| Context menus | `src/lib/components/ActionMenu.svelte`, entries in `actionMenu.ts` | Passing entries (items, separators, submenus) with accelerator shortcuts. |
| Inbox classification | `src-tauri/src/review/desk/sync.rs` (Needs me, Mine, Review requested, commits since my review) and `desk/activity.rs` (feed and seen state, muting), stored in `review_seen` and `review_feed` | Reading `inbox.triage` and `inbox.counts`; the frontend never re-derives them. |
| Worktree indicators | `WorktreeBadge` on inbox rows, stack cards, the PR page and the stack tree, its words in `worktreeBadge.ts`; `desk/pr_checkouts.rs` holds the one rule for where a PR is | A new place: read `reviewPrCheckouts` (or `reviewCheckoutState` for one PR) and pass `known`, so nothing claims there's no worktree before the read answers. |
| Pull requests inbox | `src/lib/inbox/`: `PullRequestsView`, pure helpers in `inbox.ts`, the sync cadence in `inboxStore.svelte.ts`, menus in `menus.ts` | Passing an `InboxHandlers` entry from `App.svelte` (agent reviews, worktrees, Stack page, Fix CI); without one its buttons show disabled. |
| Preview data | `src/lib/services/preview/desk.ts` | Extending its repositories and PRs rather than adding parallel fixtures. |
| Stack page | `src/lib/stack/`: `StackPage`, pure helpers in `stack.ts` (the `StackInput` the backend takes, reorder, the git branch menu, the merge follow-up), the worktree menu in `worktreeActions.ts`; backend `desk/stack.rs` (overview, PRs behind their base, plan), `desk/restack.rs` (per-branch rebases, snapshot, continue, abort; a stopped run is stored in `review_restacks`) and `desk/stack_edits.rs` (push with leases, reorder, fold, remove, split, rename, add, merged clean-up); commands in `review/stack_commands.rs`; preview fakes in `preview/stack.ts` | A new stack edit: a desk operation in `stack_edits.rs` driving a `restack::Run`, a command, a fake in `preview/stack.ts`, and a git branch menu entry. Review whole stack calls `startStackReview`. |

## Read commands (`src-tauri/src/review/commands.rs`)

| Command | Reads |
| --- | --- |
| `review_inbox(repoDir)` | Every open PR, plus the events since the previous sync. The previous sync is stored in `review_inbox_snapshots`. |
| `review_stacks(repoDir)` | Native stacks, bottom first, or `supported: false`. |
| `review_fetch_prs(repoDir, numbers)` | Fetches PR heads into `refs/pingex/pr/<n>/head` from the remote that points at the repository. |
| `review_diff(repoDir, baseSha, headSha)` | The PR diff, by SHA. |
| `review_file(repoDir, sha, path)` | One file at one commit. |
| `review_changes_since(repoDir, number, baseSha, headSha)` | What changed since the Seen head, with a range diff after a rewrite. |
| `review_prs_behind(repoDir, prs)` | How far each PR is behind its base: the commits on GitHub's base tip (`baseRefOid`, from the last sync) that GitHub's head lacks, fetching either if missing. A PR in a stack is based on the PR below, so this also catches one that no longer sits on that PR's head. `None` when the commits can't be fetched. The inbox asks only for the viewer's PRs; the review page and the Stack page ask for the PRs they show. |
| `review_mark_seen(repoDir, number, sha)` / `review_forget_pr` | Pins or drops the Seen head, or drops all of a closed PR's refs. |
| `review_mark_activity_seen(repoDir, numbers)` / `review_mute_pr(repoDir, number, headSha)` | Clears PRs' "since you last looked" entries, or leaves a PR out of Needs me until new commits. |

The writes that need no checkout are commands too: `review_merge`,
`review_close`, `review_retarget`, `review_mark_ready`, `review_check_logs`,
`review_rerun_failed_checks`, `review_create_pr`, `review_edit_comment` and
`review_delete_comment`. `review_my_repositories` and
`review_repositories_requesting_review` answer for the Global account.

Everything else runs as the Repository account. `review/github/account.rs`
resolves it in this order: a token in the environment, which `gh` obeys
regardless; the login the user pinned for this clone, which fails rather than
falls back if that login has signed out; a signed-in login that owns one of the
clone's GitHub remotes, checked in the order `gh` picks a base repository; and
last the Global account. The account menu in the inbox header
(`review_repository_account`, `review_set_repository_account`) shows the result
and changes it, and warns when GitHub answers as someone else.

Repositories added as "PRs only" have no clone. They keep using the GitHub
file listing (`review_pr_detail`) until something has to run.

## Refresh

The inbox syncs at three moments:
- when the window gains focus
- on the Sync button
- after Pingex's own mutations

While focused, it also re-syncs on a slow timer: at most every two minutes,
and backing off when `rateLimit.remaining` runs low. Nothing polls git. Local
state is re-read where the worktree service already re-reads it: on focus
and after a turn completes.

"After Pingex's own mutations" is not left to each page: the `commands`
proxy announces every settled git and review command
(`services/commandEvents.ts`), and the inbox syncs after the commands that
`services/commandEffects.ts` declares an `inbox` change.

### Query cache

A PR's detail, a stack's overview and restack preview, "behind the base",
the PR list and provider status are kept in the frontend query cache
(`services/queryCache.svelte.ts`). Opening a page again shows the last
answer at once and re-reads it in the background. Each read has a policy in
`READS`:

| Read | Shown without re-reading | Dropped after |
|---|---|---|
| GitHub reads (`reviewPrDetail`, `reviewStackOverview`, …) | 30 s | 10 min |
| Local git reads (`gitStatus`, `gitChangesSummary`, …) | 2 s | 10 min |
| `reviewProviderStatus` | 5 min | 1 h |
| SHA-keyed reads (`reviewDiff`, `reviewFile`, `reviewChangesSince`) | 30 min | 30 min |

A read can also go stale before its time is up:
- What a command marks follows from what `EFFECTS`
  (`services/commandEffects.ts`) declares it changes: a GitHub write marks
  every read of its repository and local git reads; an account change marks
  GitHub reads and provider status.
- A git change, window focus or a finished turn marks local git reads.
- Writes that touch only the profile database (viewed marks, the pending
  review, Findings) mark nothing, unless they name the reads that report them.

A stale read that a mounted page is showing is re-read at once. Any other
stale read is re-read the next time something asks for it. The review page
warms the next PR in Up next, and the Stack page warms the PRs on either
side of the selected one, so j/k and branch clicks open from the cache.
