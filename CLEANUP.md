# Cleanup log

Notes from a codebase review pass (October 2026): what was wrong, what changed,
and what was left alone on purpose. Newest section last.

## 1. Test loop speed

Baseline: `deno task test` took 40.5s wall (145s CPU) for 1,060 tests. Rust
`cargo test --lib` takes 1.5s, so the frontend suite was the bottleneck.
Vitest's breakdown showed tests themselves were a small share:
`import 219s, environment 50s, setup 29s, tests 46s` (summed over workers).

| Change | Why |
| --- | --- |
| `vitest.config.ts`: `lucideDeepImports` plugin rewrites `import { X } from "@lucide/svelte"` into per-icon imports | `@lucide/svelte` is `ssr.noExternal`, so every file rendering a component evaluated all ~1,900 icon modules through the barrel. Import time fell 219s → 57s. Test-only: Vite's build tree-shakes the barrel already. |
| 32 pure-logic test files start with `// @vitest-environment node` | They never touch the DOM but booted jsdom + jest-dom each. Forgetting the marker on a new file only costs speed. `markdown.test.ts` stays on jsdom (DOMPurify needs `window`). |
| `vitest-setup.ts` installs DOM shims and jest-dom only when `window` exists | Required for the node environment. |
| `vitest-setup.ts` drains one macrotask in `afterAll` | Unmounting a Skeleton dialog queues a zero-delay zag focus-trap timer; on a file's last test it sometimes fired after jsdom teardown and printed `document is not defined`. Flaky noise, now gone. |
| `SideQuestions.test.ts` "ends the turn locally…" uses fake timers | It waited out the real 1.5s `STOP_GRACE_MS`. Now 98ms, and it also asserts the stop button stays during the grace window. |

Result: **40.5s → ~18–20s** for the full suite. `--pool=threads` measured the
same as the default forks pool, so it stays unchanged.

Rule for new tests: a test file that never renders or touches `document`
should start with `// @vitest-environment node`. Never wait on real timers
longer than a few ms; use `vi.useFakeTimers({ toFake: ["setTimeout",
"clearTimeout"], shouldAdvanceTime: true })` so `userEvent` still works.

## 2. Make `deno task preflight` pass again

On a clean `main`, `deno task lint` failed with 27 errors and `deno task check`
with 1, so `preflight` could never go green and real regressions hid in the
noise.

- **svelte-check**: `tests/browser/composer-rich-input.spec.ts` used `Buffer`
  and the Vite/Vitest/Playwright configs run on Node, but the project had no
  Node types. Added `@types/node` (deno.json) and `"node"` to
  `tsconfig.json` `types`. App code compiled unchanged with the extra globals.
- **biome**: `architecture-review/` (generated HTML reports) is now ignored.
  This also ends the known annoyance of `lint:fix` rewriting `report.html`.
- **biome**: applied `organizeImports` and format fixes (9 files had import
  order errors, a few specs had format drift).
- `noExplicitAny`: typed the `listArchivedThreads` wire response
  (`ArchivedThreadWire` in `api.ts`) and the `view` prop in
  `RightPanel.test.ts`. Turned the rule off for `scripts/**`, which are
  throwaway JSON-RPC probes against live Codex.
- `planHandoff.test.ts`: the old comment claimed the template string avoided
  the lint, but it didn't. Replaced it with a `biome-ignore` that gives the reason.

The 7 `noExplicitAny` warnings in `homeRouting.ts` were fixed in the second
pass (section 5, item 8).

Rust side: `cargo fmt --check` had drift in 2 files and clippy had 14
warnings. Fixed all of them: `clippy --fix` for the mechanical ones, merged the
duplicate branch in `rollout.rs`, and added `#[allow(too_many_arguments)]` on
two Tauri commands, matching the existing ones. `deno task rust:preflight` is
clean.

## 3. Review findings: fixed

Four read-only reviewers swept the code: Rust drivers, the rest of Rust,
frontend logic, and Svelte components. Every finding below was re-read in
the code before it was fixed. Tests marked (test) were written red first.

