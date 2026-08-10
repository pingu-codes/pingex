# Git worktrees and desktop handoff

Priority: P1

## What exists today

The app discovers permanent Codex worktrees beneath `CODEX_HOME/worktrees`, adds their main repository when Git can identify it, and renders the worktree as a special project that can be revealed or hidden. It does not manage Git worktrees, expose their branch or status, or provide a reliable way to continue a CLI thread in this desktop app.

This is materially short of a desktop workflow: a user can see that a worktree exists, but cannot tell whether it is safe to use, which branch it owns, whether it has uncommitted work, or move a thread to the desktop while retaining the state root that owns that thread.

## Worktree management

Provide a repository-level **Worktrees** view and lightweight worktree information on every project card:

- List the main checkout and every linked worktree from `git worktree list --porcelain`, including path, branch or detached revision, HEAD, lock/prune state, and whether it is a Codex-managed permanent worktree.
- Show ahead/behind counts, concise working-tree status, and the active thread count for each worktree. Refresh explicitly and after a completed agent turn; do not poll expensive repository commands continuously.
- Create a worktree from an existing local branch, a new branch, or a selected base revision. The default location for agent-created worktrees is the current Codex-home worktree convention, while manually managed worktrees retain the user-selected location.
- Open a worktree in the app, Finder, or configured IDE; create a new thread with that exact directory as its `cwd`.
- Permit rename, lock, prune, and remove only after showing the affected branch, dirty state, and linked threads. Removal must never delete an uncommitted worktree without an explicit typed confirmation; offer `git worktree remove` and `git worktree prune` separately.
- Detect stale registrations, missing directories, branch already checked out elsewhere, detached HEAD, and non-Git folders with an actionable explanation rather than silently dropping them from the sidebar.

Worktree identity must use canonical path plus Git common directory, not the display name or the assumption that all worktrees live below `CODEX_HOME/worktrees`. Keep the existing importer as a source of discovered projects, but do not classify arbitrary linked worktrees as disposable Codex worktrees.

## CLI and desktop handoff

Treat handoff as opening the same thread in the same state root, not merely opening a matching folder. A handoff payload needs the thread ID, working directory, resolved `CODEX_HOME`, and an optional source label. The receiving app must first select that home, then load the thread and validate that the thread's `cwd` belongs to the requested worktree.

Support both directions:

- **CLI to desktop:** accept a `codex://threads/<id>?path=<cwd>&codexHome=<home>` deep link (and the equivalent new-thread link). If the supplied home differs from the running home, offer a deliberate switch or a new window; never fall back to the default home and show an empty thread.
- **Desktop to CLI:** expose `Continue in terminal` and copy a reproducible command or launch the configured CLI with the chosen `CODEX_HOME`, thread ID, and worktree `cwd`. Keep this non-destructive and report when the configured CLI does not support a resume argument.
- **Thread to worktree:** allow `Move to worktree` only for a new/forked continuation. Historical turns retain their original `cwd`; the UI must say that this is a fork/continuation rather than pretending to relocate past work.

Show the current home and worktree in the thread header and handoff confirmation. When either is unavailable, keep the thread readable but disable mutation and resume actions with a clear recovery path.

## Other useful Git surfaces

Build these on one native Git service shared by the worktree view, thread header, and future PR review:

- **Status and changes:** staged, unstaged, untracked, conflicted, ignored, and agent-touched files; render file and hunk diffs with a refresh timestamp.
- **Branch context:** current branch, upstream, ahead/behind, recent commits, merge base, and a safe branch switch/create flow that refuses a dirty or occupied worktree by default.
- **Commit and sync:** stage selected hunks, commit with a visible author and hook result, fetch/pull/push, and show authentication or non-fast-forward failures. Make network and history-changing commands user-initiated, never an agent side effect.
- **Recovery:** stash/create patch, apply a patch into a new worktree, conflict guidance, and a clear link from a failed agent change to the affected files and Git status.
- **Review bridge:** choose a branch or worktree as the base for the pull-request review feature, including local-only diffs before a PR exists.

Do not make the frontend shell out to Git. Rust owns command invocation, uses an explicit repository/worktree directory, records a bounded structured result rather than parsing presentation text in Svelte, and serializes mutating operations per common Git directory. Use argument arrays, a short timeout for read-only inspection, cancellation where possible, and redacted errors. The first implementation can use the installed Git executable; later replacement with a library is an internal detail, provided porcelain compatibility and linked-worktree behaviour remain covered.

## What it should look like

Give each project header a compact branch/status chip. Selecting it opens a Worktrees page: the main checkout is visually distinct, linked worktrees appear as cards, and each card shows branch, status summary, last agent activity, and `Open`, `New thread`, and overflow actions. Put potentially destructive controls in a separate management menu.

The thread header shows `home › repository › branch/worktree` with a `Handoff` menu. A handoff dialog states exactly which Codex home, thread, and directory will be used, and offers to open another app window when doing so avoids changing the currently active home.

## Acceptance checks

- Linked worktrees outside `CODEX_HOME/worktrees` appear correctly and are never labelled as Codex-managed solely because of their path.
- A dirty worktree, detached HEAD, missing worktree directory, and a branch checked out in another worktree each produce a distinct, useful state.
- Creating a worktree produces a thread whose `cwd` is that worktree, and removing it is blocked while dirty unless the user completes the explicit confirmation.
- A custom-home CLI-to-desktop handoff opens the intended thread without relying on the default `~/.codex`; an unknown home produces an actionable error rather than an empty view.
- Desktop-to-CLI handoff preserves both `CODEX_HOME` and `cwd`, and a thread continuation onto another worktree is visibly a fork.
- Unit tests cover porcelain parsing and path identity; integration tests use a temporary Git repository with linked worktrees; UI tests cover the warning/confirmation paths.
