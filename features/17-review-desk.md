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
| Pull requests | The inbox, grouped by repository and by stack. Filters are Needs me, Mine, Review requested, Everyone, Drafts, CI failing and Unresolved. It also shows the "Since you last looked" feed and the agent reviews that are queued or running. |
| Stack | The stack tree from the base up. Each branch shows its PR, state, commit count and threads. The stack's worktree, the selected PR's conversation and reviewers, and "Threads on this PR" sit alongside. |
| Review | Collapsible files, viewed state, inline human comments and agent findings (drafts only you see), the pending review, and the next PRs up. |
| Start review | Code (no checkout, or a PR worktree), the agent harness and focus, drafts on or off, and re-run on new commits. |
| Context menus | Menus for a PR row, a stack branch, a sidebar worktree, a thread, a review comment, an agent finding, a changed file, a branch chip and a status badge. |
| Implicit interactions | Row states, multi-select, line comments, drag-to-reorder a stack, restack progress and conflicts, cleanup after a merge, toasts, hover details and the keyboard. |
| Model | The three checkout steps, what each action needs, and where threads are grouped. |
| Adding repositories | From GitHub (PRs only, clone, or link an existing clone), from a local folder (which remote PRs come from, plus per-repository defaults), and the sidebar states. |

## Checkout steps

A PR moves up one step at a time, and only when an action needs it. Each
button names the step it would create.

| Step | On disk | Adds | Implemented by |
| --- | --- | --- | --- |
| No checkout | Hidden refs only | Read, comment, approve, merge, diff review, stack tree | `review_fetch_prs`, `review_diff`, `review_file`, `review_changes_since` |
| Worktree | A real worktree on a local branch (ADR 0007) | Build, run, test, measure, edit and commit; push, restack, agents fixing review comments | `review_ensure_pr_worktree`, `review_push_to_pr` |

## Where the code goes

| Piece | Where | Extend it by |
| --- | --- | --- |
| GitHub provider port | `src-tauri/src/review/provider.rs` (`GitHubProvider`) | Adding the operation to the trait, to `github::GhCli`, and to `fake::FakeGitHub` (with a `Sent` variant for a write). Only `review/github/` names `gh`. |
| Review desk service | `src-tauri/src/review/desk/` (`ReviewDesk`), one file per area | Adding an `impl ReviewDesk` block in the area's file, or a new file for a new area. Tests use `desk/test_support.rs`: a `TestRepo` whose `origin` reads as GitHub, plus `FakeGitHub`. |
| Commands | `src-tauri/src/review/commands.rs` | A thin wrapper that builds the desk with `desk(...)` and forwards. |
| What each action needs | `src-tauri/src/review/requirements.rs`, exported as `DESK_ACTION_STEPS`; labels and tooltips in `src/lib/review/actions.ts` | A `DeskAction` variant, then `bun run typegen` and its label. |
| Context menus | `src/lib/components/ActionMenu.svelte`, entries in `actionMenu.ts` | Passing entries (items, separators, submenus) with accelerator shortcuts. |
| Inbox classification | `src-tauri/src/review/desk/sync.rs` (Needs me, Mine, Review requested, commits since my review) and `desk/activity.rs` (feed and seen state, muting), stored in `review_seen` and `review_feed` | Reading `inbox.triage` and `inbox.counts`; the frontend never re-derives them. |
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
(`services/commandEvents.ts`), and the inbox syncs after the GitHub writes
in `GITHUB_WRITES` (`inbox/inboxStore.svelte.ts`).

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
- A review mutation marks every read of its repository.
- A git mutation, window focus or a finished turn marks local git reads.
- Writes that touch only the profile database (viewed marks, the pending
  review, Findings) mark nothing; `NARROW` lists them.

A stale read that a mounted page is showing is re-read at once. Any other
stale read is re-read the next time something asks for it. The review page
warms the next PR in Up next, and the Stack page warms the PRs on either
side of the selected one, so j/k and branch clicks open from the cache.