### Security and data loss

- **Webview could run any shell command.** `handoff_launch_terminal(command)`
  ran whatever string the renderer sent in a terminal. One XSS in rendered
  agent markdown would have meant code execution. It now takes
  `(thread_id, cwd)` and builds the `codex resume` command in Rust with the
  same builder `handoff_command` uses for the preview.
  (`handoff/commands.rs`, `ThreadHeader.svelte`, `HandoffConfirmDialog.svelte`)
- **Discard deleted the wrong files** (test). Paths went to git as pathspecs,
  so discarding `src/routes/[id]/page.svelte` also reverted `src/routes/i/…`
  and `src/routes/d/…`, and `clean -f` deleted them. Stage, unstage and
  discard now pass `--literal-pathspecs`, and so does the file diff (it showed
  merged diffs). (`git/actions.rs::with_paths`, `git/changes.rs`)
- **config.toml / settings.json writes could corrupt or lose data** (test).
  `write_doc` and `write_overrides` used a plain `fs::write`, so a crash
  mid-write truncated the user's Codex config, MCP secrets included. Each
  read-modify-write took no lock, so an MCP save and a scope toggle could
  drop each other's change. Worse, `read_overrides` fell back to defaults on
  a parse error, so the next save silently wiped every setting and recent
  home. Now:
  - `util::fs::write_atomic` is the one helper: temp file, fsync, rename.
    It follows symlinks and keeps permissions. It replaces the two
    hand-rolled copies in `integrations/scope.rs` and `memories/files.rs`.
  - `codex_config::edit_doc` and `prefs::update_overrides` hold a lock across
    read, edit and write. Every editor goes through them: MCP commands,
    config settings, memory toggles, scope overrides, runtime settings,
    recent homes, the quick shortcut, and agent settings.
  - `update_overrides` refuses to save over an unparsable settings file
    instead of resetting it. Plain reads stay lenient.

### Behaviour bugs

- **Escape on a menu stopped the running turn** (test). The composer
  interrupts on any Escape whose `defaultPrevented` is unset. The sidebar
  context menu and FloatingMenu closed on Escape without claiming it, and the
  Handoff menu ignored Escape entirely. All three now claim it in the
  capture phase, and only while open.
- **Claude threads silently dropped image attachments** (test). The
  composer sends `{type: "localImage", path}`, but the driver only matched
  `type: "image"`. It also opened host paths with `std::fs` (wrong under WSL)
  with no size cap, while holding the lifecycle lock. `claude::driver::user_content`
  now accepts both shapes, opens via `host.to_local`, caps at 20 MB, and
  fails the send with a message naming the file. It reads before spawning or
  locking anything.
- **Claude turn-closing events were filed under turn `""`.** The translator
  takes `turn_id` when it handles `result`, and the sink read the id back
  afterwards, so the cancelled tool calls and the `TurnEnded` envelope were
  projected and journaled with an empty turn. `fail_turn` had the same
  problem. `ThreadSink::emit_in` now takes the turn explicitly, snapshotted
  before the frame is handled. (No unit test: `ThreadSink` needs a live
  `AppHandle`. `deno task test:e2e:claude` covers it.)
- **A refused Claude send could still switch the running turn's model or
  permission mode.** `ensure_process` sent `set_model`/`set_permission_mode`
  before `start_turn` refused with "a turn is already running". The check now
  comes first.
- **Closed Claude threads stayed in memory.** `close_thread` used
  `get` (into a variable named `removed`), so every thread's Translator,
  Projector and maps lived until app exit. Changed to `remove`.
- **Journal writes could land out of order.** Every write was its own
  `spawn`. When a turn ended fast, `UPDATE … complete = 1` could run before the
  `INSERT`. The turn then stayed incomplete forever, and `threads::read` fell back
  to the projection merge, which duplicates messages. An upsert would have
  made the two commute, but it would also mark turns that were never watched from the
  start as complete. Each `TurnJournal` now has one writer task draining an ordered
  queue.
