# Demo screenshots

A scripted tour of Pingex that saves a screenshot of every major feature —
for READMEs, release notes, or showing the app off without a live Codex session.

```sh
deno task demo          # capture, then build the contact sheet
open demo/screenshots/index.html
```

Individual steps: `deno task demo:capture` (Playwright) and `deno task demo:index`
(contact sheet). The capture starts the browser preview itself via
`playwright.demo.config.ts`, or reuses one already on port 1420.

## What it produces

`demo/screenshots/light/` and `demo/screenshots/dark/` — the same 32 shots in both
colour schemes, 1440×900 at 2× (so 2880×1800 PNGs), plus `index.html` showing the
pairs side by side. The directory is cleared at the start of every run and is not
committed.

| Shots | Feature |
| --- | --- |
| 01 | Home — Codex home overview, projects, skills, MCP servers |
| 02–04 | Thread transcript: messages, attachments, reasoning, command output |
| 05–06 | Side panel: diffs (with truncation) and the project file tree |
| 07 | Thread overview: usage, plan, outputs, sources, subagents |
| 08–11 | Composer: `@` file mentions, `/` commands, model + effort, permissions |
| 12–14 | A live turn: streaming, completion, and an approval request |
| 15–17 | Sidebar: thread search, archived threads, thread context menu |
| 18 | Git worktrees |
| 19–20 | Pull-request review: PR list and the three-pane diff |
| 21–22 | Project details: instructions, sources, workspace search |
| 23–25 | Side questions and subagents |
| 26–31 | Settings: general, appearance, agent, integrations, connections, keyboard |
| 32 | The quick-chat window |

## How it works

Everything is driven against the **browser preview**, which serves the fixtures in
`src/lib/services/preview/` instead of talking to a real Codex process — so the
data is stable, no quota is spent, and shots are reproducible. To change what the
demo shows (project names, thread contents, PR diffs, integrations), edit those
fixtures; the e2e suite in `tests/browser` reads the same data, so keep its
expectations in mind when you do.

This is capture-only tooling: it is excluded from `deno task test:e2e`, and a
failing step here means a shot could not be posed, not that the app is broken.
