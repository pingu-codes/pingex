# Changes page

What a thread has changed, laid out like a pull request's files, for agents
that commit, hop between the git branches of a stack, and work across a
multi-project workspace. It replaces the thread's right-panel Changes view and
its Outputs view. Vocabulary: **Changes page**, **Change segment** and
**Start tips** in `GLOSSARY.md`.

## Why the old panel failed

- It diffed the thread's directory against `HEAD` in the main worktree, so it
  went blank once the agent committed.
- In a linked worktree it compared with whatever the main worktree had
  checked out, not the PR's base.
- In a workspace it diffed the hub, a folder of links, so git failed and no
  member repository was shown.
- From a subdirectory, the root-relative and cwd-relative paths it read
  disagreed.
- It saw only the git branch checked out.
- Outputs listed only the edits a harness reported, so a shell edit, a
  checkout or a rebase never showed.

## Reading

`src-tauri/src/changes/` builds the page from three commands:
- `git_thread_changes(thread_id, cwd, scope)` lists the segments.
- `git_segment_diff(repo, base, head)` reads a committed segment's patches.
- `git_working_diff(repo, base)` reads the working tree's patches, untracked
  files included.

**Repositories.** A workspace thread reads every member's effective path; any
other thread reads its directory. Each path resolves to its worktree's top
level, and every read runs from there, so paths are root-relative. A folder
outside git is skipped, and the page shows the edits the harness reported
instead.

**Branch scope.** The segments are:
- A Stack worktree, recognised by its purpose while it holds one of the
  stack's branches: one segment per PR, read from refs whichever branch is
  checked out.
  - The bottom PR is compared with the merge base of the stack's base.
  - Each PR above is compared with the merge base of the PR below.
  - `needsRestack` marks a PR that is no longer on top of the PR below.
- Any other git branch: compared with its PR's base. The PR is found by
  number for a managed PR worktree or a PR link, else by head ref, both in the
  last inbox read. With no PR, it is compared with the remote's default
  branch. On the default branch itself, the segment is what is unpushed.
- The working tree against `HEAD`, placed after the checked-out branch's
  segment.

**This thread.** The Start tips are recorded before a thread's first turn in
each repository: at `start_thread` and at each `start_turn`, waiting at most
two seconds, never failing the turn. Against them, the segments are each git
branch that:
- moved: compared with its old tip, and marked `rewritten` when the old tip is
  no longer an ancestor;
- was made since: compared with where it left the thread's start;
- was deleted since.

Then the working tree, marking files that were already dirty. A collected old
tip becomes a note on the segment. A thread from before Start tips falls back
to its PR link's start commit. With neither, the page says the tips are
recorded from the next turn.

**Choosing the base.** Every committed segment carries a base for which the
straight `base..head` diff is the answer, so its listing and patches are keyed
by SHA and cached. Content keys (blob ids, or `hash-object` for the working
tree) reset a file's Viewed mark when it changes again.

## Page

- **Route.** The page is a top-level view, `view.changes` with `threadId`,
  opened from the overview's Changes rows, Open Changes and `/diff` (which
  opens on the working tree). Back returns to the thread.
- **Layout.** A segment rail grouped by repository, the review page's
  `FileRail`, and `FileDiff` for each file.
- **Keys.** It shares the review page's keys: j/k walk segments, [ and ] walk
  files, v marks a file viewed, c comments, ⌘↵ sends.
- **Notes.** Selected lines take a note or a suggested change. The notes
  become one message, grouped by repository and segment with `path:lines` and
  the quoted lines, sent through the thread's session. If the agent is
  working, the message queues and the agent gets it after the turn; it never
  steers, because not every harness can and the lines may still move.

## Staying current

The page holds the thread's session, so it sees items live:
- Each `fileChange` or `commandExecution` item that finishes marks the
  overview stale. Reads are coalesced: 750 ms after the last change, at most
  3 s after the first.
- While a turn runs and the window is visible, it also re-reads every 5 s,
  which catches subagents and background processes.
- A finished turn, window focus and Pingex's own git commands mark it stale,
  as for every local read.
- The working tree's patches are read again only when its listing's content
  keys change.

There is no filesystem watcher. A watcher is unreliable across a WSL Host and
would need gitignore filtering. Polling git through `host.command` works on
every Host, and costs a handful of processes per repository per read.

## Known gaps

- A repository the agent works in outside its directory and the workspace
  members (a sibling clone it `cd`s into) is not shown.
- Start tips rely on git keeping the old commits. A collected tip degrades to
  a note.
- Notes live in memory per thread and are lost on restart.