- **Codex children were never reaped, and leaked when `initialize` failed.**
  `kill()`/`Drop` never called `wait()`, so each respawn, home switch and
  subagent left a zombie. An `initialize` error returned while the reader
  thread still held the child, so the process kept running. Now the reader
  reaps on EOF (Codex and Claude), every init failure kills the child, and
  `initialize` has a 90s bound. Without the bound, a Codex that hung at startup
  blocked every request for that home behind the session lock.
- **`summaryIndex` from the wire sized a buffer unchecked** (test).
  `u64::MAX` would overflow and panic the reader thread. Indices ≥ 256 are now
  ignored.
- **Slow commands froze the UI.** Tauri v2 runs non-`async` commands on the
  main thread. `check_codex_binary` (login shells, up to 30s each,
  called as the user types), `list_wsl_distros`, terminal launch, clipboard,
  Zed/Finder, attachment staging (up to 20 MB), drafts, skill reads and
  config reads/writes over the WSL share now use `#[tauri::command(async)]`.
- **WSL lookups had no timeout.** `Host::canonical` and `Host::home_dir` ran
  `wsl.exe` with a bare `.output()`, so a wedged distro hung every caller,
  bootstrap included. Both now go through `process::run` with
  `WSL_LOOKUP_TIMEOUT`.
