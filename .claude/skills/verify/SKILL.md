---
name: verify
description: How to build, run, and drive the Pingu Codex app to verify changes end-to-end.
---

# Verifying Pingu Codex changes

Run from the repo root. Pick the cheapest rung that proves the change; climb
only when a rung passes or the change needs it. Each command's output lands in
the conversation, so prefer quiet runs and pipe through `tail` when in doubt.

## The ladder

| Rung | Frontend | Rust | Cost |
|---|---|---|---|
| 1. Types compile | `deno task check` | `cd src-tauri && cargo check` | 8s / 14s |
| 2. Tests near the edit | `deno task test:changed` (vitest `--changed`) or `deno task test -- path/to/x.test.ts` | `deno task rust:test:lib -- module::name` | 15s / 7s |
| 3. Everything, once, before finishing | `deno task preflight` (check + lint + all vitest) | `deno task rust:preflight` (fmt + clippy + lib tests) | 45s / 30s |
| 4. Browser e2e | `deno task test:e2e:quick` (one chromium project) — full matrix `deno task test:e2e` only for layout/webkit work | | 15s+ |
| 5. Live harness | `deno task test:e2e:codex` / `test:e2e:claude` — spends real quota, confirm with the user first | | minutes |

Do not run rung 3 after every edit. Fix with rung 1 and 2, then run rung 3 once.

`cargo test` without `--lib` also compiles the four `live_*` integration
crates; use `rust:test:lib` unless you changed those.

## Browser preview surface (covers most frontend work)
The frontend runs without Tauri: `api.ts` gates on `isTauri()` and serves preview
fixtures, including a fake streaming turn (send any message; include the word
"approve" to trigger a fake approval card).

1. `deno task frontend:dev` → http://localhost:1420 (run in background; Playwright
   reuses it if already up).
2. Prefer writing a Playwright spec under `tests/browser/` and running
   `deno task test:e2e:quick tests/browser/<spec>.spec.ts` (no `--`; deno forwards the path) over ad hoc
   scripts: it uses the pinned browser build and the shared fixtures.
3. If you do need a one-off script, use the Playwright bundled with the repo
   (`deno run -A npm:playwright@1.61.1 ...`) rather than a hard-coded Chromium
   path under `~/Library/Caches/ms-playwright`; those builds rotate.
4. Useful flows: click preview thread "Custom frontend skeleton" (attachments,
   diff truncation, "Worked for" reasoning collapse); type in the composer and
   press Enter (optimistic bubble → Working… shimmer → typing dots → streamed
   agent text → "Worked for Xs"); right-click a sidebar thread for the context
   menu (archive/delete); the New thread button top-left of the sidebar.
- Known noise: `/favicon.ico` 404 in console — pre-existing, ignore.

## Real Tauri surface
- `deno task dev` (sets `CODEX_HOME=$HOME/.codex-personal`, opens a window).
- The `codex` shell alias is guarded; the real binary is `/opt/homebrew/bin/codex`.
  Set `PINGEX_E2E_CODEX=/opt/homebrew/bin/codex` for the live suites.
- Protocol smoke test without the GUI (validates app-server wire assumptions):
  ```bash
  ( printf '%s\n' \
  '{"id":0,"method":"initialize","params":{"clientInfo":{"name":"pingu_codex","title":"Pingu Codex","version":"0.1.0"}}}' \
  '{"method":"initialized","params":{}}' \
  '{"id":1,"method":"thread/list","params":{"limit":2,"sortKey":"updated_at","sortDirection":"desc","archived":false}}'; sleep 8 ) \
  | CODEX_HOME="$HOME/.codex-personal" timeout 20 /opt/homebrew/bin/codex app-server --stdio 2>/dev/null | grep '"id":1'
  ```
  Keep stdin open (the `sleep`) or the server exits before responding.
- Sending a real turn spends the user's Codex quota — confirm before driving live turns.

## Worktrees
Agents working in `.claude/worktrees/*` should export
`CARGO_TARGET_DIR=<repo>/src-tauri/target` before any cargo command so they
reuse the warm build cache instead of compiling a fresh 1 GB target directory.
