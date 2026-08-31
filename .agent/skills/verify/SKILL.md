---
name: verify
description: How to build, run, and drive the Pingu Codex app to verify changes end-to-end.
---

# Verifying Pingu Codex changes

## Fast checks
- `cd app && deno task check` — svelte-check (types).
- `cd src-tauri && cargo check` (or `cargo test` for the storage tests).

## Browser preview surface (covers most frontend work)
The frontend runs without Tauri: `api.ts` gates on `isTauri()` and serves preview
fixtures, including a fake streaming turn (send any message; include the word
"approve" to trigger a fake approval card).

1. `cd app && deno task frontend:dev` → http://localhost:1420 (leave running in background).
2. Drive headless with Python Playwright (installed via pyenv). The bundled
   headless-shell build is missing; launch the full Chromium build explicitly:
   ```python
   exe = os.path.expanduser("~/Library/Caches/ms-playwright/chromium-1169/chrome-mac/Chromium.app/Contents/MacOS/Chromium")
   browser = p.chromium.launch(executable_path=exe, headless=True)
   ```
3. Useful flows: click preview thread "Custom frontend skeleton" (attachments,
   diff truncation, "Worked for" reasoning collapse); type in the composer and
   press Enter (optimistic bubble → Working… shimmer → typing dots → streamed
   agent text → "Worked for Xs"); right-click a sidebar thread for the context
   menu (archive/delete); the New thread button top-left of the sidebar.
- Known noise: `/favicon.ico` 404 in console — pre-existing, ignore.

## Real Tauri surface
- `cd app && deno task dev` (sets `CODEX_HOME=$HOME/.codex-personal`, opens a window).
- The `codex` shell alias is guarded; the real binary is `/opt/homebrew/bin/codex`.
  Set `PINGU_CODEX_CLI_PATH=/opt/homebrew/bin/codex` if `codex` is not resolvable.
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