- **Agent cwd used `Path::join` on Linux paths** (test). This broke the
  CLAUDE.md rule. On Windows, `"/home/u/x"` isn't `is_absolute()` and joins
  with `\`, so WSL agents got invalid cwds. `agents::tools::resolve_cwd` now
  takes the `Host` and works on host-path strings throughout
  (`join_str`/`is_under`).
- **A deleted repo folder was reported as "Git is not installed"** (test).
  Spawning in a missing cwd fails with the same `NotFound` as a missing
  binary. `git::run::run_error` now checks the folder first.

### Frontend races and stale state

- **Review drafts could post on the wrong PR** (test). `openPr`/`restoreDraft`
  never checked the response was still current. Clicking PR #1 then #2 could
  apply #1's pending comments while #2 was showing, then save and submit them
  there. `ReviewView` now has an `openRun` counter checked after every await.
- **Events during a thread's initial read were lost** (test). While
  `readThread` was in flight, `this.thread` was null and `handleEvent` dropped
  everything. Deltas left gaps for good (reasoning is never repaired), and a
  turn that ended during the read was finalized as *interrupted*. The session
  now buffers events during `load` and replays them onto the snapshot,
  skipping deltas for items the snapshot already holds, since deltas append.
  Side effects run only for events the old transcript never saw.
- **A queued message could send twice** (test). `syncFromServer` only
  checked `mutations > 0` at start and resolve, so a listing that raced a full
  delete put the drained entry back. Overlapping listings could also land out of
  order. `ThreadQueue` now has a generation counter bumped by every mutation
  and sync.
- **Subagent lists: an older response overwrote a newer one.** One spawn
  fires about three refreshes. They are now sequenced, and a failed refresh keeps the
  last good list instead of blanking it.
- **Deleted/archived brand-new threads reappeared** (test). The optimistic
  entry was only dropped once bootstrap listed the thread, so deleting one
  before it persisted re-inserted it on every refresh.
  `forgetOptimisticThread` is now called from delete and archive.
- **Listener setup could fail silently.** `startCodexListeners()` was neither
  awaited nor caught, and set `started = true` up front, so a failed `listen`
  left the window deaf with no error and no retry. Setup is now all-or-nothing
  (attached listeners are undone), resets `started`, and shows a toast.
  The two raw `listen` calls got a `.catch`.
- **@-mention picker went blank.** A failed search for an old query cleared
  newer results; the `catch` now has the same staleness check as the success
  path.
- **Diffs fetched twice per refresh.** The selection effect in
  `ChangesPanel` also tracked `summary` through `load`; it now uses `untrack`.
- **Pairing poll leaked.** Closing Connections during `remotePairingStart`
  started a 3s interval that nothing cleared. Guarded with a run counter.
- **Side-question answers never showed on a slow host.** A 1s
  `setInterval` superseded each read with the next, so a read slower than 1s
  was always stale. It's now a `setTimeout` chain that waits for each read.
- **Thread cost stayed stale.** The 15s usage throttle dropped changes
  instead of deferring them, so a turn that ended within 15s of the last read
  never showed its final cost. It's now a trailing throttle.
- **"Worked for 60s" / "1m 60s"** (test). Minutes and seconds were rounded
  separately; `workedLabel` now rounds once, then splits.
- **Agent runs that failed while starting held a concurrency slot forever**
  (test; found while checking the supervisor finding). `spawn_agent` drops
  the watch channel's only receiver, and `watch::Sender::send` refuses and
  *keeps the old value* when no one is subscribed. So `finish(Failed)` and
  `kill_all`'s `Killed` were no-ops until the deadline task subscribed, and a
  run that failed during startup read as `Starting` and counted against
  the cap from then on. `set_state` now uses `send_replace`.
- **Agent cap race and revived kills** (test). The cap was counted under one
  lock and the run inserted under another, with a binary lookup and file
  reads in between, so parallel spawns could both pass. `try_insert` now does
  both under one lock. A run killed during startup (home switch) was then
  set back to `Running` and its child orphaned; `start_running` refuses on a
  terminal state and the child is killed.

## 4. Performance

| Where | Before | After |
| --- | --- | --- |
| `utils/markdown.ts`, streaming a 5.7 KB reply with 3 code blocks, 300 deltas | 6.4s (21ms/delta, every finished block re-highlighted, `highlightAuto` tries every language) | 1.8s. Per-block highlight cache (bounded LRU), and an unlabeled fence that has not closed yet renders escaped until it does (test) |
| `ThreadView` folded "Worked" segments | Skeleton's `Collapsible.Content` keeps closed children mounted, so every folded segment rendered all its WorkItems (markdown, output, highlighted diffs) on load | Rendered only while open (`openWork` set). Tests now assert "not in the document" instead of "not visible" |
| `DiffBlock` | Highlighted even while collapsed; a one-line 2 MB minified diff passed the 200-line cap and was highlighted whole | Highlighted only when open; preview capped at 100 KB too, with a "Show the full diff" label (test) |
| `agentRuns.setActivity` | Rewrote `"writing…"` on every token, invalidating every agent card | Skips unchanged labels |
| `codex/rollout.rs` context breakdown, polled per token update | Re-parsed the whole rollout whenever it grew (quadratic over a turn) and re-walked `sessions/` per call | Resumes from the last offset (a half-written last line is held back), and caches the path per thread (test) |
| Claude `start_turn` | Loaded and JSON-parsed every journaled item to test emptiness | `storage::has_thread_items` (`SELECT 1 … LIMIT 1`) (test) |
| Bootstrap worktree discovery, on every sidebar refresh including after each turn | A `git worktree list` spawn per discovered worktree, plus two `wsl.exe readlink` spawns per classification on WSL | Parent lookups memoized (re-validated with a metadata check); WSL canonicalisation memoized inside the worktree classifiers |

The WSL canonical memo caches only what the distro answered
(`Host::wsl_resolve`). When a lookup fails (distro stopped or wedged), the
lexical stand-in is used for that call and not remembered, so it can't stick
for the life of the app. The failure also doesn't trigger a second
30s lookup.

### Windows

- `main.rs` lacked `windows_subsystem = "windows"`, so a release build
  opened a console window. Added it, and gave console children (`wsl.exe`,
  `git`, the `cmd` URL opener) `CREATE_NO_WINDOW` through
  `util::host::no_console_window`, so spawns don't flash a window each. The
  terminal handoff deliberately keeps its console. (Type-checked for
  `x86_64-pc-windows-msvc` in debug and release; see section 5, item 11.)

## 5. Second pass: the items first left open

The first pass parked these as "needs a design decision or a live harness".
All of them are now fixed:

1. **Frontend branched on the harness kind** (CLAUDE.md forbids it). There was
   no `Capability` type in the code, only in the docs. Added
   `harness::Capability` (`goals`, `revert`, `permissionModes`,
   `collaborationModes`, `liveToolResults`), declared per harness by
   `HarnessKind::capabilities`. It is exported into `bindings.ts` as the
   constant `HARNESS_CAPABILITIES` (tauri-specta `.constant`), so there is no
   startup fetch and no window where the composer guesses. The frontend asks
   `can(harness, capability)` (`utils/capabilities.ts`, tested) for goals,
   message editing, Claude modes vs Codex presets plus collaboration mode, the
   plan toggle, and live tool folding. Model catalogs go through one
   `listHarnessModels(harness)` for every harness, with Codex's speed-field
   normalization shared. That also fixed the first send of a Claude thread waiting on
   the *Codex* model list. Approvals carry the requesting harness from the
   envelope instead of a hard-coded `"claude"`, and titles name it via
   `harnessName`. What still mentions a harness by name is identity, not
   behaviour: labels and badges, routing threads and `codex://` handoffs to a
   home of their harness, and a home's default config dir.
