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
| 2 | Move preview fakes out of `api.ts` behind the `commands` proxy | §4.1 | ~800 lines from api.ts (~450 net) | med |
| 3 | Delete ~71 pass-through wrappers in `api.ts` | §4.2 | ~250 lines | med |
| 4 | `HomeWindow::ctx()` so 144 commands drop their `State` param | §10.1 | ~150–250 lines | low |
| 5 | `db::conn` / transaction boilerplate in storage | §10.2 | ~190–220 lines | low |
| 6 | Shared frontend test factories (`makeProject`, `makeThreadSummary`, `makeBootstrap`, `worktreeEntry`) | §11.1 | ~250–300 lines | low |
| 7 | Retire Codex compat gates older than the last-stable tier | §9.1 | ~200–300 lines | med (policy call) |
| 8 | Deduplicate `.agents/skills` vs `.claude/skills` | §1.3 | ~205 KB, 51 files | low |
| 9 | `errorMessage(cause)` helper (109 copies) | §2.1 | ~4 KB, many 3→1-line handlers | low |
| 10 | Tailwind `@utility` classes for repeated class strings | §2.6 | ~10 KB of markup | low |
| 11 | git/commands.rs: use the existing `locked()` and add `blocking()` | §10.3, §10.4 | ~170–210 lines | low |
| 12 | Split ThreadView (goal banner, scroll follow) | §5.2, §5.3 | ~330 lines out of the 1,489-line file | med |
| 13 | Split Composer (attachments, draft, settings) and use Skeleton Popover | §6.2, §6.3 | Composer ~1,374 → ~600 lines; ~130 removed | med |
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

### 2.1 `errorMessage(cause)`: 109 inline copies
- **Done:** `src/lib/utils/errors.ts`; ThreadView has `fail(cause)`.
- **What:** `cause instanceof Error ? cause.message : String(cause)` appears
  109 times under `src/` (verified). That includes 16 in ThreadView.svelte,
  7 in threadSession.svelte.ts, 7 in SettingsView.svelte and 5 in
  Connections.svelte. `MemoriesView.svelte:66` has its own local `message()`,
  and `api.ts:834-901` repeats it inside 5 `is*Unsupported` predicates.
- **Proposal:** `export function errorMessage(cause: unknown): string` in
  `$lib/utils`, used everywhere, including `submitState` and
  `gitErrors.ts:32`. In ThreadView, also add
  `fail = (c) => (session.streamError = errorMessage(c))`, which covers about
  10 sites.
- **Savings:** ~4 KB; many catch blocks become one line. **Risk:** low.

### 2.2 `basename` / `folderName`: about 14 copies with four behaviours
- **Done:** `src/lib/utils/path.ts`. `folderName` is gone; `shortHomeName`
  and `dirName` wrap `basename`. `appData.directoryKey` keeps its own
  normalisation.
- **Where:** `Composer.svelte:820`, `panels/ChangesPanel.svelte:97`,
  `utils/codeLanguage.ts:110`, `utils/mentions.ts:12`,
  `workspaces/CreateWorkspaceDialog.svelte:48`, `thread/messageText.ts:14`,
  `thread/UserMessageBubble.svelte:108`, `thread/FloatingMenu.svelte:96`,
  `git/StatusFileList.svelte:36`, `services/preview/attachments.ts:22`,
  `worktrees/worktrees.ts:4`, `handoff.ts:13,21`, `ThreadImage.svelte:46`,
  `ProjectDetail.svelte:251`, `SidebarSearch.svelte:54`. A separate
  `.replace(/\/+$/, "")` trailing-slash trim appears 17 times.
