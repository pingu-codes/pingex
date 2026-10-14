# Refactoring opportunities

A survey of where the codebase can get smaller and easier to edit, done on
2026-10-13 against `main` at `13898d2`. Nothing here has been changed yet.

**Method.** Eight read-only reviewers each covered one area: thread UI,
composer, layout/panels/app, services/types/utils, the frontend feature
folders, the Rust harness drivers, the Rust backend services, and
tests/scripts/repo hygiene. Each finding was checked against the code by its
reviewer. The headline claims were re-checked afterwards with `grep`, including
the counts in the summary table, the dead-code lists, the compat-gate versions,
the tracked-file sizes and the bundle contents. Line numbers are as of
`13898d2` and will drift.

**Reading the entries.** Savings are rough estimates of lines removed, net of
any new helper. "Risk" means how likely the change is to break behaviour and
how much re-verification it needs. Items fixed in the October 2026 cleanup
pass (the former `CLEANUP.md`, now in git history) are left out. Its open
items moved to §13.

**Scale.** Tracked source (excluding fixtures and `bindings.ts`) is about
3.9 MB. On top of that, about 0.9 MB of tracked files were generated or
duplicated (section 1); §1.1, §1.4 and §1.5 are done.

---

## Contents

0. [Top 15 by value](#0-top-15-by-value)
1. [Repo hygiene](#1-repo-hygiene)
2. [Cross-cutting frontend helpers](#2-cross-cutting-frontend-helpers)
3. [Skeleton components replacing hand-rolled UI](#3-skeleton-components-replacing-hand-rolled-ui)
4. [Services, types and the preview layer](#4-services-types-and-the-preview-layer)
5. [Thread UI (`src/lib/thread`)](#5-thread-ui-srclibthread)
6. [Composer (`src/lib/composer`, `components`, `quick`)](#6-composer)
7. [Layout, panels and app state](#7-layout-panels-and-app-state)
8. [Feature folders (integrations, git, worktrees, review, memories, usage)](#8-feature-folders)
9. [Rust: harness drivers, threads, agents](#9-rust-harness-drivers-threads-agents)
10. [Rust: commands, storage and services](#10-rust-commands-storage-and-services)
11. [Tests](#11-tests)
12. [Rule violations found along the way](#12-rule-violations-found-along-the-way)
13. [Bugs found along the way](#13-bugs-found-along-the-way)
14. [Suggested order](#14-suggested-order)
15. [Checked and left alone](#15-checked-and-left-alone)

---

## 0. Top 15 by value

| # | Opportunity | Where | Est. savings | Risk |
|---|---|---|---|---|
| 1 | ~~Untrack generated Tauri schemas~~ (done) | §1.1 | ~578 KB, 6 files | low |
| 2 | ~~Move preview fakes out of `api.ts` behind the `commands` proxy~~ (done) | §4.1 | ~800 lines from api.ts (~450 net) | med |
| 3 | ~~Delete ~71 pass-through wrappers in `api.ts`~~ (done) | §4.2 | ~250 lines | med |
| 4 | `HomeWindow::ctx()` so 144 commands drop their `State` param | §10.1 | ~150–250 lines | low |
| 5 | `db::conn` / transaction boilerplate in storage | §10.2 | ~190–220 lines | low |
| 6 | Shared frontend test factories (`makeProject`, `makeThreadSummary`, `makeBootstrap`, `worktreeEntry`) | §11.1 | ~250–300 lines | low |
| 7 | Retire Codex compat gates older than the last-stable tier | §9.1 | ~200–300 lines | med (policy call) |
| 8 | Deduplicate `.agents/skills` vs `.claude/skills` | §1.3 | ~205 KB, 51 files | low |
| 9 | ~~`errorMessage(cause)` helper (109 copies)~~ (done) | §2.1 | ~4 KB, many 3→1-line handlers | low |
| 10 | ~~Tailwind `@utility` classes for repeated class strings~~ (done) | §2.6 | ~10 KB of markup | low |
| 11 | git/commands.rs: use the existing `locked()` and add `blocking()` | §10.3, §10.4 | ~170–210 lines | low |
| 12 | ~~Split ThreadView (goal banner, scroll follow)~~ (done) | §5.2, §5.3 | ~330 lines out of the 1,489-line file | med |
| 13 | ~~Split Composer (attachments, draft, settings) and use Skeleton Popover~~ (done) | §6.2, §6.3 | Composer ~1,374 → ~600 lines; ~130 removed | med |
| 14 | Shared JSONL child process core for Codex and Claude | §9.2 | ~100–130 lines | med |
| 15 | Split IntegrationsSection into dialogs and rows | §8.1 | ~120 lines; file ~1,100 → ~450 | med |

---

## 1. Repo hygiene

### 1.1 Untrack generated Tauri ACL schemas: done
- `src-tauri/gen/schemas/` (578 KB, 6 files; the four platform schemas were
  byte-identical) is gitignored and untracked. `tauri-build` recreates
  `desktop-schema.json`, which `capabilities/*.json` `$schema` points at,
  byte-identical when the build script runs (checked by deleting the folder
  and forcing a rerun). Only the host platform's schema is written, so the
  `linux`/`windows` copies no longer appear on macOS.

### 1.2 `llms/skeleton.txt` (790 KB): resolved
- CLAUDE.md now points to it as the local Skeleton docs (grep, then read a
  range), so it stays.

### 1.3 `.agents/skills` duplicates `.claude/skills`
- **Where:** 51 files in each tree.
- **What:** 50 of the 51 are byte-identical (~205 KB). The one that differs,
  `.agents/skills/verify/SKILL.md`, is stale: it still says
  `cd app && deno task check`. Several skills are generic and unrelated to
  this repo (`teach`, `setup-matt-pocock-skills`, `triage`, `to-tickets`,
  `ask-matt`).
- **Proposal:** make `.agents/skills` a symlink to `.claude/skills`, after
  checking that Codex follows a symlinked skills dir. Move the generic
  personal skills to the user-level skills directory.
- **Savings:** ~205 KB / 51 files, ~60 KB more for the generic ones.
  **Risk:** low–med (symlink support).

### 1.4 Leftover top-level docs and assets: done
- Deleted `REQUIREMENTS.md`, `floating menu.png`, `features/PRIORITY.md` and
  the eight one-paragraph specs for finished features (00, 01, 02, 04, 05,
  09, 10, 11).
- `features/README.md` is now an index: specs kept for reference (03, 12–16),
  the deferred ones (06–08), and the removed ones.
- `architecture-review/` is untracked and gitignored (the local copy stays).
  biome's explicit exclude went with it, since biome reads `.gitignore`.
- `CLEANUP.md` is deleted. Its durable rules (the `@vitest-environment node`
  marker, fake timers, `chooseTurnSettings` in browser specs) and the live
  suite's credential rules are now in CONTRIBUTING.md, which also had a
  stale claim that the live suite copies `auth.json`; it symlinks. Its open
  items are §13.9–13.12.

### 1.5 `.claude/settings.local.json`: done
- Untracked and gitignored; the local file stays.

### 1.6 `scripts/check-project-integrations.ts`
- **What:** 6.7 KB ad-hoc protocol probe with a hard-coded
  `/opt/homebrew/bin/codex`. It's referenced only from
  `features/03-integrations.md:28` and isn't a deno task. It sits outside the
  three-tier version policy the live suite enforces.
- **Proposal:** port the plugin and project-rule cases to `live_codex` and
  delete the script. Skills and MCP are already covered in
  `live_codex/main.rs:69-103`.
- **Savings:** ~180 lines. **Risk:** med (the live suite spends quota).

### 1.7 deno.json: pin each tool version once
- **Where:** `vitest@4.1.10` appears 4 times, `playwright@1.61.1` 5,
  `@biomejs/biome@2.5.4` 4 and `vite@8.1.5` 3. The `imports` entries for
  CLIs (`@biomejs/biome`, `svelte-check`, `@tauri-apps/cli`) are never
  imported by code.
- **Proposal:** add base tasks (`"vitest": "deno run -A npm:vitest@4.1.10"`,
  `"pw"`, `"biome"`, `"vite"`) and compose the rest from them
  (`"test": "deno task vitest run"`). Drop the unused import entries.
- **Savings:** readability; a version bump becomes a one-line edit.
  **Risk:** low.

### 1.8 Small config overlaps
- `vitest.config.ts:54-57` and `vite.config.ts:13-16` both declare the same
  `$lib` alias and svelte plugin, and `tsconfig.json` `paths` declares the
  alias a third time. Use `mergeConfig` on a shared base, keeping Tailwind out
  of tests.
- `playwright.demo.config.ts` repeats `webServer`/`baseURL` from
  `playwright.config.ts`. Export a `previewServer` constant for the demo
  config to spread.
- **Savings:** ~15 lines. **Risk:** low.

---

## 2. Cross-cutting frontend helpers

Several reviewers reported these independently. Each is a small utility plus
a mechanical sweep.

### 2.1 `errorMessage(cause)`: done
- `errorMessage` in `src/lib/utils/errors.ts` replaces all 109 inline copies,
  including `MemoriesView`'s local `message()`, the `is*Unsupported`
  predicates in `api.ts`, `submitState` and `gitErrors.ts`. ThreadView has
  `fail(cause)` for its 11 `session.streamError` sites.

### 2.2 `basename` / `trimSlash`: done
- `src/lib/utils/path.ts` has `basename` (splits on `/` and `\`, ignores
  trailing separators) and `trimSlash`. They replace the 14 local copies and
  the 17 inline trailing-slash trims. `folderName` is gone; `shortHomeName`
  and `dirName` keep their `null` handling and wrap `basename`.
  `appData.directoryKey` keeps its own normalisation.

### 2.3 Formatters: done
- `src/lib/utils/format.ts` has `formatBytes`, `formatTokens`,
  `formatTokensShort` and `relativeTime(ts, { suffix, granularity, now })`.
  `utils/time.ts`, `formatSize`, the memories `formatBytes`, both token
  formatters and the `HomePicker`/`HomePage` copies are gone, and the inline
  KB maths in `FilePreviewDialog` and `GitTab` uses `formatBytes`.
  `formatAge` and `lastSeenLabel` wrap `relativeTime` for their special cases.
- Compact token counts now read `k`/`M`, whole numbers from 100 up
  (`120.6K` → `121k`); sizes read `2 KB`, not `2.0 KB`.

### 2.4 `escapeHtml` and `.code-dark`: done
- `utils/markdown.ts` exports `escapeHtml` and `highlightDiff(patch,
  maxBytes)`; the copies in CodeView, ChangesPanel and GitTab and their
  `HIGHLIGHT_BYTES` logic are gone. A global `.code-dark` in `app.css`
  replaces the seven `#0d1117` blocks, and markdown code blocks render with
  it. No `DiffPre.svelte`: it wouldn't pay for itself. DiffBlock keeps its own
  truncation.

### 2.5 Global markdown prose styles: done
- `.prose-md` (from WorkItem's `.prose-thread`) and the `.prose-md-compact`
  modifier (from `.prose-skill`/`.prose-side`) live in `app.css`. WorkItem,
  MemoriesView (which now gets styles: the bug), the SKILL.md preview,
  ReasoningBlock and RightPanel's plan view use them. Reasoning, plan and
  SKILL.md text now also get list bullets, inline-code chips and code-lang
  labels.

### 2.6 Tailwind `@utility` classes: done
- `app.css` defines `eyebrow` (11px; 10px sites add `text-[10px]`),
  `error-note`, `panel-card`, `empty-state`, `empty-cta`, `popover-card`,
  `menu-row`, `pill`/`pill-active`, `tooltip-bubble` and `field` (the
  hand-rolled inputs; Skeleton's `input` looks too different, see §3.6).
  About 190 sites use them.
- Left alone because the recipes differ: the large `p-4 text-sm` error
  panels, the 11px QuickChat/Memories errors, HomePicker's add-home button,
  the dashed rows in SectionPicker/Integrations/SystemPrompt and menu-like
  rows without `w-full`/`text-left`.
- Visible changes: SidebarContextMenu rows are `text-xs` (line height ~2px
  shorter) and six error notes moved from `rounded-md` to the card radius.

### 2.7 Small rune helpers: done
- `utils/flash.svelte.ts` has `flashFlag(ms)` and the keyed `flashKey(ms)`,
  used by WorkItem, UserMessageBubble, ThreadHeader, FilePreviewDialog and
  ThreadDebugPopover.
- `utils/resource.svelte.ts` has `resource(fetcher)` → `{ value, error,
  loading, reload, set }` with latest-request-wins built in. It's used by
  IntegrationsSection, UsageSummary, the GitTab and ChangesPanel diffs and
  the three worktree dialogs. That fixed two races, each with a test: a late
  older diff overwriting ChangesPanel's cache, and a deselected file's diff
  landing in GitTab.
- `submitState` moved to `utils/actionState.svelte.ts` (with `describe` and
  `exclusive: false`). It's used by the request cards (§5.10),
  IntegrationsSection, ReviewView's submit/reply/resolve and Worktrees.
- Left alone: ReviewView `openPr`/`openLocal` (they share one guard with
  `loadProvider`/`restoreDraft`), GitTab `commit()` (one `busy` label across
  all mutations) and `copy.ts` (it toggles a DOM class).

### 2.8 A generic `ConfirmDialog`: done
- `components/ConfirmDialog.svelte` takes `{ title, body, detail, confirmLabel,
  cancelLabel, tone, icon, width }`. DiscardQueuedDialog, ReplaceGoalDialog,
  DeleteThreadDialog, git/DiscardDialog and the dead RewindThreadDialog are
  deleted. Thread titles and goals in the body are now quoted plain text,
  not bold.

### 2.9 Shared image-extension table: done
- Rust: `util::media` has `IMAGE_EXTENSIONS` (ext → MIME), `image_mime` and
  `is_image_path`, used by `composer/attachments.rs` and `files/preview.rs`.
  TS: `utils/media.ts` has `IMAGE_EXTENSIONS`, `isImagePath` and `guessKind`,
  used by Composer and the preview attachments fake. Both lists are the union:
  png, jpg, jpeg, gif, webp, bmp, tif, tiff, heic, avif, ico, svg. The stage
  command still doesn't return the kind.

---

## 3. Skeleton components replacing hand-rolled UI

CLAUDE.md asks for Skeleton first. All of §3 is done; the notes below record
what changed and what was left alone on purpose.

### 3.1 Menus: done
- `SidebarContextMenu` is a Skeleton `Menu` built from an
  `{action, icon, label, danger, title, show}` list. The backdrop, the window
  Escape handler and Sidebar's 80/110/330/380 height guesses are gone.
  `Menu.ContextTrigger` can't carry a per-row value, so Sidebar calls
  `useMenu` and spreads zag's `getContextTriggerProps({value})` /
  `getTriggerProps({value})` onto rows and ellipsis buttons. There are two
  menus over one item list: zag never positions a context-trigger menu opened
  from a button, so right-click and the ellipsis use separate instances.
- FloatingMenu "Open in", ThreadHeader Handoff, the Worktrees overflow menu
  and QuickChat's project picker (radio items; focus returns to the input)
  are Skeleton `Menu`s. Zag's dismissable layer calls `preventDefault` on
  Escape in the capture phase, so the composer still never sees it as an
  interrupt (tests cover Sidebar, FloatingMenu and ThreadHeader).

### 3.2 Popovers in the composer: done
- `composer/ComposerPopover.svelte` sits on Skeleton `Popover` with a
  controlled `open`, so `/model` and `/permissions` still open it from code.
  Model, Permissions and SubagentPolicy popovers use it.
  `positionComposerPopover.ts` and Composer's window `onclick` closer are
  deleted.
- It isn't portalled, so More's outside-click check still counts clicks in
  it as inside; it is fixed-positioned so More's scroll area doesn't clip it.
  The More panel itself stays hand-rolled: it must stay rendered while hidden
  so the toolbar can measure item widths.
- Opening a popover now moves focus into it and closing returns focus to the
  trigger.

### 3.3 Switches: done
- `components/Toggle.svelte` wraps Skeleton `Switch` (sr-only label). Its
  hidden checkbox carries `role="switch"` and the old `data-testid`s. Used by
  SettingsView (twice) and MemoriesView; tests use `toBeChecked()`.

### 3.4 Collapsibles / disclosures: done
- ArchivedThreadsSection and HomePage's "Archived & hidden" are Skeleton
  `Collapsible`s (they gain `aria-expanded`; archived threads still load on
  first open).
- IntegrationsSection's MCP details and SKILL.md view are `Collapsible`s
  driven by the existing `expanded`/`skillOpen` maps through `open` +
  `onOpenChange`; the maps stay because a focus request and a failed SKILL.md
  load change open state from code. The focus-scroll waits for `tick()`.
- WorkItem: see §5.6.

### 3.5 Segmented controls / tabs: done
- `components/Segmented.svelte` wraps Skeleton `SegmentedControl`
  (`value`, `options: SegmentedOption<T>[]`, `onChange`, `label`, `size`,
  `fill`, `iconOnly`, `disabled`). It replaces the IntegrationsSection filter
  and transport, CreateWorktreeDialog's and BranchSwitcherDialog's modes,
  UsageSummary's range and ThreadView's new-thread location picker. They
  render as radio groups; tests query `role="radio"`.
- All segmented controls now share one look (grey track, raised indicator);
  the integration filter and usage range used to be primary-filled pills.

### 3.6 Inputs, tooltips, progress: done
- Inputs: git/ and worktrees/ already used the `field` utility from §2.6,
  which deliberately replaces Skeleton's larger `input`; the three remaining
  hand-styled review textareas now use it too. IntegrationsSection spreads a
  `codeInput` const for its 8 autocomplete/autocorrect/spellcheck sets.
- HomePage's `iconAction(label, Icon, …)` takes the icon and renders
  TooltipButton (this fixed "Workspace details" showing an X icon).
- `TooltipButton` renders through `TooltipAnchor tag="button"`; TooltipAnchor
  gained optional `tag`, `placement`, `content` and `contentClass`.
- ContextMeter's hover card is a `TooltipAnchor`; the meter uses
  `aria-disabled` so the stats stay reachable while compaction is
  unavailable.
- The goal budget bar is Skeleton `Progress` (value clamped to the budget:
  `Progress` throws above max). FloatingMenu's process filter chips are a
  multiple `ToggleGroup` that keeps the pill look and `aria-pressed`.

### 3.7 Shared composite components: done
- `components/DirectoryField.svelte` (path input + `isTauri()`-gated Browse)
  is used by AddWorktreeDialog, CreateWorktreeDialog and AddProjectDialog;
  the Tauri `open` import lives only there, so the `openDialog` collision is
  gone. AddProjectDialog's Browse is now an icon button, hidden in preview.
- `components/HostSelect.svelte` reads `launch.distros`, always says "This
  computer", and still offers a selected distro missing from the list. Used
  by SettingsView (twice), AddProjectDialog, ProfileHomes and HomePicker; the
  three per-component `listWslDistros` fetches are gone.
- `components/BranchList.svelte` is shared by BranchSwitcherDialog and
  CreateWorktreeDialog. `remote` gives the grouped Remote heading,
  `combobox` gives arrow/Enter navigation (now with
  `aria-activedescendant`), plus `currentLabel`, `selected`, `disabled`.
  Checked-out branches use real `disabled`.

---

## 4. Services, types and the preview layer

All of §4 is done. `api.ts` went from 2,223 to 456 lines.

### 4.1 Move the browser-preview fakes out of `api.ts`: done
- `services/preview/commands.ts` is a `Partial` of fakes keyed by command
  name. The routed `commands` proxy in `homeRouting.ts` sends every command
  there when `!isTauri()`; commands without a fake resolve `undefined`.
  Fakes return wire shapes, so the real unwrapping (queue pages, usage,
  model normalising, MCP status keying, skill dedupe) runs on top of them.
- The only `isTauri` branches left in api.ts are Tauri-only by nature
  (`threadImageSrc`, `stageBrowserFile`).
- `homeRouting.test.ts` runs in node, so it mocks `$lib/services/tauri` to
  report Tauri.

### 4.2 Delete the thin pass-through wrappers: done
- 150 wrappers are gone; callers use `commands.x` from
  `$lib/services/homeRouting`, passing defaults explicitly
  (e.g. `commands.gitBranches(dir, null)`). `saveProject`, the five
  `*NativeSideQuestion` aliases, `clearThreadGoal` and
  `removeStagedAttachment` went to their commands.
- 34 remain because they reshape arguments or results (queue, usage, MCP
  status, skills, models, `startThread`/`startTurn`/`forkThread`, goal
  setters, …) or only cast an untyped `Json`/app-server result
  (`readThread`, `listSubagents`, `startReview`, `getThreadGoal`,
  `readAccountRateLimits`, `revertThread`, `remotePairingStart/Status`).
- `AgentSettingsPayload.sandboxOptions` is now required, which removed the
  `readAgentSettings`/`writeAgentSettings` casts.
- 21 test files mock `$lib/services/homeRouting`'s `commands` instead of
  `$lib/services/api`. No shared helper: an imported helper can't be used
  inside a hoisted `vi.mock` factory.

### 4.3 Dead exports in api.ts, their routing and their Rust commands: done
- Removed `readClaudeStatus`, `listArchivedThreads` + `ArchivedThreadWire`,
  `queueStart`, `openAgentThread`, `reloadMcpServers`, `checkCodexBinary`,
  `previewArchived`, their routing branches, and the Rust commands
  `list_archived_threads`, `read_claude_status`, `queue_start`,
  `open_agent_thread`, `reload_mcp_servers`, `check_codex_binary` with their
  registrations and the helpers only they used. Fixed bug §13.2.

### 4.4 Preview fixtures ship in the production bundle: done
- The preview module loads through `import.meta.env.DEV ? await import(…)`,
  so production builds contain no preview code (`dist/assets` has no
  `preview-pairing-code`). Playwright and the demo run against the dev
  server. A production build opened in a plain browser now gets `undefined`
  from every command.

### 4.5 `types.ts`: a hand-maintained re-export list and rename aliases: done
- `types.ts` is `export type * from "$lib/bindings"` plus local types.
- All 17 aliases are gone, each by a Rust-side rename: 14 structs carry
  `#[serde(rename = "…")]` (changes the generated name, not the JSON;
  container `#[specta(rename)]` is deprecated in rc.25), and two enums were
  renamed outright because specta ignores serde renames on enums
  (`WorktreeBranch` → `WorktreeBranchRequest`, `UsageScopeArg` →
  `UsageScope`). `AgentSettings` went with §4.2's `sandboxOptions` change.
- `TurnOptions` is the generated type with three fields overridden;
  `ClaudeStatus`/`HarnessKind` come from bindings;
  `codexVersionFromUserAgent` lives in `utils/userAgent.ts`.

### 4.6 `homeRouting.ts` proxy is a 230-line if-chain: done
- A `routers` table (usage breakdown, search, every-Home fan-out,
  profile-root sidebar, id collection) with single-owner routing as the
  fallback, plus `resultMappers` and `defaultHarness` tables. `hostKey`,
  `hostLabel` and `sameHost` come from `$lib/app/host`. The file grew
  (667 → 743 lines, mostly per-router docs and the preview dispatch); the
  gain is readability.

### 4.7 `codexEvents.svelte.ts` listener boilerplate: done
- `listenScoped(evt, handler)` and `persistQuestion(…)`. Listeners still
  attach one at a time so a failure undoes all of them.

### 4.8 `is*Unsupported` predicates: done
- One `isUnsupported(cause, ...prefixes)`; the named predicates are
  one-liners over it and every `*_UNSUPPORTED` constant stays.

### 4.9 Repeated preview mutation snippets: done
- `editThread`/`editProject` helpers, one `removeThread` for
  archive/delete, one `dropConnection` for disconnect/revoke, one Claude
  status literal and one goal fake; the goal setters share
  `updateThreadGoal(threadId, patch)`.

### 4.10 Over-exported symbols: done
- The listed symbols are un-exported; `WorkspaceSearchGroup`, `RecentHome`,
  `WorkspaceFileMatch` and `WorkspaceThreadMatch` are deleted.
- `handoff_copy` is renamed `copy_text`, so `commands.copyText` is the one
  name, and `isTauri` is imported from `$lib/services/tauri` everywhere.

### 4.11 `WorktreeEntry` literal copied 7 times: done
- `services/preview/factories.ts` exports `worktreeEntry(overrides)` and
  `threadSummary(overrides)`, used by the fixtures, the preview
  `gitWorktreeAdd` fake and the three worktree tests.

---

## 5. Thread UI (`src/lib/thread`)

All of §5 is done. ThreadView went from 1,499 to 1,079 lines.

### 5.1 Dead code: done
- Removed `groupForTurn` (test-only), `isBranch`, `resetAutoNameState`,
  `ApplyOutcome.changed`, `ApplyOutcome.misalignment` (the turn error still
  carries it), the orphaned and duplicate JSDoc in `threadStream.ts`, and
  FloatingMenu's unused `FileText`. Biome still doesn't see unused imports in
  `.svelte` files.

### 5.2 Extract the goal banner from ThreadView: done
- `GoalBanner.svelte` (Skeleton `Progress` for the budget bar) and
  `goalCommands.ts` (`run`, `set`, `editBudget`, `toggle`, `clear`). Each
  command reads the session once up front, so switching threads mid-request
  can no longer land the result on the other thread. Redundant
  `aria-label`s equal to `label` are gone here and in QueuedMessageRow and
  WorkItem.

### 5.3 Extract the transcript scroll-follow state machine: done
- `transcriptScroll.svelte.ts` (`TranscriptScroll`, wired with `{@attach}`)
  with its own unit tests.

### 5.4 ThreadView boilerplate: done
- `runCommand` is a `Record<SlashCommandId, handler>` plus a
  `DRAFT_COMMANDS` set (`/diff` and `/status` still work on drafts). The
  composer is typed `ReturnType<typeof Composer>`. `workDir`, `repoDir` and
  `costUsd` are derived once. `interrupt`, `compact`, `undoTurns`,
  `failCommand`, `queuedPreview` and 7 of the 12 session aliases are gone.
- `findLast` isn't available (tsconfig's lib is ES2022), so `lastTurnModel`
  uses `[...x].reverse().find()`.

### 5.5 FloatingMenu repeated markup: done
- `section` and `empty` snippets; rows already used `menu-row` (§2.6).

### 5.6 WorkItem: five near-identical Collapsible expanders: done
- `expander`, `liveLine` and `openThreadButton` snippets cover four
  expanders; the reasoning one keeps its own differently styled trigger.
  `AgentActivityRow.svelte` was not worth splitting out.

### 5.7 `AgentRun → SubagentDetail` mapping duplicated: done
- `runAsSubagent(run)` in `services/agentRuns.svelte.ts`.

### 5.8 Status-to-tone classification in four places: done
- `thread/agentStatus.ts` (`statusBucket`, `statusTone`). Where the copies
  disagreed: `killed`, `orphaned` and `interrupted` are now Stopped (grey),
  and unknown statuses read as Finished (grey); WorkItem used to show some of
  these green and FloatingMenu called interrupted agents "Finished".

### 5.9 Queued-message edit duplication: done
- `queuedText` lives in `queueEntries.ts`, `editedParts` in
  `messageText.ts`, and an `InlineEdit` class (`inlineEdit.svelte.ts`) is
  shared by QueuedMessageRow and UserMessageBubble. Both check
  `isComposing` and `preventDefault` Escape. Queued edits keep their chips
  (bug §13.8); the textarea shows only the prose.

### 5.10 Request cards hand-roll `submitState()`: done
- The three cards use `submitState` (§2.7).

### 5.11 ThreadSession operation scaffolding: done
- `#runOperation(kind, body)` and `#maybeIdle()` in `threadSession`.

### 5.12 `threadStream.ts` duplicate switch arms: done
- Message/plan deltas, summary part/delta and output/terminal interaction
  share arms; `padTo` pads summaries.

---

## 6. Composer

All of §6 is done except the `acceleratorFromEvent` move in §6.10, which
belongs with the SettingsView work in §7.1. Composer.svelte went from 1,359 to
825 lines.

### 6.1 `richInput.ts`: six edit verbs share one frame: done
- A private `editUnits(root, handlers, edit)` carries caretOffset → units →
  splice → render → place caret for all six verbs. `RichEditor.take(event,
  parts)` replaces the repeated `preventDefault(); adopt(x); return;` blocks.

### 6.2 Split `Composer.svelte` (1,374 lines): done
- `ComposerSettings` (`composerSettings.svelte.ts`, next to `composerPrefs`):
  prefs, their scoping effects, everything derived from the model list, the
  turn options and every `choose*`/`toggle*`.
- `AttachmentStager` (`attachments.svelte.ts`): staging, retry, native and
  HTML drag/drop, the file picker and paste.
- `ComposerDraft` and `HistoryRecall` (`composerDraft.svelte.ts`): draft
  persistence and the ↑/↓ walk.
- The `IMAGE_EXTENSIONS`/`basename`/`guessKind` copies were already gone
  (§2.2, §2.9).

### 6.3 Composer popovers → Skeleton Popover: done
See §3.2.

### 6.4 Chip construction duplicated three ways: done
- `chipShell`, `buildChip` (also used by `renderParts`), `insertChip`,
  `readChip` and one `CHIP_SELECTOR`. RichEditor's twin insert methods go
  through `insertAtQuery`. `ChipPart` lives in `composerParts.ts`.

### 6.5 Selection boilerplate: done
- `liveRange()`, `caretRangeIn(root)`, `selectCollapsed()` and
  `placeCaretAtEnd()` are exported. `placeCaretBesideChip` is now
  `placeCaretBeside(node)`, with no alias, and `removeMentionChip` is
  `removeChip`.

### 6.6 Caret helpers re-implement each other: done
- `moveCaretVertically` uses `lineEdgeOffset`. `chipBesideCaret` and
  `chipAcrossLineBreak` share `nodeBesideCaret`, and `chipBesideCaret` uses
  `chipContaining`. `Unit`/`Units` are declared at the top.

### 6.7 Dead Claude permission presets: done
- `CLAUDE_PERMISSION_PRESETS`, `permissionPresetsFor`, the preset
  `approvalsReviewer` field and `presetFor`'s harness argument are gone. A
  one-entry `LEGACY_PRESET` map reads `"auto-review"` as `"auto"`.

### 6.8 Composer-local duplication: done
- `togglePolicy` (composerPrefs), `ComposerSettings.modelList` and
  `setPlanMode`, `persist()` everywhere, and `cloneParts`, `plainText` and
  `trimParts` in `composerParts.ts`, each with unit tests.

### 6.9 Dead code and stale comments: done
- The test-only `renderParts` is gone; `renderPartsWith` took its name. The
  test-only `resolveAppSubagents` is gone too.
- `planTurnOptions` and both `ensureHarnessModels` wrappers (Composer's and
  the one in `models.svelte.ts`) are gone. `turnOptions` is the one export.
  `cancelGoalEdit` is no longer exported.
- The doubled `onCommand` JSDoc and the orphaned `isEmpty` one are fixed, and
  so is the comment on `coerce`.
- `composerPrefs.svelte.ts` is now `composerPrefs.ts`.

### 6.10 Small cleanups
- Done: ReviewTargetPicker looks up its stage's mode once and drops the
  `?? ""` (`query` is a required string). AdaptiveToolbar groups its visible
  items once into `shown`.
- Left: `quick/quickChat.ts acceleratorFromEvent` is used only by
  SettingsView. Move it to `utils/accelerator.ts` as part of the §7.1
  SettingsView work.

---

## 7. Layout, panels and app state

### 7.1 SettingsView (876 lines): repeated cards and rows; split it: done
- **Repeated markup:**
  - The card wrapper appears 8 times (369, 493, 564, 654, 680, 725, 733,
    759).
  - The label/description/control row appears about 12 times.
  - There are 4 error cards (the switches are `Toggle` now, §3.3).
- **Proposal:** `settingsCard` and `settingRow` snippets or components,
  Skeleton `Switch`, and a split into `layout/settings/{General, Agents,
  Keyboard, Advanced}.svelte`.
- **Savings:** ~150 lines; the main file drops to ~300. **Risk:** med.

### 7.2 Settings navigation state is mirrored: done
- `App.svelte:72-85, 290-305` syncs a local `settingsOpen` from
  `settingsNav.open` through an effect. SettingsView takes 4 props that
  mirror the store.
- Section ids are typed `string`, the icons live in a separate map, and
  `SettingsSection` names two different types.
- Use `{#if settingsNav.open}` and read the store directly. Put `icon` on
  `SETTINGS_SECTIONS` and type the ids as a union. ~25 lines.

### 7.3 Dead code: done
- `SettingsView.svelte:139-140, 215-217` calls `readHomeOverview()` and
  stores `overview`, which nothing reads. That costs an IPC round-trip every
  time Settings opens (verified).
- An orphaned docstring at `SettingsView.svelte:258`.
- `panels/workspaceSearch.ts`: `totalMatchCount` is used only by its test,
  `emptyStateLabel` is a one-liner, and `debounce` belongs in utils.

### 7.4 HomePage duplication: done
- The 4 usage tiles are written out by hand (244-264) while the tiles above
  use `{#each}`.
- 3 project sections have the same heading, blurb and list (346-396).
- 2 identical cancellable `$effect` fetches (83-111).
- (`iconAction` went in §3.6 and the local `relativeTime` in §2.3.)
- ~80–100 lines.

### 7.5 Sidebar: done
- 5 header TooltipButtons share one 120-char class (703-746). Render them
  with `{#each}`.
- The hover-reveal class is copied 5 times.
- The Collapsible folder wrapper is written twice (535-546 vs 640-651); use
  a `folderNode` snippet.
- `{#if thread.harness === "claude"}` at line 434 (see §12).
- ~50 lines.

### 7.6 SideQuestions vs NativeSideQuestions: done
- The list and the textarea footer are built separately in each
  (`SideQuestions.svelte:322-346, 425-465`;
  `NativeSideQuestions.svelte:218-256`), and their styles have drifted.
- Extract `SideQuestionList.svelte` and `SideComposer.svelte`. ~50 lines.

### 7.7 RightPanel header: done
- A 9-branch if-chain for icon and title (`RightPanel.svelte:148-178`).
  Replace it with a `PANEL_META` table. ~25 lines.

### 7.8 ProjectDetail: done
- "Local chats" and "Thread messages" are byte-identical result blocks
  (467-500). Use a `resultGroup` snippet.
- The Sources tab is mis-indented (504-566). ~30 lines.

### 7.9 `menuAction`: a 130-line if-return chain: done
- `app/actions.svelte.ts:313-446` is 25 sequential `if (action === X)`
  blocks with order-dependent fall-through, and togglePin is the implicit
  default.
- Use a `switch` with exhaustive narrowing, or a
  `Record<MenuAction, handler>`. ~30 lines; med risk.

---

## 8. Feature folders

### 8.1 Split `IntegrationsSection.svelte` (1,100 lines)
- **What:** two inline forms (Add skill 572-615, MCP add/edit 617-731), two
  row types (741-934, 947-1033), the page shell and 30+ `$state` variables.
  Every other feature does create/edit through `openDialog` + `DialogShell` +
  `submitState`.
- **Proposal:** `McpServerDialog`, `AddSkillDialog`, `McpServerRow` and
  `SkillRow`.
- **Savings:** ~120 lines; the main file drops to ~450. **Risk:** med (23
  tests drive the inline forms).

### 8.2 Repeated blocks inside IntegrationsSection
- Resources and Resource-templates lists are identical (891-930).
- The inherit/enabled/disabled `<select>` appears twice (799, 1043).
- So do the "Plugin: …" span and the Confirm/Cancel delete pair.
- The `data.settings[\`mcp:${…}\`]` key is built about 10 times.
- Add `uriList`, `overrideSelect` and `confirmDelete` snippets, plus a
  `setting(kind, id)` accessor. ~55 lines.

### 8.3 Shared `GitErrorNote`
- The GitError block (message, hint, `<pre>` detail) is identical in
  `GitTab.svelte:229-233` and `CommitBox.svelte:68-72`.
- GitTab's `commit()` re-implements `mutate()` with a different error sink;
  give `mutate` an error-target parameter.

### 8.4 Usage stacked bar
- `UsageBreakdownBar.svelte:34-60` and `PromptPartsList.svelte:67-90` render
  the same bar and legend. Make UsageBreakdownBar take generic rows.
  ~25 lines.

### 8.5 Dead and over-exported code
- `review/review.ts`: `fileAnchor` (33) and `reviewEventLabel` (155) have no
  references. `threadsForFile` (118) is used only by its test.
- `git/gitStatusFiles.ts`: `isStagedCode` and `isUnstagedCode` are exported
  but internal.
- `git/sync.ts` is a whole file for one type.
- `git/gitErrors.ts:5-29`: the `KINDS` array repeats the union. Derive it
  with `as const`, and turn `hintFor` into a Record.
- ~45 lines.

### 8.6 Small duplicated helpers
- The branch label in `AddWorktreeDialog.svelte:49-52` duplicates
  `worktrees.ts:122-127`.
- `statusIsDirty` (`gitStatus.svelte.ts:56`) duplicates `isDirty`
  (`worktrees.ts:39`).
- `stripFrontmatter` (`memoriesHelpers.ts:69`) and `splitFrontmatter`
  (`integrationsHelpers.ts:239`) use two different regexes; define one in
  terms of the other.
- ~20 lines.

### 8.7 Move temp-worktree path logic to the backend
- `worktrees/worktrees.ts:25-37` rebuilds `${codexHome}/worktrees-tmp/...`
  with a hard-coded `/`, and detects temp worktrees with
  `path.includes("/worktrees-tmp/")`. It's used in 5 components.
- The Rust side (`projects/worktrees.rs:132-150`) already does both
  host-aware.
- Add `isTemp` to `WorktreeEntry`/`ThreadSummary`, computed in
  `bootstrap.rs`, and have the create command pick the location. ~20 TS
  lines and one source of truth. Also a Host-rule fix (§12).

---

## 9. Rust: harness drivers, threads, agents

### 9.1 Retire Codex compat gates every supported tier now has
- **Where:**
  - `codex/compat.rs:35-80`: seven `Feature`s whose newest `since` is
    0.153.
  - `docs/SUPPORTED_VERSIONS.md`: last stable is 0.159.3 (both verified).
  - `codex/session.rs:392-453` (`unsupported`, `mark_unsupported`,
    `send_gated`).
  - 33 Rust call sites, 23 `*_UNSUPPORTED`/`is*Unsupported` references in
    `src/`, and 6 `expect_legacy` branches in `live_codex/main.rs`.
- **What:** the probe-and-cache machinery, the frontend fallbacks and the
  legacy test branches serve only versions the policy no longer supports.
- **Proposal:** delete the gates whose `since` is at or below last stable.
  Keep `Feature`/`send_gated` as scaffolding for the next upstream API. Keep
  `REVERT_NEEDS_PAGINATED`, which depends on the thread's data age, not the
  CLI version. Consider adding "retire gates below last stable" to
  `versions:check` so this doesn't recur.
- **Savings:** ~200–300 lines across Rust, TS and live tests. **Risk:** med.
  This is a policy call: ADR 0001 says older releases "mostly keep working".

### 9.2 One JSONL child-process core for Codex and Claude
- **Where:** `codex/child.rs:56-409` vs `claude/child.rs:60-406`.
- **What:** about 100 near-identical lines:
  - locked stdin write and flush
  - the stderr-tail thread
  - `Drop`/`kill`/`is_alive`/`mark_dead`
  - the pending-oneshot insert and the `fail_pending` drain
  - reaping and spawn pipe setup

  `claude::child::spawn` is also a pass-through to `spawn_with_app`.
- **Proposal:** `harness/process.rs` with `JsonlProcess<K>`, generic over
  the pending-key type. Each driver keeps only its framing.
- **Savings:** ~100–130 lines. **Risk:** med (concurrency; both sides
  tested).

### 9.3 Decode notifications once; journal and agents consume the typed enum
- **Where:** `codex/journal.rs:42-541` has 17
  `get("threadId"/"turnId")…` chains plus about 24 others, and `observe` is
  a chain of 7 `if method == …` blocks. `agents/supervisor.rs:313-393`
  re-parses the same raw JSON that `events.rs` already decodes into typed
  params.
- **Proposal:** decode first in each sink, then pass `&CodexNotification`
  and `match` on it.
- **Savings:** ~50–70 lines; the journal becomes much easier to read.
  **Risk:** med.

### 9.4 One "journal, trim, emit" helper
- `codex/session.rs:106-118`, `agents/supervisor.rs:266-272` and
  `claude/driver.rs:150-160` all repeat observe + build + emit.
- Only the main session runs `trim_live_params`, so a subagent's or Claude
  thread's large output reaches the webview untrimmed (§13).
- Add `TurnJournal::observe_and_emit`. ~20 lines plus a behaviour fix.

### 9.5 Generate `KNOWN_METHODS` from the enum
- `codex/events.rs:36-151` (enum with 50 renames) and `180-233`
  (`KNOWN_METHODS`) state every wire method twice, and a test exists only to
  keep them in sync.
- A `notifications!{}` macro in the style of the existing `params!` would
  emit both. ~60 lines.

### 9.6 Generic adjacently-tagged decode
- `CodexNotification::decode` and `ServerRequest::decode`
  (`events.rs:156-176, 362-381`) are the same 20 lines. Add
  `decode_tagged<T>`. ~18 lines.

### 9.7 Table-drive the round-trip tests
- `events.rs:503-664, 725-766`: about 50 `round_trips(...)` calls that
  rustfmt spreads over 4 lines each. Loop over a `[(method, json!(…))]`
  array instead. ~80 lines.

### 9.8 `harness_of(thread_id)` instead of 20 hand branches
- `storage::thread_harness(&ctx.database(), &id).await?.is_some()` and
  similar checks appear 20 times across `threads/{lifecycle, turn, queue,
  subagents, autoname, side_questions, read}.rs`.
- Short term: `HomeContext::harness_of(&id)`.
- Longer term: a small `ThreadDriver` enum with `rename`, `archive`,
  `delete`, `interrupt` and `start_turn`, matching the `Driver` CLAUDE.md
  describes (no `Driver` type exists in `harness/` today).
- ~30 lines now, more with the enum.

### 9.9 `spawn_agent`: four identical failure exits
- `agents/supervisor.rs:493-687` is a 195-line function with 4 copies of an
  8-line `finish(…Failed…); return Err` block. It also has 3 hand-rolled
  `.get("thread").and_then(|t| t.get("id"))` lookups.
- Extract `launch()` and handle the error once. Use `pointer("/turn/id")`
  and an `AgentRun::new` constructor. ~40 lines.

### 9.10 Char-boundary truncation, 5 copies
- `codex/journal.rs:637-651`, `codex/wire.rs:96-99`,
  `agents/tools.rs:368-385`, `claude/child.rs:377-381` and
  `git/actions.rs:161-166`. "Keep the tail with a marker" appears twice.
- Add `util::text::{floor_boundary, head, tail}`. ~35 lines.

### 9.11 Hand-rolled base64 encoder
- `claude/driver.rs:1242-1266` plus its test at 1411 (verified). `base64` is
  already in Cargo.lock. Add it as a direct dependency. ~35 lines.

### 9.12 Projector and translator chunk duplication
- `harness/project.rs:303-375`: the `AgentMessageChunk` and
  `AgentThoughtChunk` arms are duplicated. `project()` is one 295-line match,
  and the terminal-state check disagrees with itself
  (`Completed|Failed` at :396, `Completed|Failed|Cancelled` at :460).
- `claude/translate.rs:157-264`: the text and thinking arms are duplicated in
  start, delta and stop.
- Add `stream_chunk`, `ToolCallStatus::is_terminal()`, per-arm methods and
  `BlockKind::chunk`. ~70 lines.

### 9.13 Use the existing `util::json` helpers on the Codex side
- `str_at`/`i64_at`/`arr_at` are used more than 100 times in `claude/*` and
  `threads/*`, but never in `codex/journal.rs` (24), `rollout.rs` (6),
  `agents/supervisor.rs` (5+), `wire.rs` (3) or `claude/child.rs`/
  `codex/child.rs` (6). There are also ~48 multi-line chains. ~50 lines
  (partly moot after §9.3).

### 9.14 Split `claude/driver.rs` (1,415 lines)
- Move runtime discovery, status and the model list (369-593) to
  `claude/runtime.rs`, and content/image encoding to `claude/content.rs`.
- `missing_claude_message` builds its text by `.replace()`-ing words in
  Codex's message. Give `binary::missing_message_on` a product-name
  parameter instead.
- `deny_pending(&self, _reason)` ignores its parameter at all 5 call sites.
- The settings merge and the mode + `exit_plan_mode` derivation are repeated
  2–3 times. Add `Settings::merged_over` and `Settings::set_mode`.
- ~30 lines and a large readability gain.

### 9.15 `threads/turn.rs`: type the harness and merge the two start paths
- `harness: Option<String>` is compared with `"claude"` twice (153-170), and
  `storage::record_harness_thread(.., "claude", ..)` is stringly typed.
- The resolved-settings extraction is duplicated across the Claude and Codex
  paths (234-341).
- Take `Option<HarnessKind>` and add `record_resolved(...)`. ~25 lines; run
  typegen.

### 9.16 Three ways to send a Codex request
- `codex/child.rs:118-157` and `codex/session.rs:312-348` both offer
  `request`, `send` and `try_request`/`request_served`. Some callers
  destructure a `Request` back into method and params.
- `thread/name/set` and `thread/fork` have no builder in `requests.rs`. Raw
  method strings appear in `agents/mod.rs:135`, `codex/pairing.rs` and
  `threads/lifecycle.rs`.
- Expose only `send`/`try_send(Request)` and add the missing builders.
  ~25 lines.

### 9.17 Lock, option and timestamp helpers
- `.lock().map_err(|_| "… lock was poisoned".to_string())?` appears 41 times
  across the crate.
- "Entry or `{}`, then coerce to an object" appears twice in
  `codex/session.rs`.
- `session.rs:128-132` reimplements `util::time::unix_secs`.
- About 10 `Mutex<Option<T>>` get/set sites in `agents/supervisor.rs`.
- Add `util::sync::lock(&m, what)`, `cached_object` and a `Slot<T>`.
  ~40 lines.

---

## 10. Rust: commands, storage and services

### 10.1 `HomeWindow::ctx()` instead of `State<AppState>` everywhere
- **Where:** `home_window.rs:19-67` and `lib.rs:319`. `HomeWindow` already
  holds the resolved context, and `ContextWindow for HomeWindow` ignores
  `state`. `state.ctx(&window)` appears 173 times (verified).
- **What:** 144 of the 149 commands that take both `HomeWindow` and `State`
  use `state` only for this lookup. Each carries an extra parameter line,
  two imports and a lookup that does nothing.
- **Proposal:** add `HomeWindow::ctx(&self) -> Arc<HomeContext>`, drop the
  param and call `window.ctx()`. `State` isn't in the bindings, so
  `bindings.ts` doesn't change.
- **Savings:** ~150–250 lines. **Risk:** low, mechanical.

### 10.2 Storage connection and transaction boilerplate
- **What:** `let connection = db::conn(database)?;` appears 135 times
  (verified). The 18 transactions each spend about 5 lines on
  `.unchecked_transaction().await.map_err(db::db_error)?` …
  `.commit().await.map_err(db::db_error)`.
- **Proposal:** make `db::{exec, rows, one, exists}` take an `impl Executor`
  implemented for `&Database` and `&Connection`. Add `db::begin` and
  `db::commit`, or `db::in_transaction(database, async |tx| …)`.
- **Savings:** ~190–220 lines. **Risk:** low for conn, med for the closure
  form (lifetimes).

### 10.3 `git/commands.rs`: use the `locked()` it already has
- **Where:** 10 copies of
  `spawn_blocking(...).await.map_err(|_| "Git inspection failed")`
  (37-98, 301-355, 420-422, 558-570).
- **What:** the common-dir lock is inlined in `worktree_add`, `remove`,
  `prune`, `lock`, `unlock` and `handoff` (116-388), while `locked()` at
  396-410 does exactly this. That boilerplate is about 30% of the file's 865
  lines.
- **Proposal:** route the six mutations through `locked()` and add
  `inspect()`.
- **Savings:** ~110–130 lines. **Risk:** low.

### 10.4 One `blocking()` helper for 39 `spawn_blocking` sites
- **Where:** review/commands.rs (8), usage, workspaces (`materialize_hub` is
  duplicated 3 times), files, sources, profiles, projects, memories.
- **What:** each site spends 3–4 lines on spawn_blocking + `map_err`, and
  the `?`/`??` flattening is inconsistent.
- **Proposal:** `util::blocking(what, f) -> Result<T, String>`, which also
  replaces `inspect()` from §10.3.
- **Savings:** ~60–80 lines. **Risk:** low.

### 10.5 `CommandOutput::require` for 32 `if !output.ok` checks
- **Where:** git/actions.rs (10), git/commands.rs (5), review/actions.rs (4),
  review/queries.rs (3), projects/worktrees.rs (2) and others.
- **Duplicate runners:** `git/run.rs:66-89 run_git_network` rebuilds the
  `-C dir` argv, and `review/gh.rs:39-78` has near-identical `run_gh` and
  `run_gh_with_input`.
- **Proposal:** add `run_git(...)?.require(|o| redact_classified("…", o))?`
  plus one shared runner per tool.
- **Savings:** ~70–90 lines. **Risk:** low.

### 10.6 usage DTOs mirror storage rows field for field
- `usage/commands.rs:25-95, 148-167, 209-237` vs
  `storage/turn_usage.rs:22-142`:
  - `CategoryTokensDto` is identical to `CategoryTokens`.
  - `UsageScopeArg` mirrors `UsageScope`.
  - `ModelUsage` and `ThreadUsageSummary` mirror their row types.
- Derive `Serialize, specta::Type` on the storage structs and return them
  directly. Keep only `UsageTokensDto`.
- **Savings:** ~70–90 lines. **Risk:** low–med (typegen).

### 10.7 MCP server summaries implemented twice
- `integrations/config_doc.rs:224-280` (toml_edit) vs
  `integrations/scope.rs:79-114` (JSON). The only production caller of the
  first uses it as a name-exists check.
- `config_doc::edit` is a one-line pass-through.
- Replace the check with a direct lookup, delete `summarize_mcp_servers`,
  and point its tests at `mcp_summaries`. ~60 lines.

### 10.8 `bootstrap.rs` defaults
- Production code builds 3 identical
  `StoredProject { path, name: None, pinned: false, archived: false,
  worktree: false }` literals (404-437).
- The tests (654-1222) hold 7 `BootstrapExtras` literals (14 fields each),
  13 `StoredProject` and 7 `RuntimeConfig`. There are about 20 more
  `RuntimeConfig` literals crate-wide.
- Derive `Default`, add `StoredProject::bare(path)` and a test-only
  `RuntimeConfig::native(home)`. ~150 lines.

### 10.9 `Host::settle` for three copies of host settling + tilde expansion
- `profiles.rs:467-473`, `settings/commands.rs:23-49 settle_home` and
  `handoff/deeplink.rs:16-24`.
- `settle_home` hand-rolls WSL `~` expansion even though
  `Host::expand_tilde` exists. It also passes `rest` with its leading `/`
  into `join_str`, the pitfall `expand_tilde`'s comment warns about (§13).
- ~25 lines.

### 10.10 Split `lib.rs` (1,083 lines)
- Move `RuntimeConfig`/`HomeContext`/`AppState` (88-500) into `state.rs`.
- `forget_context` and `release_if_orphaned` repeat the same check.
- Startup prep in `run()` (822-826) repeats `ensure_context` (418-419).
- Extract `AppState::in_use` and `prepare_home`. lib.rs drops to ~650
  lines.

### 10.11 `write_bindings` hand-parses generated TypeScript
- `lib.rs:761-811` runs about 50 lines of brace-depth parsing over
  `bindings.ts` to build `commandArguments`, plus string `.replace` patches.
  It breaks if tauri-specta changes its output format.
- Build the map from specta command metadata, or from a `macro_rules!` that
  wraps the registration list. ~30–40 lines; med risk (check the rc.25 API).

### 10.12 `os.rs`
- The spawn/status/check sequence is repeated 3 times, and the opener is
  built in 3 per-OS `cfg` blocks. Add `run_status` and `opener()`.
  ~20 lines.

### 10.13 Minor
- `util/host.rs:494-503`: the two WSL binary caches (`binary_cache`,
  `binary_paths`) could be one map. ~10 lines.

---

## 11. Tests

### 11.1 Frontend fixture factories
- **What:** `Project` literals appear in 10 files, `ThreadSummary` in 7
  (16 fields each), `BootstrapData` in 6 (12–43 lines each), and
  `WorktreeEntry` in 3 test files plus fixtures (done in §4.11:
  `services/preview/factories.ts` has `worktreeEntry` and `threadSummary`). Adding a field to
  a bindings type means editing about 10 files.
- **Proposal:** `src/lib/testing/factories.ts` with `makeProject`,
  `makeThreadSummary`, `makeBootstrap` and `worktreeEntry(overrides)`, shared
  with `preview/fixtures.ts` where the shapes match.
- **Savings:** ~250–300 lines. **Risk:** low.

### 11.2 Mock-reset boilerplate
- There are 81 `mockReset()` calls across 10 `vi.hoisted` files.
  ThreadView.test.ts alone has 14 describe-level `beforeEach` blocks
  repeating the same resets, 52 `mockReset()` calls and 34 copies of
  `render(ThreadView, { threadId: "thread-1", cwd: "/projects/example" })`.
- **Proposal:** set `mockReset: true` in `vitest.config.ts`, give the hoisted
  `vi.fn` default implementations (Vitest 4 restores them), and add
  `renderThread(...)`.
- **Savings:** ~150–200 lines. **Risk:** med (tests that seed state in
  `beforeAll`).

### 11.3 Shared thread test helpers
- `detail()`, `settle`, `deferred` and `emit` are duplicated across
  ThreadView.test.ts, threadSession.test.ts, threadStream.test.ts and
  threadQueue.test.ts. The `listModels` and `listHarnessModels` fixtures are
  identical. Move them to `src/lib/testing/threads.ts` next to
  `codexEvents.ts`.

### 11.4 Composer test helpers
- `richInput.test.ts` defines the same `select` helper 4 times.
  `readyAttachment` and `mention` are duplicated across 3 files.
- `Composer.test.ts:100-118` re-declares the component's props by hand; type
  it as `Partial<ComponentProps<typeof Composer>>`.
- The `insertLineBreak`/`deleteLineBreak`/`deleteToLineEdge` suites in
  richInput.test largely repeat richEditor.test's key-press cases. Keep only
  the DOM-shape cases.
- ~100–150 lines.

### 11.5 IntegrationsSection.test.ts
- 14 `vi.fn()` consts plus 14 pass-through lambdas (6-36). Switch to the
  `vi.hoisted({...})` pattern MemoriesView.test already uses.
- 21 `userEvent.setup()` + `render` pairs and 15
  `.closest("[id^='mcp-row-']")` lookups. Add `setup` and `mcpRow` helpers.
- A stray top-level `it` at :41. ~60 lines.

### 11.6 Sidebar test setup
- `Sidebar.svelte.test.ts:58-91 setup` takes 4 positional parameters, and
  line 574 calls `render(Sidebar)` separately. Use one options object
  together with §11.1.

### 11.7 Playwright preview helpers
- `loadPreview` is copied into 6 specs in 3 strictness variants.
  `selectPreviewThread` is copied 3 times plus inline. `demo/shoot.ts`
  already exports the same `open`/`openDemoThread`/`closeOverview`.
- Rename `tests/browser/turnSettings.ts` to `tests/browser/preview.ts` and
  export them all. The two near-identical selection evaluators in
  `composer-rich-input.spec.ts:16-61` can merge.
- ~60–80 lines.

### 11.8 Live suites: shared NDJSON process bus
- `live_codex/harness.rs` and `live_claude/harness.rs` each hand-roll:
  - the stdout reader thread + Condvar and the stderr ring buffer
  - `wait_for`, `write`, `home_dir()`
  - the env gate, the `live!` macro and the home-source lookup
- Add `src-tauri/tests/common/ndjson_bus.rs`, included in both through
  `#[path]`. ~120–150 lines; med risk (re-verifying costs quota).

### 11.9 Rust unit-test fixtures
- `async fn database()` is defined separately in 7 modules, and there are
  about 45 tempdir + `storage::open` pairs.
- `fn git()` is identical in `git/commands.rs:584-600` and
  `projects/worktrees.rs:316-330`.
- Add `#[cfg(test)] storage::test_db()` and `git::test_support`.
  ~60–80 lines.

---

## 12. Rule violations found along the way

These break CLAUDE.md rules. Fixing them is part of the refactor, not extra
scope.

| Rule | Where |
|---|---|
| Frontend never checks harness kind | `ThreadView.svelte:718, 1437`; `HarnessMenu.svelte:50`; hard-coded "Codex" in `Composer.svelte:1209-1210, 1237` and the TypingDots default label |
| Hosts: no hand-built Linux paths | `worktrees/worktrees.ts:25-37` (§8.7); `review/gh.rs:44, 65` guesses the host with `Host::from_local` instead of using the Home's host (§13) |
| Stringly-typed harness in Rust | `threads/turn.rs:153-170, 206` (§9.15) |

---

## 13. Bugs found along the way

Reviewers spotted these while looking for bloat. They are latent defects,
listed so they aren't lost.

1. **Memories render unstyled markdown.** `.prose-thread` is scoped to
   WorkItem, so MemoriesView's `class="prose-thread"` does nothing (§2.5).
2. **Unarchive does nothing in preview mode.** The preview
   `unarchiveThread` (`api.ts:750`) splices `previewArchived`, but the UI
   reads `previewArchivedThreads` (§4.3). Fixed: unarchive moves the
   thread back into its project and the search index.
3. **gh runs on the wrong host for WSL Homes.** `review/gh.rs` derives the
   host from a Linux path with `Host::from_local`, which gives `Native`
   (§10.9, §12).
4. **Subagent and Claude live events skip `trim_live_params`,** so large
   tool output reaches the webview untrimmed (§9.4).
5. **`settle_home` passes `rest` with its leading `/` into `join_str`.**
   That is the pitfall `Host::expand_tilde` documents (§10.9).
6. **The projector's terminal-status check is inconsistent.**
   `Cancelled` counts as terminal at `project.rs:460` but not at :396
   (§9.12).
7. **Settings opens with a wasted IPC call.** `readHomeOverview()` runs on
   every open and its result is unused (§7.3). Fixed: the call is gone, and
   each Settings section now loads its own data when shown.
8. **Queued-message edits flatten chips to `@name`,** while bubble edits
   preserve them (§5.9). Fixed with the shared `InlineEdit`.

Carried over from the cleanup log's "Found, not yet fixed" list:

9. **Session focus and `clearThread` compare ids of different kinds.** They
   check the sidebar's root thread ids against `view.threadId`, which is a
   version (leaf) id.
10. **SidebarSearch mixes queries.** "Load more" within the debounce window
    mixes queries, and its timer isn't cleared on unmount.
11. **Possibly incomplete Rust fixes:** caching rollout-lookup misses,
    pruning per-thread rows on delete, and clamping agent limits in
    `prefs::write_agent_settings`.
12. **Unreviewed Rust fixes from a stopped agent.** They were committed in
    `13898d2` and pass `rust:preflight`, but nobody has read them line by
    line. They touch `connections/commands.rs` (revoke result),
    `usage/ledger.rs` + `harness/project.rs` (Claude per-process totals),
    `review/queries.rs` (PR page merging, git option-injection guard),
    `storage/search.rs`, `storage/projects.rs` (`update_store` lock) and
    `projects/commands.rs`, `profiles.rs` and `threads/lifecycle.rs`.

---

## 14. Suggested order

1. **Free and safe.** The rest of repo hygiene (§1.3, §1.6–1.8) and the dead-code deletions
   (§4.3, §4.10, §5.1, §6.7, §6.9, §7.3, §8.5). Most of the byte savings,
   almost no risk.
2. **Helpers first, then sweeps.** §2 is done. Still to add: the test
   factories (§11.1). Many later items get shorter now that the helpers
   exist.
3. **Rust mechanical wins.** `HomeWindow::ctx()`, `db` executor, `locked()`
   / `blocking()` / `require()`, `Default` derives (§10.1–10.5, §10.8).
   All low risk, compile-checked, and about 700 lines together.
4. ~~**api.ts restructuring** (§4.1, §4.2, §4.4, §4.5).~~ Done.
5. ~~**Skeleton swaps** (§3).~~ Done.
6. **Large component splits:** SettingsView, IntegrationsSection (§7.1,
   §8.1). ThreadView (§5.2, §5.3) and Composer (§6.2) are done.
7. **Driver-level Rust:** `JsonlProcess`, typed journal, `ThreadDriver`
   (§9.2, §9.3, §9.8).
8. **Policy call:** retiring the compat gates (§9.1). Decide the ADR 0001
   wording first.

Verify each step with the cheapest rung that proves it (see the `verify`
skill): `deno task check` / `cargo check`, then `test:changed` /
`rust:test:lib`, then `preflight` once per batch.

---

## 15. Checked and left alone

- **The lib.rs command list** (`:518-733`) is already grouped by module.
  `collect_commands!` needs one flat invocation.
- **One crate-wide error type.** There are 335 `Result<_, String>`, but
  errors already funnel through `db_error`, `redact_*`, `RunError` and
  `StageError`, and the frontend parses string prefixes. A crate error type
  would touch every signature for little gain.
- **Processes** all spawn through `host.command`/`process::run`. The only
  `Command::new` calls are local desktop openers and test helpers.
- **Fixtures:** every file in `tests/fixtures/protocol/**` is referenced.
- **Dependencies:** every Cargo and runtime JS dependency is used. That
  includes `tempfile` (`util/fs.rs:24`) and both `toml` and `toml_edit`.
- **`richInput.ts` vs `richEditor.svelte.ts`** don't duplicate each other in
  substance: one is the stateless DOM layer, the other the stateful
  controller. Only the overlaps in §6.1, §6.4 and §6.5 are worth fixing.
- **A shared `$lib/bindings` mock** wouldn't help. Only one test file mocks
  bindings; the seam is `$lib/services/api`, mocked in 21 files with small,
  targeted mocks.
- **Pricing estimation** lives only in the frontend; the backend stores only
  the reported cost, so there's no duplication.
- **All `*_UNSUPPORTED` prefixes** still match a live backend gate (§4.8).
  Whether those gates should exist at all is §9.1.