2. **Sidebar data from overlapping requests** (test). Every request that
   replaces the sidebar takes a ticket when it starts (`dataTicket` /
   `applyFresh` / `applyTicketed` in `appData.svelte.ts`). A response older
   than one already applied is dropped. If the dropped one was an action
   result, one fresh read follows, because the server may have committed it
   after the newer read. All 28 call sites converted.
3. **Claude effort changes on a live process.** `ensure_process` now
   restarts an idle process under the same session when effort changes
   (`--effort` is spawn-only), keeping the permission mode. Verified against
   the real CLI with a new live test,
   `a_session_resumed_at_another_effort_keeps_its_context`. The context
   survived, and the test fails when inverted.
4. **`ensure_resumed` recorded the resume on the wrong child** after a
   respawn. `request_served` now returns the session that answered.
5. **Claude spawn did blocking IO under the driver-wide lock.** `spawn` is
   async; binary lookup, `~` expansion and the session probe run in
   `spawn_blocking`.
6. **`item/completed` was deep-cloned onto the ledger queue.** The reader
   sketches the item (a few counts) and sends that. `trim_live_params` checks
   string lengths before cloning anything (test for the diff path).
7. **Native Windows handoff** now builds `set "CODEX_HOME=…" && codex resume
   …` for `cmd /K` (test).
8. **`homeRouting.ts`'s `any`s** replaced with `unknown` plus a narrow
   `JsonRecord`/`Summary` typing. Lint now has 0 warnings.
9. **The Thread overview card covered Stop/Send.** `FloatingMenu` moved inside
   the transcript region, so its `bottom-4` anchor ends above the composer. The
   e2e workaround (collapsing the overview) was removed, and the interrupt
   spec now clicks Stop with the overview open.
10. **The Claude turn-id fix had no test.** The driver now calls
    `Translator::frame_in_turn`, and a test drives a `result` through the
    translator and projector, asserting the cancelled call lands in its own
    turn.
