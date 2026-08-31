# Message versions

Editing a user message keeps every version of it. Vocabulary: **Version**,
**Branch**, **Root thread** in `CONTEXT.md`.

## Behaviour

- The pencil on a user message opens the text inline. Sending the edit forks
  the thread strictly before that message's turn, sends the edited message on
  the fork, and opens the fork. Attachments and skills on the original message
  keep their place; only the text changes.
- Under an edited message the bubble shows `‹ 2 / 3 ›`. The arrows open the
  thread holding the neighbouring version, replies and all. Editing an edit
  adds to the same group (`3 / 3`), never a nested one.
- Branches are hidden from the sidebar and from search. The sidebar lists and
  highlights the root thread whichever branch is on show; opening the root
  lands on the most recently active thread in the family.
- Deleting a thread deletes its branches. Archiving leaves them be.
- No edit while a turn is running, on an optimistic turn Codex has not
  acknowledged yet, or on a Claude Code thread (the driver cannot fork).
- Nothing is rewound. `/undo` still truncates in place and keeps its
  confirmation.

## Mechanism

- `thread/fork` with `beforeTurnId` excludes that turn and everything after
  it, and preserves turn and item ids — present on every supported tier
  (since 0.150.0), so there is no `Feature` gate.
- `thread_branches` (`src-tauri/src/storage/branches.rs`) records each fork:
  parent, `group_turn_id` (the original message's turn id, stable across the
  family), `replaced_turn_id`, `inherited_turns`, and `edit_turn_id` once the
  fork's first turn has an id. `updated_at` is filled from the thread listing
  at bootstrap.
- `src/lib/thread/messageVersions.ts` computes a bubble's place in its group
  by its own turn id, the arrow targets, the root of a family and its newest
  leaf. `ThreadView.submitEdit` forks, records the branch, primes the fork's
  session and sends before navigating so the edit is already streaming when
  the view remounts.

## Later

- Expose fork as a `Capability` so a driver that can fork (Claude Code's
  `--fork-session`) gets versions without the frontend checking the harness.
- A version count on the root's sidebar row.
