# Review desk

Pull requests, stacks and the checkouts they need, in one place. Designed in
the "Pingex review desk" canvas
(https://claude.ai/artifact/67heHF9b6TSYXzwoz6DcRC). Vocabulary (**Review
desk**, **Inbox**, **Seen head**, **Stack**, **Review checkout**) is in
`GLOSSARY.md`. How git and GitHub are read and written is decided in
`docs/adr/0004-git-and-github-io.md`.

## Status

- **Read layer (done):** the inbox, stacks, PR refs, diffs and files by SHA,
  "changes since", and the Seen head.
- **Writes (next):** review checkouts, promote to worktree, restack, push
  stack.
- **Desk UI (after that):** not started.

## Screens (canvas boards)

| Board | What it shows |
| --- | --- |
| Pull requests | The inbox, grouped by repository and by stack. Filters are Needs me, Mine, Review requested, Everyone, Drafts, CI failing and Unresolved. It also shows the "Since you last looked" feed and the agent reviews that are queued or running. |
| Stack | The stack tree from the base up. Each branch shows its PR, state, commit count and threads. The stack's worktree, the selected PR's conversation and reviewers, and "Threads on this PR" sit alongside. |
| Review | Collapsible files, viewed state, inline human comments and agent findings (drafts only you see), the pending review, and the next PRs up. |
| Start review | Code (no checkout, review checkout, or worktree on the branch), the agent harness and focus, drafts on or off, and re-run on new commits. |
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
| Review checkout | A detached checkout Pingex owns | Run tests, the app or a build; agent reviews that run code | next slice |
| Worktree | A named worktree in the sidebar | Edit, commit, push, restack, agents fixing review comments | existing worktree service, plus next slice |

## Read commands (`src-tauri/src/review/commands.rs`)

| Command | Reads |
| --- | --- |
| `review_inbox(repoDir)` | Every open PR, plus the events since the previous sync. The previous sync is stored in `review_inbox_snapshots`. |
| `review_stacks(repoDir)` | Native stacks, bottom first, or `supported: false`. |
| `review_fetch_prs(repoDir, numbers)` | Fetches PR heads into `refs/pingex/pr/<n>/head` from the remote that points at the repository. |
| `review_diff(repoDir, baseSha, headSha)` | The PR diff, by SHA. |
| `review_file(repoDir, sha, path)` | One file at one commit. |
| `review_changes_since(repoDir, number, baseSha, headSha)` | What changed since the Seen head, with a range diff after a rewrite. |
| `review_mark_seen(repoDir, number, sha)` / `review_forget_pr` | Pins or drops the Seen head, or drops all of a closed PR's refs. |

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