11. **The Windows-only code was never compiled.** The whole crate can't
    cross-check here (turso's build script needs `rc.exe`), so the exact
    `no_console_window` code and the `windows_subsystem` attribute were
    checked for `x86_64-pc-windows-msvc` (debug and release) in a scratch
    crate. The target was removed again afterwards.

Not a defect: the preview simulator starts its slow "command" scenario for
any prompt containing "command". That's by design (keyword-driven scenarios);
noted so specs avoid the word.

## 6. Playwright suite back to green

`deno task test:e2e:quick` failed 8 of 64 on untouched `main` (checked in a
HEAD worktree). All were stale specs, not app bugs:

- **No model or permission chosen** (3 specs). A fresh profile has neither, and
  the composer deliberately refuses to send ("Choose a model", unit-tested).
  Added `tests/browser/turnSettings.ts::chooseTurnSettings`. The chips spec
  had only passed because its send silently failed. Now that sends work,
  echo checks use `.last()`: new threads are titled with their first
  message, so the text also appears in the sidebar and header.
- **Thread list truncation**: `hideOldThreads` defaults on (37905e4), so the
  spec turns it off before checking the 15-thread cap.
- **Thread disclosures**: output moved into a header row (05be7ba) and long
  output now wraps (0fa53c3). The locator and the overflow assertion were
  updated to match.
- **Approval**: the prompt contained "command", which triggers the slow
  simulated command, so the reply came after the expect timeout. The prompt
  was reworded.
- **Interrupt**: the button is "Stop". `{ name: "Interrupt" }` substring-matched
  the new thread's sidebar row, so the interrupt never fired.
- **Right panel**: the header was renamed "Changes" → "Outputs" (795b117), and the
  "Files" button needed `exact: true`.

Result: **64/64**, stable across repeat runs.

## Final state

| Check | Before | After |
| --- | --- | --- |
| `deno task check` | 1 error | 0 |
| `deno task lint` | 27 errors | 0 errors, 0 warnings |
| `deno task test` | 1,060 pass, 40.5s | 1,075 pass, ~16s |
| `deno task rust:preflight` | fmt drift, 14 clippy warnings | clean, 519 tests |
| `deno task test:e2e:quick` | 56/64 | 64/64 |

## 7. Live suites: parallel, and no longer able to log you out

| Suite | Before | After |
| --- | --- | --- |
| `test:e2e:claude` | 53s, `--test-threads=1` | 15s, default parallelism (tests already isolated; passes unchanged) |
| `test:e2e:codex` | 975s for 30 tests, serial, one shared app-server (and 21 failing on a spent login) | **~100s, 30/30** — one app-server + scratch home per test, `RUST_TEST_THREADS=2`. With a dead login it now stops in 28s with the fix in the message. |

**Credential hazard found while running it.** 21 of 30 Codex tests failed
with *"your refresh token was already used"*. The harness *copied*
`auth.json` into its scratch home, and refresh tokens are single-use: when a
copy refreshed an expired access token, it spent the refresh token the source
home still held. `~/.codex-personal` (the default source, and the home
`deno task dev` uses) is in that state now. Its file says logged in, but its
next refresh will fail. Fixes in `tests/live_codex/harness.rs`:

- Scratch homes **symlink** `auth.json` instead of copying it. Codex writes the
  file in place (open/truncate), so a refresh through the link lands in the
  source and keeps it valid.
- The first **model request** of a run (`turn/start`, `review/start`,
  compaction, goal set) is preceded by **one warm-up turn**, and every other
  test's first model request waits for it (`OnceLock`). A due refresh therefore happens
  once, never from parallel processes at the same moment. Tests that never
  reach the model don't wait, so they still run with a bad login. If the
  warm-up fails, every model test stops at once with the fix in the message,
  instead of 30 tests each timing out.
- Old runs had left `auth.json` *copies* in `$TMPDIR/pingex-live-e2e-*`;
  those dirs were removed.
- The shared `static SERVER` became a per-test `Server` (`live!(server)`),
  dropped at the end of its test: the process is killed and reaped, and the
  scratch root is removed unless the test failed (its `logs_2.sqlite` holds
  the reason). `mcp_echo` is built once behind a lock.

Verified 2026-10-13 after re-logging `~/.codex-personal` (Codex 0.162.1).
The login was still valid afterwards, and no scratch dirs were left.

| Parallelism | Runs | Green | Time |
| --- | --- | --- | --- |
| 1 (old default) | 1 | 1 | 176s |
| 2 (**new default**) | 6 | 6 | 93–116s |
| 4 | 4 | 2 | 58–65s green; stalled runs 128–224s |

The failures at 4 were all *stalls*: a response websocket that opens and
never delivers, with no error in Codex's log, and the same tests passed alone
in 8s. That points at upstream load from concurrent sessions, so the default
is 2. `harness::STALL_TIMEOUT` (60s of total silence) now ends such a turn
with "stalled: … (an upstream hang, not a slow turn)" instead of waiting out
the 180s `TURN_TIMEOUT`. A throwaway probe on an idle server confirmed it
fires at 60.5s.

### App-side Codex changes, checked in the real app

The live harness has its own client, so it never runs the app's
`CodexSession`. To cover section 3/5's Codex changes (the `spawn_blocking`
orphan sweep and binary lookup, the `initialize` timeout, kill-on-failure,
and reaping) beyond unit tests, `deno task dev` was launched against
`~/.codex-personal` without sending a turn:

