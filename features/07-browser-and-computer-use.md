# Browser and computer use

Priority: P1. Browser use: built for Codex. Computer use (the whole
desktop): not started.

## What it does

A Codex thread drives **the user's own Chrome**: their profile, sign-ins
and open tabs, never a browser Pingex launches. It can do anything a
remote debugging port allows, but it works through cheap high-level tools
first, and every tab it touches and every site it visits is visible and
approved.

## How

**Connection.** Since Chrome 136, `--remote-debugging-port` is ignored on
the default profile. Chrome 144+ lets the user switch on remote debugging for
the running browser at chrome://inspect/#remote-debugging. Chrome then writes
`DevToolsActivePort` to its user-data dir and asks the user to allow each
connection. Pingex reads that file (Chrome, Beta, Canary, Chromium, Brave,
Edge) and holds the one connection (`src-tauri/src/browser/attach.rs`,
`cdp.rs`). The browser always runs where the app runs, even for a WSL Home.

**Tools.** Threads started with Settings → Browser on get the dynamic tools
in the `pingex_browser` namespace (`thread/start.dynamicTools`, which every
supported Codex tier has; they persist in the rollout). Pingex answers
`item/tool/call` in Rust (`browser/tools.rs`):

- `tabs`, `open`, `navigate` (URL, back, forward, reload), `close`
- `snapshot`: an accessibility outline with short element refs (`e12`),
  about 30× smaller than the raw `Accessibility.getFullAXTree` it is drawn
  from. Refs are keyed by backend node id, so they stay stable.
- `click`, `type`, `press`, `select`, `scroll`: each answers with only the
  snapshot lines that changed.
- `wait_for`, `read` (paged text), `evaluate`
- `screenshot`: a downscaled JPEG, sent as an `inputImage` data URL.
- `take` (the user's active tab, or one matching a title/URL) and `release`
- `console` and `network` (buffered since attach)
- `cdp` and `cdp_events`: raw DevTools Protocol, the fallback for anything
  else, on an owned tab, its child sessions, or (with Full browser access)
  the browser target. Large results are written to a file.

`cdp`, `cdp_events`, `console` and `network` are `deferLoading`, so their
schemas cost nothing per turn until the model looks them up. On models
without tool search, Codex exposes them directly.

**Ownership.** Each thread's Owned tabs sit in a Chrome tab group titled
"Pingex · <thread>". CDP cannot touch tab groups, so the Pingex Browser
Helper extension does. Its source is `extensions/pingex-chrome`. The app
builds it in and installs it to its app-data folder ("Install helper"), from
which the user loads it unpacked. It has no native-messaging host:

- Pingex attaches to the helper's service worker over the same CDP
  connection and calls it with `Runtime.evaluate`.
- The helper reports the user's own drags through a `Runtime.addBinding`
  binding. Dragging a tab out of the group releases it; dragging one in
  shares it.
- Staying attached keeps the worker alive. A tab creation or a 30-second
  alarm wakes it after Chrome starts.
- Only the helper's fixed extension id (from the manifest `key`) is trusted.

Without the helper, tabs still work and Pingex tracks ownership alone.

**Policy** (`browser/policy.rs`):

- Inside an Owned tab every tool and every raw CDP method passes through.
- Each tab has a second "gate" session holding `Fetch` interception on
  main-frame documents. A navigation to a site the thread has no Site grant
  for pauses and asks the user; if denied, it is aborted and the page stays
  as it was. The gate runs on its own session, so the agent's own `Fetch`
  use cannot switch it off. It also catches navigations started by page
  JavaScript.
- Seeing or attaching to other tabs, every site's cookies, and the browser
  target need Full browser access, which the user grants per thread.
- File uploads (`DOM.setFileInputFiles`) ask each time.

**Approvals** are Pingex's own, not Codex server requests. The waiting tool
call blocks on them. They live in Rust, so a reloaded window gets them again
(`browser/approvals.rs`).

**Cancellation.**
- When Codex resolves the request or ends the turn, Pingex aborts the browser
  work behind it.
- The Browser panel's Stop button fails the thread's pending calls, so Codex
  is told.

## What it looks like

- **Approval cards** inline in the thread: visit a site (Allow / Always allow
  / Deny), take over a tab, upload files, Full browser access.
- **Browser tab in the right panel:** connection status with setup steps,
  the helper's status, the thread's Owned tabs (focus in Chrome, release),
  its Site grants and Full browser access (revocable), and Stop.
- **In the thread:** `pingex_browser` calls render as compact browser
  action rows, with screenshots inline.
- **Settings → Browser:** on/off for new threads, browser choice, and the
  "Always allow" sites. These are separate from shell permissions.

## Not yet

- **Downloads** are not gated: Chrome's own download settings apply.
- **Spilled results** (large raw CDP output) are written to the app's temp
  dir, which a WSL-hosted Codex reads through `/mnt/...`.
- **Claude Code and maki:** the same engine can be exposed through a
  Pingex-hosted MCP server.
- **A local CDP endpoint** (`PINGEX_CDP_URL`) for the agent's own
  Playwright or Puppeteer scripts.
- **Computer use** (desktop control).
