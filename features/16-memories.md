# Memories

What each harness remembers between conversations: shown, edited, deleted
and switched on or off from one page. Vocabulary: **Memory store**, **Memory
entry** in `CONTEXT.md`.

## Where it shows

A page of its own in the main column, like pull-request review. It is not a
Settings section. Open it from **Memories** in the sidebar footer or with
`/memories` in the composer. It covers every Home in the window's Profile.

- **Left**: each Home with its switches, then its Memory stores, then each
  store's entries. Search matches titles, descriptions, paths and kinds.
- **Right**: the selected entry. Markdown renders; other files (a skill's
  script) show as text. Edit swaps in a text editor (⌘S saves). Unsaved edits
  stay attached to their entry while you look at others. Reveal in Finder and
  Delete (inline confirm) sit beside Edit.

## Where memories live

Neither harness offers a protocol for listing or editing memories, so the
backend (`src-tauri/src/memories/`) reads and writes the files on the Home's
Host.

| Harness | Stores | Index | Switches |
| --- | --- | --- | --- |
| Codex | `$CODEX_HOME/memories`, `memories_v2` | `MEMORY.md`, written by Codex's consolidation agent | `[features] memories` (off by default), `[memories] generate_memories`, `use_memories` in `config.toml` |
| Claude Code | `<config>/projects/<slug>/memory`, one per project | `MEMORY.md`, one link line per file | `autoMemoryEnabled` in `<config>/settings.json` |

Claude's slug is the project path with every non-alphanumeric character
replaced by `-`. A store whose slug matches a project the window knows is
titled with that project's name.

## Rules

- Paths are checked against the Home's store roots after resolving symlinks.
  The roots themselves, anything outside them, dot entries (Codex's `.git`
  baseline) and files that do not exist are refused. The page edits memories
  but never creates them.
- Writes are atomic (temp file, then rename).
- Deleting a Claude memory also removes the lines in its `MEMORY.md` that link
  to it. A Claude file that `MEMORY.md` does not link to is flagged "Not in
  MEMORY.md", because Claude never loads it.
- `config.toml` is edited with `toml_edit`, keeping comments. `settings.json`
  is edited as text, keeping key order and formatting, and the edit is
  re-parsed to prove it hit the top-level key.
- Codex's `memory/reset` (wipes every memory) is not offered.

## Not done

- **Per-thread memory mode.** Codex has `thread/memoryMode/set` but nothing to
  read a thread's mode back, so a control could not show its current state.