- The app spawned its own `codex app-server --stdio` child, which initialized
  and served bootstrap (28 state-database handles open, no errors in the dev
  log).
- On quit (SIGTERM) the app exited and the child went with it: no orphan, no
  zombie.

The two fixes a live run cannot provoke (both need a respawn or a slow
write at exactly the wrong moment) were given seams and tests instead:

- **Journal ordering**: the writer queue is now `codex::journal::OrderedWrites`
  (resolver-injected, so tests use a real on-disk database).
  `journal_writes_land_in_queue_order` queues a slow `record_turn_start` and an
  instant `mark_turn_complete` and asserts the turn reads complete. Run
  against the old behaviour (each write spawned on its own), the same test
  fails.
- **Resume recorded on the serving session**: the retry-and-respawn loop is
  now `codex::session::served_by`, which returns the response together with
  the session that answered. Tests: a respawn mid-request hands back the
  respawned session, and a request that may have run is not resent.
  `ensure_resumed` records on that returned session.

## 8. Second review sweep (stopped part-way at the user's request)

Three more reviewers covered the areas the first pass barely touched. I
re-read each finding below in the code before fixing it; "(test)" means
red on the old code, green after.

**Fixed:**
- Sidebar drag-and-drop: dropping an item just below its upper neighbour
  anchored on itself and moved it to the end (test). A folder dragged over a
  thread row inside itself became its own parent (test). Escape during a drag
  now claims the key, so it no longer interrupts the running turn.
- Editing a workspace silently unlinked members whose project was hidden
  (test).
- Agent limits: an emptied or out-of-range "Time limit" saved as a
  1-second timeout. Values are now validated (test), and the inputs got
  accessible names.
- Review: a submitted review could clear and delete *another* PR's draft
  (PR number now pinned before the await). A failed reply no longer throws
  away the typed text (test).
- Side questions: a fork could be filed under the wrong thread after a
  switch, and the loading skeleton could stick.
- QuestionCard: a failed answer is shown instead of an unhandled rejection,
  and a stranded retry doesn't resend the message (test). ElicitationCard: a
  failed reply keeps the card (the MCP request is still waiting), and form
  defaults are seeded (`initialValues`).
- GitTab no longer shows one file's diff under another's header. Tracking a
  remote branch on a dirty tree offers "Switch anyway" (test).
- Stale async results: ProjectDetail search after clearing, Integrations MCP
  status, and MemoriesView save/delete. ProjectDetail instruction saves now
  report failure. The add-skill scroll uses the row's real id. An edited
  Claude Home re-probes its login. Plus the WSL canonical memo fix (section 4).

**Rust fixes applied by a background agent that was stopped mid-run.** They
compile and pass `rust:preflight` (527 tests, including ones it added), but I
have **not** reviewed them line by line:
- `connections/commands.rs`: revoke no longer reports success when the
  revoke wasn't sent.
- `usage/ledger.rs` + `harness/project.rs`: Claude's per-process totals; each
  turn books its own `last` (with a test).
- `review/queries.rs`: PR page merging and the git option-injection guard.
- `storage/search.rs`: search matches message text.
- `storage/projects.rs`: `update_store` holds a lock across
  read-modify-write; callers converted in `projects/commands.rs`.
- `profiles.rs` and `threads/lifecycle.rs`: changes from the same batch.

Review that diff before committing.

**Found, not yet fixed:**
- Session focus and `clearThread` compare the sidebar's root thread ids with
  `view.threadId`, which is a version (leaf) id.
- SidebarSearch "Load more" within the debounce mixes queries, and its timer
  isn't cleared on unmount.
- From the Rust list, possibly incomplete: caching rollout-lookup misses,
  pruning per-thread rows on delete, and clamping agent limits in
  `prefs::write_agent_settings`.
