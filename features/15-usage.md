# Usage

Where the tokens go: by thread, by project, and across the Home. Vocabulary:
**Usage row**, **Attribution**, **Context composition**, **Opening row** in
`CONTEXT.md`.

## What it answers

Two questions, both by category:

- **Context composition** — what the thread's context window holds right now.
- **Spend by category** — what every turn so far has cost, summed for one
  thread, for every thread under a project path, or for the whole Home.

The categories, in display order:

| Category | Holds | Figure |
| --- | --- | --- |
| System prompt | the harness's own prompt, tool definitions, instructions, memory files | ≈ estimated |
| Skills | skill files the user invoked with `$` | ≈ estimated |
| Your messages | text the user typed | ≈ estimated |
| Tool use | commands, diffs, tool calls and their output | ≈ estimated |
| Replies | the model's messages | reported |
| Reasoning | thinking tokens | reported |
| Not attributed | an opening row, or context the estimate does not cover | — |

Cached input, cache writes, output and reasoning totals are always the wire
figures. Cost is the harness's own figure where it reports one (Claude), and
the API list price from `usageCost.ts` for the rest, shown as `≈ $…`.

## Where it shows

- **Thread**: `/status` (the right panel). The overview card's Usage row
  opens it; the figures no longer sit inline. Two bars: context composition
  and spend by category, then the wire totals, the Codex credit estimate and
  the account limits as before.
- **Project**: the Usage tab of project details, with an All time / 30 days /
  7 days range, the heaviest threads (click to open) and a per-model table.
- **Home**: the Usage section of Settings, same layout at global scope, and a
  "last 30 days" card on the homepage that links to it.

## How it is recorded

Both harnesses report token usage as a running total per thread on
`thread/tokenUsage/updated`, and nothing kept it. The ledger
(`src-tauri/src/usage/ledger.rs`) now turns every report into a row of
`turn_usage` keyed `(thread_id, turn_id)`:

- Inside a turn, the row is `total − (total when the turn began)`, so a
  Codex turn of several requests ends as one row of the turn's full spend.
  It is rewritten on every report and once more on `turn/completed`, when
  the items that completed after the last report are in the composition.
- Outside a turn, whatever the total exceeds the stored rows by is booked
  to the thread's **opening row** (`turn_id = "_opening"`), unattributed.
  That is how a thread resumed for the first time after this shipped gets
  its history counted at all, and it is why an old thread's bar shows a
  large "Not attributed" share.
- A total that goes backwards means the harness process restarted its count
  (the Claude projector counts per process); the turn then starts from zero.
- Spend rows are never copied to a fork (the parent paid) and never deleted
  with a thread (the Home's bill does not shrink). A rollback keeps them and
  resets the composition, which is rebuilt from the turns that remain.

The ledger applies items, usage and turn completions through one consumer
per Home, so they are seen in stream order regardless of how the journal's
own writes interleave.

## How attribution is estimated

`src-tauri/src/usage/estimate.rs`, pure and unit-tested.

- Items are sized at about four characters per token: user text → Your
  messages; skill parts → Skills (the file's length, counted once per
  thread); commands with their output, diffs, tool calls with their
  arguments and results → Tool use. Replies and reasoning are not sized;
  their tokens are exact.
- The **system prompt** is derived once per thread as the part of the first
  measured context that nothing else explains, and carried forward. When the
  estimate outgrows a later measurement it is scaled down to fit.
- A turn's input tokens are split across the composition pro rata; output
  and reasoning are added exactly. The parts always sum to the wire total.
- After a turn its reply joins the composition. Compaction drops messages,
  tools and replies from it and keeps the prompt and skills.
- A thread the ledger has not seen since the app started is rebuilt from its
  stored rows and journaled items on first sight.

## Exact where the harness can say

Claude Code answers `get_context_usage` with a categorised view of the live
context. `read_context_breakdown` sends it for a thread with a live process
and maps the categories: prompt, tools, agents and memory files → System
prompt; Skills → Skills; the single "Messages" figure is split by the
ledger's estimated ratio; free space and the autocompact buffer are not
tokens in use. The composition is then badged "reported" rather than
"≈ estimated".

Codex has no such API, and a Claude thread between processes has nothing to
ask. Both fail with the `harness-unsupported:context_breakdown` prefix and
the panel shows the estimate. The frontend never checks the harness kind.

## Limits

- Estimates are estimates. Sizes are generous (markup, JSON punctuation), so
  the fit-to-measurement step matters; the system prompt inherits whatever
  the first measurement did not explain, including tool definitions.
- Prior reasoning is not re-sent by either harness and is not counted in
  context.
- Usage that streamed while the app was closed is lost for Claude threads
  (nothing replays it) and lands in the opening row for Codex threads.
- A Branch counts only its own turns; a whole-family total is not offered.