- **What:** the copies disagree on edge cases. Only two handle `\`, and only
  some strip trailing slashes. That matters for Windows-native Homes.
- **Proposal:** `src/lib/utils/path.ts` with `basename(path)` (both
  separators, trailing separators trimmed) and `trimSlash`.
- **Savings:** ~25–40 lines, plus consistent Windows behaviour. **Risk:** low.

### 2.3 Formatters: bytes, tokens, relative time
- **Done:** `src/lib/utils/format.ts` (`utils/time.ts` removed). Compact
  tokens use `k`/`M`, whole numbers from 100 up. `formatAge` and
  `lastSeenLabel` stay as thin wrappers over `relativeTime`.
- **Tokens:** `thread/goalBudget.ts:24 formatTokens` and
  `thread/contextUsage.ts:70 formatTokensShort` are near-identical (`k` vs
  `K`), and `contextUsage.ts:65 formatTokens` is a *different* function with
  the same name. `trimZero` exists in both files.
- **Bytes:** `composer/composerParts.ts:59 formatSize` and
  `memories/memoriesHelpers.ts:78 formatBytes` differ only in KB rounding.
  There's also inline KB math in `FilePreviewDialog.svelte:66` and
  `GitTab.svelte:288`.
- **Time:** `utils/time.ts relativeTime` has verbatim copies with an " ago"
  suffix in `HomePicker.svelte:147` and `HomePage.svelte:115`, plus separate
  ladders in `memoriesHelpers.ts:85 formatAge` and
  `layout/connectionState.ts:54-56`.
- **Proposal:** `src/lib/utils/format.ts` with `formatBytes`,
  `formatTokens`/`formatTokensShort` (one suffix case) and
  `relativeTime(ts, { suffix, granularity })`.
- **Savings:** ~60 lines; the name clash goes away. **Risk:** low (some test
  strings change case).

### 2.4 `escapeHtml` and the dark code-block styles
- **Where:** `escapeHtml` appears in `CodeView.svelte:68`,
  `ChangesPanel.svelte:117`, `GitTab.svelte:123` and `utils/markdown.ts:49`
  (private there). An identical `background:#0d1117; color:#e6edf3;
  :global(.hljs){…}` block appears in `CodeView.svelte:111`,
  `DiffBlock.svelte:53`, `ChangesPanel.svelte:217`, `RightPanel.svelte:310`,
  `WorkItem.svelte:696`, `GitTab.svelte:299` and
  `IntegrationsSection.svelte:1094`. GitTab and ChangesPanel also copy the
  same `HIGHLIGHT_BYTES` escape-or-highlight logic and the `<pre><code
  class="hljs">` markup.
- **Proposal:** export `escapeHtml` and `highlightDiff(patch, maxBytes)` from
  `utils/markdown.ts`, add a global `.code-dark` class in `app.css`, and
  optionally add a `DiffPre.svelte` for GitTab and ChangesPanel.
- **Savings:** ~100 lines. **Risk:** low.

### 2.5 Global markdown prose styles (also fixes a bug)
- **Where:** `.prose-thread` lives in a component-scoped `<style>` in
  `thread/WorkItem.svelte:626-717`. `memories/MemoriesView.svelte` uses
  `class="prose-thread"` but gets none of the styles. `.prose-skill` in
  `IntegrationsSection.svelte:1074-1100` is a private workaround for the same
  gap, and `.prose-reasoning :global(p)` is duplicated in
  `ReasoningBlock.svelte:27`.
- **Proposal:** one global `.prose-md` in `app.css` with a compact modifier,
  used by WorkItem, MemoriesView, the Integrations SKILL.md preview and
  ReasoningBlock.
- **Savings:** ~120 lines out of components; fixes memory rendering.
  **Risk:** low (check visually).

### 2.6 Tailwind `@utility` classes for repeated class strings
Long class recipes are pasted dozens of times and have drifted apart.
`src/app.css` already uses Tailwind v4, so `@utility` fits.

| Recipe | Copies | Suggested name |
|---|---|---|
| `text-[10px\|11px] font-semibold uppercase tracking-[0.08em] text-surface-500` | 41 exact (26 at 11px, 15 at 10px), more with extra classes mixed in | `eyebrow` |
| `card preset-tonal-error … px-3 py-2 text-xs` (5 variants) | ~27 | `error-note` (or an `ErrorNote` component) |
| `rounded-xl border border-surface-200-800 bg-surface-100-900` | ~25 | `panel-card` |
| `border-dashed border-surface-300-700 …` empty-state CTA | ~15 | `empty-state` |
| Popover card `card … border … bg-surface-50-950 … shadow-xl` | ~13 | `popover-card` |
| `flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-xs hover:preset-tonal` | ~17 (FloatingMenu 12, ThreadHeader 4, …) | `menu-row` |
| Pill trigger `inline-flex items-center gap-1.5 rounded-full px-2 py-1 text-[11px] …` | 8 | `pill`, `pill-active` |
| Hand-rolled input `rounded-md border border-surface-300-700 bg-surface-50-950 px-2.5 py-1.5 … focus:border-primary-500` | 10 | use Skeleton `input`/`textarea` instead (§3.6) |
| Tooltip content class | 2 (TooltipButton, TooltipAnchor) | `tooltip-bubble` |

- **Savings:** ~10 KB of markup; restyling becomes a one-place edit.
  **Risk:** low, mechanical.

### 2.7 Small rune helpers repeated across components
- **Copy-feedback flash:** `copied` + timer + `setTimeout(…, 1500)` is
  written 4 times (`WorkItem.svelte:66-89` twice,
  `UserMessageBubble.svelte:47-55`, `ThreadHeader.svelte:88-89`). Add
  `flashFlag(ms)` returning `{ on, trigger() }`.
- **"Latest request wins":** `const mine = ++gen; … if (mine === gen)` is
  hand-written 6 ways: `IntegrationsSection.svelte:124-176` (twice),
  `ReviewView.svelte:104-147`, `UsageSummary.svelte:32-47`,
  `GitTab.svelte:98-114` and `ChangesPanel.svelte:80-93`. Three dialogs
  share the same onMount loader with `loaded`/`loadError`
  (`AddWorktreeDialog.svelte:33-44`, `CreateWorktreeDialog.svelte:41-50`,
  `BranchSwitcherDialog.svelte:35-46`). Add a `resource(fetcher)` rune
  returning `{ value, error, loading, reload }` with the guard built in.
- **Busy/error action state:** `submitState()` in `app/dialogs.svelte.ts`
  already does busy + error + run, but only dialogs use it. ApprovalCard,
  QuestionCard and ElicitationCard each hand-roll it (§5.10), and so do
  IntegrationsSection (7 handlers), ReviewView, GitTab's `commit()` and
  `Worktrees.svelte:104 runAction`. Move it to
  `$lib/utils/actionState.svelte.ts` and use it in views as well.
- **Savings:** ~150 lines across these. **Risk:** low–med; the guards fix
  races, so keep their tests green.

### 2.8 A generic `ConfirmDialog`
- **Where:** `DiscardQueuedDialog.svelte`, `ReplaceGoalDialog.svelte`,
  `layout/DeleteThreadDialog.svelte`, `git/DiscardDialog.svelte` (and
  `RewindThreadDialog`, which is dead, see §5.1).
- **What:** each is ~25–30 lines of the same DialogShell + text +
  Cancel/Confirm.
- **Proposal:** `components/ConfirmDialog.svelte` taking
  `{ title, body, confirmLabel, tone }`.
- **Savings:** ~60 lines, 3–4 files. **Risk:** low.

### 2.9 Shared image-extension table
- **Where:** `src-tauri/src/composer/attachments.rs:113-124`,
  `src-tauri/src/files/preview.rs:162-171`, `Composer.svelte:819-825`
  (`guessKind`) and `services/preview/attachments.ts:10-19`. The lists are
  already out of sync (heic vs avif/ico).
- **Proposal:** one `util::media::IMAGE_EXTENSIONS` in Rust and one exported
  `guessKind` in TS. Better still, let the stage command return the
  placeholder kind.
- **Savings:** ~15 lines. **Risk:** low.

---

## 3. Skeleton components replacing hand-rolled UI

CLAUDE.md asks for Skeleton first. `composer/HarnessMenu.svelte` already shows
the right `Menu` pattern, and `Collapsible` is already used 8 times.

### 3.1 Menus
- `layout/SidebarContextMenu.svelte`: 27 copy-pasted `<button
  role="menuitem">` blocks with an identical 100-char class string, plus its
  own backdrop, Escape handler and fixed positioning. That is why
  `Sidebar.svelte:342-367` guesses menu heights (80/110/330/380).
  - Quick win: render a `{action, icon, label, danger, show}[]` list with
    `{#each}`.
  - Full fix: Skeleton `Menu` with `Menu.ContextTrigger`, which removes the
    backdrop, the Escape handling and the height guessing.
  - ~150–200 lines.
- `thread/FloatingMenu.svelte:171-232` ("Open in") and
  `thread/ThreadHeader.svelte:133-200` (Handoff): `svelte:window` click and
  Escape capture plus manual a11y. ~50 lines. Check that Escape inside a
  Menu still doesn't reach the composer's interrupt handler.
- `worktrees/Worktrees.svelte:294-336`: popup with a fixed-backdrop button.
- `quick/QuickChat.svelte:160-200`: project menu with manual close-on-click.

### 3.2 Popovers in the composer
- **Where:** `ModelPopover.svelte:38-60`, `PermissionsPopover.svelte:30-51`,
  `SubagentPolicyPopover.svelte:55-76`, `positionComposerPopover.ts` (whole
  file), `Composer.svelte:398, 512-515, 1049-1050`,
  `AdaptiveToolbar.svelte:75-141`.
- **What:** each re-implements stopPropagation, card chrome, outside-click
  dismissal and fixed positioning, with `svelte-ignore a11y` comments. Their
  `absolute bottom-8 left-0 z-50` classes are dead because
  `positionComposerPopover` overwrites them.
- **Proposal:** one `ComposerPopover.svelte` on Skeleton `Popover`, with
  controlled `open` so `/model` and `/permissions` can still open it from
  code. Then delete `positionComposerPopover.ts`.
- **Savings:** ~100–130 lines. **Risk:** med; re-test the More-overflow focus.

### 3.3 Switches
- Hand-built `role="switch"` toggles in `SettingsView.svelte:539-556` and
  `771-787`, and in `MemoriesView.svelte:196-206`. Use Skeleton `Switch`.
  `SettingsView.test.ts` queries `role="switch"`, so check what Skeleton
  renders.

### 3.4 Collapsibles / disclosures
- `layout/ArchivedThreadsSection.svelte:18-73` and `HomePage.svelte:399-408`
  (a boolean, a rotated chevron, `{#if}` and no `aria-expanded`).
- `IntegrationsSection.svelte:809-842` (`expanded[...]` map) and `979-1012`
  (`skillOpen[...]` map). Use Collapsible's `open` prop for the
  focus-scroll case.
- `WorkItem.svelte`: five near-identical Collapsible expanders (§5.6).

### 3.5 Segmented controls / tabs
- Five hand-rolled controls: `IntegrationsSection.svelte:542-556` (filter)
  and `636-647` (transport), `CreateWorktreeDialog.svelte:~143-158`,
  `BranchSwitcherDialog.svelte:~107-110` and `UsageSummary.svelte:~98-110`.
  ThreadView has 3 more.
- Wrap Skeleton `SegmentedControl` in a `components/Segmented.svelte`.
  ~50 lines. Tests query `role="tab"`/`aria-pressed`.

### 3.6 Inputs, tooltips, progress
- 10 hand-styled inputs and 4 hand-styled textareas in git/, worktrees/ and
  review/. Skeleton's `input`/`select`/`textarea` classes are already used
  next door.
- `IntegrationsSection.svelte` repeats
  `autocomplete="off" autocorrect="off" autocapitalize="off"
  spellcheck={false}` 8 times. Spread a `codeInput` const.
- `HomePage.svelte:123-146` `iconAction` rebuilds TooltipButton by hand and
  picks its icon through a 10-way `label.startsWith("Pin")` chain. Pass the
  icon in.
- `TooltipButton.svelte` and `TooltipAnchor.svelte` are near-copies. Render
  one through the other.
- ContextMeter's hover card (`ContextMeter.svelte:18-80`, 4 handlers) can be
  `TooltipAnchor`.
- The goal budget bar is a hand-rolled progressbar; Skeleton has `Progress`.
- FloatingMenu's process filter chips (445-459) can use Skeleton
  `ToggleGroup`.

### 3.7 Shared composite components
- `components/DirectoryField.svelte` (path input + `isTauri()`-gated Browse)
  for AddWorktreeDialog, CreateWorktreeDialog and AddProjectDialog.
  CreateWorktreeDialog imports Tauri's `open` as `openDialog`, which collides
  with `$lib/app/dialogs.openDialog`. ~35 lines.
- `HostSelect.svelte`: the native/WSL `<select>` is built 5 times
  (`SettingsView.svelte:417-454` twice, `AddProjectDialog.svelte`,
  `ProfileHomes.svelte:241-246`, `HomePicker.svelte:298-308`) with
  inconsistent wording ("This PC" vs "This computer"). `listWslDistros()` is
  fetched 4 times even though `launch.distros` already holds the list.
  ~50 lines and 3 IPC calls.
- `BranchList.svelte`: `BranchSwitcherDialog.svelte:113-158` and
  `CreateWorktreeDialog.svelte:196-245` duplicate the filter + listbox +
  states + badges. ~40 lines; med risk (remote-branch and keyboard
  differences).

---

## 4. Services, types and the preview layer

### 4.1 Move the browser-preview fakes out of `api.ts`
- **Where:** `src/lib/services/api.ts` (2,222 lines, 79 KB) has 182
  `isTauri()` calls (verified). About 710 lines sit inside
  `if (!isTauri())` branches, including 91 one-line guards (19 of them bare
  `return;`), plus ~60 lines of preview imports.
- **Seam:** the `commands` Proxy in `services/homeRouting.ts:383`.
- **Proposal:** add `preview/commands.ts`, a `Partial<typeof nativeCommands>`
  of fakes keyed by command name. The proxy dispatches to it when
  `!isTauri()` and resolves `undefined` for commands it doesn't implement,
  which removes every no-op branch. api.ts keeps only wrappers that do real
  work: `normalizeModels`, `queueList` pagination, `listSkillsFor` dedupe,
  `listMcpServerStatus` keying, and the 27 `unknown` result casts.
- **Savings:** ~800 lines out of api.ts (~450 net). **Risk:** med; 21 test
  files mock `$lib/services/api`.

### 4.2 Delete the thin pass-through wrappers
- **What:** 71 functions in api.ts are just a guard plus
  `return commands.x(sameArgs)` (~280 lines). Five
  (`add/read/prepare/answer/cancelNativeSideQuestion`, 1013-1031) have no
  preview branch and are pure aliases. `saveProject` (141) only forwards to
  `addProfileProject`.
- **Proposal:** after §4.1, callers use `commands.x` directly, as CLAUDE.md
  already prescribes. Keep `export const x = commands.x` only where a short
  name helps.
- **Savings:** ~250 lines. **Risk:** med (many call sites and mocks).

### 4.3 Dead exports in api.ts, their routing and their Rust commands
- **Dead in TS:** none of these has a caller outside `homeRouting`'s routing
  table (verified):
  - `readClaudeStatus` (442-455, plus `homeRouting.ts:522`)
  - `listArchivedThreads` + `ArchivedThreadWire` (730-748,
    `homeRouting.ts:371`)
  - `queueStart` (996-1006)
  - `openAgentThread` (1910-1915, `homeRouting.ts:377`)
  - `reloadMcpServers` (2020-2023)
  - `checkCodexBinary` (1295-1307, used only by another preview branch)
  - `previewArchived` (fixtures.ts:364)
- **Rust side:** the commands behind them then become dead too:
  - `list_archived_threads`, `read_claude_status` (`threads/lifecycle.rs`)
  - `queue_start` (`threads/queue.rs`)
  - `open_agent_thread` (`agents/commands.rs`)
  - `reload_mcp_servers` (`integrations/app_server.rs`)
  - `check_codex_binary` (`settings/commands.rs`)

  The backend reviewer counted these as "referenced from src/" only because
  the dead wrappers reference them.
- **Savings:** ~80 TS lines plus the Rust commands and their registrations.
  **Risk:** low.

### 4.4 Preview fixtures ship in the production bundle
- **What:** api.ts statically imports `preview/fixtures.ts` (1,699 lines),
  `stream.ts`, `memories.ts` and `attachments.ts`. `isTauri()` is a runtime
  check, so nothing tree-shakes. Verified: `dist/assets/autoName-*.js`
  contains `preview-pairing-code`.
- **Proposal:** after §4.1, load the preview module with
  `await import("./preview/commands")` when `!isTauri()`, or gate it on
  `import.meta.env`.
- **Savings:** ~60–90 KB unminified out of shipped JS. **Risk:** low–med.

### 4.5 `types.ts`: a hand-maintained re-export list and rename aliases
- **Where:** `types.ts:6-122` holds a 29-name import block and a 72-name
  `export type {…}` list. There are about 17 aliases that give one concept
  two names (106-122, 575, 610-616, 629, 658, 758), e.g.
  `ThreadSection = StoredThreadSection`, `GitCommit = CommitInfo`,
  `RemoteConnection = Connection`.
- **Hand-written duplicates of generated types:**
  - `TurnOptions` (549-566 vs bindings 2110).
  - `ClaudeStatus` and `HarnessKind`, re-declared in `api.ts:408, 431-439`.
- **Proposal:** `export type * from "$lib/bindings"` (local declarations
  still win). Rename the Rust structs, or use `#[specta(rename)]`, so the
  aliases can go. Derive `TurnOptions` with the existing `Override` pattern.
  Move the runtime function `codexVersionFromUserAgent` (types.ts:605) to
  utils.
- **Savings:** ~130 lines. **Risk:** low–med (Rust renames change bindings;
  run `deno task typegen`).

### 4.6 `homeRouting.ts` proxy is a 230-line if-chain
- **Where:** `homeRouting.ts:383-612` (`get` handler) and `343-379`
  (`normalizeResult` name switch). It also re-implements `hostKey` (42) and
  the "WSL · distro" label (73, 83), which exist in `$lib/app/host`.
- **Proposal:** a `Partial<Record<CommandName, Handler>>` table of
  per-command routers and result mappers, with generic id/path routing as
  the fallback. Import the host helpers.
- **Savings:** ~30 lines; mostly readability. **Risk:** med (core routing,
  but `homeRouting.test.ts` covers it).

### 4.7 `codexEvents.svelte.ts` listener boilerplate
- Four identical `listen → eventMatchesHome → scopeEvent → handler` blocks
  (466-489). The "mark unanswered + `recordUserInputRequest`" step is
  duplicated in `onServerRequest` and `onHarnessRequest` (378-393 vs
  417-423).
- Add `listenScoped(evt, handler)` and `persistQuestion(...)`. ~25 lines.

### 4.8 `is*Unsupported` predicates
- Five identical bodies at `api.ts:834-901`. All six prefixes are still live
  (checked against `compat.rs` and `usage/commands.rs`). Add one
  `isUnsupported(cause, ...prefixes)` and keep the constants CLAUDE.md
  requires. ~20 lines.

### 4.9 Repeated preview mutation snippets (mostly folded into §4.1)
- "Loop `previewData.projects` → find thread → mutate" appears 6 times. The
  `archiveThread`/`deleteThread` fakes are identical, as are
  `disconnectConnection`/`revokeConnection`. `readClaudeStatus` and
  `readHomeClaudeStatus` share a 9-line literal. The three `setThreadGoal*`
  functions are near-identical.
- Add `previewThread(id, fn)` and `previewProject(path, fn)`, and collapse
  the goal setters into `updateThreadGoal(threadId, patch)`. ~60 lines.

### 4.10 Over-exported symbols
- Exported but used only within their own file:
  - **types.ts:** `WorkspaceFileMatch`, `WorkspaceThreadMatch`,
    `TurnPlanStep`, `TurnError`, `TokenUsageBreakdown`,
    `ThreadUsageBreakdownGroup`, `ModelUpgradeInfo`, `McpAuthStatus`,
    `McpServerRuntimeStatus`.
  - **api.ts:** `StartedThread`, `RevertedThread`,
    `TurnSettingsUpdateStatus`.
  - **codexEvents:** `markUnanswered`, `onHarnessRequest`.
  - **Elsewhere:** `connections.setConnections`,
    `processes.processesFor`, and the fixtures `previewPixel`,
    `previewArchivedThreads`, `previewQueues`, `previewRepoInfo`.
  - **utils:** `fileRef.safeDecode`, `externalLinks.LinkHandlers`,
    `mentions.MentionSegment`, `rateLimits.UsageWindow`,
    `resize.ResizeHandleOptions`.
- Fully dead: `WorkspaceSearchGroup` and `RecentHome`.
- Other aliases: `api.ts:1690 copyText = handoffCopy` (pick one name), and
  `api.ts:134` re-exports `isTauri` (import it from `services/tauri`).

### 4.11 `WorktreeEntry` literal copied 7 times
- `entry()` builders appear in `AddWorktreeDialog.test.ts:7`,
  `RemoveWorktreeDialog.test.ts:7` and `worktrees.test.ts:18`. There are also
  3 full literals in `fixtures.ts:1012-1094` and one in `api.ts:1603-1622`.
  Export one `worktreeEntry(overrides)`. The same applies to the 7
  hand-written `ThreadSummary` literals in fixtures.ts. ~100 lines.

---

## 5. Thread UI (`src/lib/thread`)

### 5.1 Dead code
- `RewindThreadDialog.svelte`: the whole file is dead. Its only reference is
  an unused import at `ThreadView.svelte:57` (verified).
- `ThreadView.svelte:51` imports `groupForTurn`, which is then used only by
  tests.
- `FloatingMenu.svelte:7` imports `FileText`, which is unused.
- `autoName.ts:37 resetAutoNameState` and `messageVersions.ts:127 isBranch`
  have no references (verified).
- `threadStream.ts:155`: `ApplyOutcome.changed` is always `true` and never
  read. `misalignment` is read only by a test.
- An orphaned JSDoc at `threadStream.ts:160-163` sits above the wrong
  function, and there's a stale duplicate doc line at :152.
- Biome doesn't catch unused imports inside `.svelte` files, so these
  accumulate.
- **Savings:** ~60 lines + 1 file. **Risk:** low.

### 5.2 Extract the goal banner from ThreadView
- **Where:** `ThreadView.svelte:850-1025` (`goalCommand`,
  `goalBudgetCommand`, `applyGoalBudget`, `editGoalBudget`, `setGoal`,
  `toggleGoal`, `clearGoal`, the label/preset tables, `startGoalEdit`) and
  the markup at 1291-1357.
- **What:** about 240 lines of one feature live inside a 1,489-line view. It
  passes `aria-label` equal to `label` on 4 TooltipButtons, which already
  default to that.
- **Proposal:** `GoalBanner.svelte` plus `goalCommands.ts`, with Skeleton
  `Progress` for the budget bar.
- **Savings:** ~230 lines out of ThreadView. **Risk:** med (`/goal` is well
  tested).

### 5.3 Extract the transcript scroll-follow state machine
- **Where:** `ThreadView.svelte:213-233, 358-454` (`following`,
  `programmaticScroll`, `atBottom`, `recentDownScroll`, `maybeScroll`,
  `onScroll`, `onWheel`, the restore effect …) and the jump-pill condition
  at 1273.
- **Proposal:** a `TranscriptScroll` class in a `.svelte.ts` file next to
  `scrollPositions.ts`, exposing `attach`, `onScroll`, `onWheel`, `follow`,
  `restore` and `showJumpPill`.
- **Savings:** ~100 lines out of ThreadView; testable on its own.
  **Risk:** med (timing).

### 5.4 ThreadView boilerplate
- 12 `$derived(session.x)` aliases (185-196).
- One-line wrappers: `interrupt`, `compact`, `undoTurns`.
- A hand-rolled `lastTurnModel` loop where `findLast` would do.
- An inline 12-member `composer` type where `ReturnType<typeof Composer>`
  would do.
- Repeated expressions:
  - `estimateCost(...)` is computed twice.
  - `workspaceId ? cwd : thread?.cwd || cwd` appears 4 times.
  - `repoInfo?.root ?? (projectPath || cwd)` appears 3 times.
- `runCommand` is an if-chain (780-841) that should be a
  `Record<SlashCommandId, handler>`.
- **Savings:** ~60 lines. **Risk:** low.

### 5.5 FloatingMenu repeated markup
- 12 copies of the row-button class, 7 of the section header and 8 of the
  empty line. Use `section`, `row` and `empty` snippets, or `menu-row` from
  §2.6. ~80 lines.

### 5.6 WorkItem: five near-identical Collapsible expanders
- **Where:** `WorkItem.svelte:338-348, 416-441, 557-567, 571-580, 586-600`.
  The same Trigger class appears 4 times and the same Content class 5 times.
  The "▸ activity" line and the progress line are the same markup, and
  "Open thread" is duplicated (456-462, 511-517).
- **Proposal:** `expander`, `liveLine` and `openThreadButton` snippets.
  Optionally move the agent rows into `AgentActivityRow.svelte`.
- **Savings:** ~60 lines (plus ~90 from §2.5). **Risk:** low.

### 5.7 `AgentRun → SubagentDetail` mapping duplicated
- The same 11-field literal appears at `ThreadView.svelte:335-351` and
  `WorkItem.svelte:188-202`. Export `runAsSubagent(run)` from
  `services/agentRuns.svelte`. ~15 lines.

### 5.8 Status-to-tone classification in four places
- `WorkItem.svelte:231-236` and `FloatingMenu.svelte:136-169` (four
  functions) each restate the status lists, with slightly different results.
  Add one `agentStatus.ts` with `statusBucket` and `statusTone`. ~20 lines.

### 5.9 Queued-message edit duplication
- `QueuedMessageRow.svelte:23-29 queuedText` (an instance export from a
  component) is the same as `ThreadView.svelte:585-590 queuedPreview`.
- Inline-edit logic is near-identical in `QueuedMessageRow.svelte:31-55` and
  `UserMessageBubble.svelte:56-90`. Only one checks `isComposing`, and only
  one preserves chips.
- Move `queuedText` into `queueEntries.ts`, add an `inlineEdit()` helper, and
  reuse `editedParts`. ~30 lines.

### 5.10 Request cards hand-roll `submitState()`
- `ApprovalCard.svelte:43-64`, `QuestionCard.svelte:44-135` and
  `ElicitationCard.svelte:28-45` each carry `submitting`/`error`/try/finally.
  ~30 lines.

### 5.11 ThreadSession operation scaffolding
- `review`, `compact` and `undoTurns` (`threadSession.svelte.ts:135-219`)
  each set `operation`/`starting`/`streamError` and guard staleness. The idle
  check appears 3 times.
- Add `#runOperation(kind, body)` and `#maybeIdle()`. ~25 lines; med risk
  (concurrency).

### 5.12 `threadStream.ts` duplicate switch arms
- `agentMessage/delta` and `plan/delta` are identical (234-248). Summary
  padding is repeated (249-261). `outputDelta` and `terminalInteraction` are
  identical (276-288). Fall through, and add `padTo`. ~15 lines.

---

## 6. Composer

### 6.1 `richInput.ts`: six edit verbs share one frame
- **Where:** `deleteLineBreak` (750), `insertLineBreak` (778),
  `deleteToLineEdge` (799), `deleteToWordEdge` (831),
  `deleteCharBesideChip` (864) and `normaliseEditorDom` (918). Each does
  caretOffset → `toUnits(readParts)` → splice → `fromUnits` → render → place
  caret.
- **Proposal:** a private `editUnits(root, handlers, (units, caret) =>
  newCaret | null)`. In `richEditor.svelte.ts:162-264`, add `take(event,
  parts)` for the 5 repeated `preventDefault(); adopt(x); return;` blocks.
- **Savings:** ~50–60 lines. **Risk:** low (well tested).

### 6.2 Split `Composer.svelte` (1,374 lines)
- **Concerns:** attachments (798-941, 1033-1046, drag/drop 280-317), drafts
  (210-278), history recall (180-210) and prefs/model/plan settings
  (343-700).
- **Duplication:** `IMAGE_EXTENSIONS`/`basename`/`guessKind` (819-825) are
  verbatim copies from `preview/attachments.ts`.
- **Proposal:** `AttachmentStager` (`attachments.svelte.ts`),
  `ComposerDraft`, and a `ComposerSettings` class next to `composerPrefs`.
- **Savings:** Composer drops to ~600 lines. **Risk:** med (many `$derived`
  dependencies; Composer.test.ts is broad).

### 6.3 Composer popovers → Skeleton Popover
See §3.2. ~100–130 lines.

### 6.4 Chip construction duplicated three ways
- `buildMentionChip`/`buildSkillChip`/`buildAttachmentChip`
  (`richInput.ts:156-303`) repeat the span shell. `insertMentionChip` and
  `insertSkillChip` have identical bodies. "Is this a chip" is spelled out at
  42-58, 387-390 and 905. `richEditor.svelte.ts:298-320` has twin insert
  methods.
- Add `chipShell`, `buildChip` (also used by `renderPartsWith`),
  `insertChip`, one `CHIP_SELECTOR`, and `insertAtQuery`. ~40 lines.

### 6.5 Selection boilerplate
- "Get the live range" has about 9 copies and "select this collapsed range"
  5. `placeCaretBesideChip` is a pure alias, and `removeMentionChip` removes
  any chip.
- Export `liveRange()` and `selectCollapsed()`, drop the alias, and rename
  to `removeChip`. ~30 lines.

### 6.6 Caret helpers re-implement each other
- `moveCaretVertically` (509-532) hand-rolls `lineEdgeOffset` (695-700).
- `chipBesideCaret` and `chipAcrossLineBreak` share a 10-line neighbour
  lookup.
- `chipBesideCaret` also re-implements `chipContaining`.
- The `Units` alias is declared after its first uses.
- ~25 lines.

### 6.7 Dead Claude permission presets
- `CLAUDE_PERMISSION_PRESETS` and `permissionPresetsFor`
  (`composerPrefs.svelte.ts:75-115`) only take effect when
  `can(harness, "permissionModes")`, which is exactly when the UI uses
  `CLAUDE_MODES` instead. Their only live job is mapping legacy
  `"auto-review"` to `"auto"`.
- Replace them with a one-entry `LEGACY_PRESET` map. ~40 lines.

### 6.8 Composer-local duplication
- `toggleSubagentModel` and `toggleSubagentEffort` share their expansion
  logic. `(models ?? [])` appears 7 times.
- The plan-mode set/clear branch is repeated 4 times. `saveScopedPrefs` is
  inlined although `persist()` exists.
- `parts.map(p => ({...p}))` appears 6 times, and the single-plain-text check
  twice.
- Add `togglePolicy`, `modelList`, `setPlanMode`, `cloneParts`, `plainText`
  and `trimParts` (the last three in `composerParts.ts`). ~35 lines.

### 6.9 Dead code and stale comments
- `richInput.ts:333 renderParts` and `composerPrefs.resolveAppSubagents` are
  used only by tests.
- `planTurnOptions`, `turnOptions` and `ensureHarnessModels` are pure
  wrappers.
- `export cancelGoalEdit` has no external caller.
- Two orphaned or doubled JSDocs.
- The comment on `coerce` is wrong: it says the function drops unknown
  fields, but it doesn't.
- `composerPrefs.svelte.ts` uses no runes, so it can be renamed to `.ts`.
- ~25 lines.

### 6.10 Small cleanups
- `ReviewTargetPicker.svelte:45-46`: a double `find` and a needless
  `?? ""`.
- `AdaptiveToolbar.svelte:118-126`: three filtered `{#each}` loops; group
  them once.
- `quick/quickChat.ts acceleratorFromEvent` is used only by SettingsView.
  Move it to `utils/accelerator.ts`.

---

## 7. Layout, panels and app state

### 7.1 SettingsView (876 lines): repeated cards and rows; split it
- **Repeated markup:**
  - The card wrapper appears 8 times (369, 493, 564, 654, 680, 725, 733,
    759).
  - The label/description/control row appears about 12 times.
  - There are 2 hand-built switches (§3.3) and 4 error cards.
- **Proposal:** `settingsCard` and `settingRow` snippets or components,
  Skeleton `Switch`, and a split into `layout/settings/{General, Agents,
  Keyboard, Advanced}.svelte`.
- **Savings:** ~150 lines; the main file drops to ~300. **Risk:** med.

### 7.2 Settings navigation state is mirrored
- `App.svelte:72-85, 290-305` syncs a local `settingsOpen` from
  `settingsNav.open` through an effect. SettingsView takes 4 props that
  mirror the store.
- Section ids are typed `string`, the icons live in a separate map, and
  `SettingsSection` names two different types.
- Use `{#if settingsNav.open}` and read the store directly. Put `icon` on
  `SETTINGS_SECTIONS` and type the ids as a union. ~25 lines.

### 7.3 Dead code
- `SettingsView.svelte:139-140, 215-217` calls `readHomeOverview()` and
  stores `overview`, which nothing reads. That costs an IPC round-trip every
  time Settings opens (verified).
- An orphaned docstring at `SettingsView.svelte:258`.
- `panels/workspaceSearch.ts`: `totalMatchCount` is used only by its test,
  `emptyStateLabel` is a one-liner, and `debounce` belongs in utils.

### 7.4 HomePage duplication
- The 4 usage tiles are written out by hand (244-264) while the tiles above
  use `{#each}`.
- 3 project sections have the same heading, blurb and list (346-396).
- 2 identical cancellable `$effect` fetches (83-111).
- A local `relativeTime` (§2.3) and the hand-rolled `iconAction` (§3.6).
- ~80–100 lines.

### 7.5 Sidebar
- 5 header TooltipButtons share one 120-char class (703-746). Render them
  with `{#each}`.
- The hover-reveal class is copied 5 times.
- The Collapsible folder wrapper is written twice (535-546 vs 640-651); use
  a `folderNode` snippet.
- `{#if thread.harness === "claude"}` at line 434 (see §12).
- ~50 lines.

### 7.6 SideQuestions vs NativeSideQuestions
- The list and the textarea footer are built separately in each
  (`SideQuestions.svelte:322-346, 425-465`;
  `NativeSideQuestions.svelte:218-256`), and their styles have drifted.
- Extract `SideQuestionList.svelte` and `SideComposer.svelte`. ~50 lines.

### 7.7 RightPanel header
- A 9-branch if-chain for icon and title (`RightPanel.svelte:148-178`).
  Replace it with a `PANEL_META` table. ~25 lines.

### 7.8 ProjectDetail
- "Local chats" and "Thread messages" are byte-identical result blocks
  (467-500). Use a `resultGroup` snippet.
- The Sources tab is mis-indented (504-566). ~30 lines.

### 7.9 `menuAction`: a 130-line if-return chain
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
  `WorktreeEntry` in 3 test files plus fixtures (§4.11). Adding a field to
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
| Frontend never checks harness kind | `Sidebar.svelte:434` (`thread.harness === "claude"` badge); `ThreadView.svelte:718, 1437`; `HarnessMenu.svelte:50`; hard-coded "Codex" in `Composer.svelte:1209-1210, 1237` and the TypingDots default label; "Claude" in `NativeSideQuestions.svelte:216` |
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
   reads `previewArchivedThreads` (§4.3).
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
   every open and its result is unused (§7.3).
8. **Queued-message edits flatten chips to `@name`,** while bubble edits
   preserve them (§5.9).

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
2. **Helpers first, then sweeps.** Add `errorMessage`, `utils/path`,
   `utils/format`, `escapeHtml`/`.code-dark`, `.prose-md`, the `@utility`
   classes, and the test factories (§2, §11.1). Many later items get shorter
   once these exist.
3. **Rust mechanical wins.** `HomeWindow::ctx()`, `db` executor, `locked()`
   / `blocking()` / `require()`, `Default` derives (§10.1–10.5, §10.8).
   All low risk, compile-checked, and about 700 lines together.
4. **api.ts restructuring** (§4.1, §4.2, §4.4, §4.5). This is the biggest
   frontend win. Do it as one focused change with the 21 mocking test files
   updated together.
5. **Skeleton swaps** (§3). One widget type per change, each with its tests
   adjusted.
6. **Large component splits:** ThreadView, Composer, SettingsView,
   IntegrationsSection (§5.2, §5.3, §6.2, §7.1, §8.1).
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
