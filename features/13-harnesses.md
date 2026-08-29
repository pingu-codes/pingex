# Harnesses: Claude Code and ACP alongside Codex

Priority: P1

Decisions recorded on Linear map PIN-5 (tickets PIN-6 to PIN-15). Research
behind them: `docs/research/acp-neutral-model.md`,
`docs/research/claude-code-stdio.md`, `docs/research/claude-sessions.md`.
Rationale: `docs/adr/0002-harness-interface.md`.

## What exists today

Pingex drives exactly one agent CLI, Codex, over its `app-server` JSON-RPC
protocol. `HomeContext.session` is a `CodexSession` (`src-tauri/src/lib.rs`),
the frontend reducer `applyThreadEvent` (`src/lib/thread/threadStream.ts`)
switches on Codex method names, `ThreadItem` (`src/lib/types.ts`) is the
union of Codex item types, the journal (`src-tauri/src/codex/journal.rs`)
stores Codex payloads, and every window binds to a `CODEX_HOME`.

Claude Code has a bidirectional stdio protocol (`claude -p --input-format
stream-json --output-format stream-json --include-partial-messages
--replay-user-messages --permission-prompt-tool stdio`) that covers chat,
streaming, tool calls, approvals, interrupt, resume and model changes. Nothing
in the app can reach it without either faking Codex vocabulary or building a
second frontend.

## Vocabulary

Defined in `CONTEXT.md`, restated here so the spec reads on its own:

- **Harness**: the agent CLI being driven. Codex, Claude Code, or any ACP agent.
- **Home**: one harness's config directory. Today's `CODEX_HOME`; for Claude,
  `CLAUDE_CONFIG_DIR` or `~/.claude`.
- **Profile**: one Pingex database plus the Homes it knows. What a window binds to.
- **Driver**: the Rust implementation of the harness interface for one harness.
- **Capability**: something a driver can do beyond the required core. Declared
  by the driver, or probed and cached (the old Feature).

"Supported harness" means: chat, streaming, tool items, approvals, interrupt,
resume and list, model and effort. Everything deeper is a Capability the UI
hides when absent.

## The neutral event model

One Rust enum, `HarnessEvent`, generated into `src/lib/bindings.ts` by
tauri-specta and delivered on one Tauri event, `harness:event`. Envelope:

```
{ profile, homeKey, threadId, turnId?, seq, event }
```

`threadId` is the Pingex id (see Profiles). `seq` is per thread, monotonic,
assigned by the driver, and orders the journal.

### Events

Turn lifecycle:

- `turn_started { turnId, model?, effort?, mode? }`
- `turn_ended { turnId, stopReason, error?, durationMs?, usage? }` with
  `stopReason` one of `end_turn | max_tokens | max_turn_requests | refusal |
  cancelled | error`.
- `turn_state { state: running | idle | requires_action }`.

Content (each carries `itemId`, so a chunk stream becomes one item):

- `user_message { itemId, content: ContentBlock[] }` (whole, not chunked).
- `agent_message_chunk { itemId, text, ext? }`.
- `agent_thought_chunk { itemId, text, channel: summary | raw, index }`. Codex
  streams a summary and the unabridged text in parallel; Claude has one
  stream, which is `raw`. `ReasoningBlock` renders whichever channel has text.

Tool calls:

- `tool_call { itemId, title, kind, status, name?, content?, locations?,
  rawInput?, ext? }`.
- `tool_call_update { itemId, title?, kind?, status?, content?, locations?,
  rawOutput?, outputDelta?, ext? }`. Every field except `itemId` is optional.
  Omitted means unchanged, `null` clears, a value replaces, `outputDelta`
  appends to the item's terminal text. This rule is not negotiable: Codex's
  `item/completed` for a command can arrive with `aggregatedOutput: null`
  after the output streamed, and a translator that copies it wipes the text.
- An update whose `itemId` has no call yet creates a pending shell. Claude's
  `can_use_tool` names a `tool_use_id` before the stream announces the call.

`ToolKind` is ACP's: `read | edit | delete | move | search | execute | think |
fetch | switch_mode | other`. `ToolCallStatus` is `pending | in_progress |
completed | failed | cancelled`; the driver sets `cancelled`, the reducer also
sets it on every unfinished call when a turn ends `cancelled`.

`ToolCallContent` is one of:

- `{ type: "content", content: ContentBlock }`
- `{ type: "diff", path, oldText: string | null, newText }` (`oldText: null`
  is a new file, `newText: ""` a deletion)
- `{ type: "terminal", text, exitCode?, cwd? }` (inlined; Pingex owns no
  terminal abstraction and both official ACP adapters fake one through
  `_meta` anyway)

Session-level:

- `plan { itemId, entries: [{ content, priority, status }] }`. Codex
  `turn/plan/updated`, Claude `TodoWrite` and `Task*`. The Codex plan-mode
  markdown item is an `agent_message_chunk` with `ext.codex.role = "plan"`.
- `usage_update { used, size, cost?, breakdown? }`.
- `config_options { options: SessionConfigOption[] }`, complete state, ACP
  shape. What the composer renders.
- `available_commands { commands: [{ name, description, inputHint? }] }`.
- `session_info { title?, updatedAt? }`.
- `notice { level: info | warning | error, text, detail?, retrying?, ext? }`.
  Replaces Codex's four warning notifications, `model/rerouted`,
  `hook/completed` failures and Claude's `api_retry` and `permission_denied`.
- `compaction { itemId, trigger: manual | auto, preTokens?, postTokens? }`.

Requests from the harness travel on `harness:request` as `HarnessRequest`,
answered by `requestId`:

- `request_permission { requestId, toolCall, options: [{ optionId, name,
  kind: allow_once | allow_always | reject_once | reject_always }], ext? }`
- `request_user_input { requestId, questions }` (existing `UserInputQuestion`)
- `elicitation { requestId, ... }` (existing)
- `request_cancelled { requestId }` (the harness withdrew it)

Answer: `{ outcome: "cancelled" } | { outcome: "selected", optionId } |
{ outcome: "answered", answers }`.

Thread bookkeeping that is not transcript (queue changed, reverted, sections,
projects, rate limits, MCP status, remote control) travels on
`harness:thread`, tagged `ext.codex`.

### Items

`ThreadItem` becomes a union on `kind`: `user_message | agent_message |
thought | tool_call | plan | compaction | notice`. `THREAD_ITEM_TYPES` goes
away. `turnSegments` keys on `kind`: thought runs collapse, tool-call runs
collapse into "Worked for", messages and plans and compactions stay visible.
`WorkItem.svelte` dispatches on `kind`, then on `toolKind`, then on `ext` for
harness-specific cards. `rendersSomething` is `kind !== "thought" ||
text.length > 0`.

`Turn.status` becomes the stop reason (`running` while open). `ThreadDetail`
gains `harness` and `nativeId`.

How Codex items map:

| Codex item | kind | ToolKind | content |
|---|---|---|---|
| `commandExecution` | tool_call | execute (read / search / list when `commandActions` says so) | terminal |
| `fileChange` | tool_call | edit | one diff per file |
| `mcpToolCall`, `dynamicToolCall`, `functionCallOutput` | tool_call | other | text; `ext.codex.{server,tool,arguments}` |
| `webSearch` | tool_call | fetch | text |
| `imageView` | tool_call | read | resource_link |
| `imageGeneration` | tool_call | other | image |
| `collabAgentToolCall`, `subAgentActivity` | tool_call | think | text; `ext.codex.subagent` |
| `enteredReviewMode` / `exitedReviewMode` | tool_call | switch_mode | `ext.codex.review` |
| `hookPrompt` | notice | | |
| `userInputAnswered` | tool_call | other | `ext.questions`, `ext.answers` |
| `sleep` | tool_call | other | |
| guardian review | field on the item: `ext.codex.guardian` | | |

### Extensions

`ext` is `{ codex?, claude?, acp? }` on any event, item or request. A driver
writes only its own key. The reducer never branches on `ext` for anything the
transcript needs to be readable. A card that reads `ext.codex.*` is a
Codex-only card and is hidden when the key is absent.

## The driver interface

`src-tauri/src/harness/mod.rs`:

```rust
#[async_trait]
pub(crate) trait Driver: Send + Sync {
    fn kind(&self) -> HarnessKind;
    fn home_key(&self) -> &str;
    fn capabilities(&self) -> &CapabilitySet;

    async fn start_thread(&self, req: StartThread) -> Result<ThreadHandle, DriverError>;
    async fn resume_thread(&self, native_id: &str, cwd: &Path) -> Result<ThreadHandle, DriverError>;
    async fn read_thread(&self, native_id: &str) -> Result<ThreadDetail, DriverError>;
    async fn list_threads(&self, cwd: Option<&Path>) -> Result<Vec<ThreadSummary>, DriverError>;
    async fn start_turn(&self, thread: &ThreadHandle, input: TurnInput, opts: TurnOptions) -> Result<TurnId, DriverError>;
    async fn interrupt(&self, thread: &ThreadHandle) -> Result<(), DriverError>;
    async fn respond(&self, thread: &ThreadHandle, request_id: RequestId, answer: RequestAnswer) -> Result<(), DriverError>;
    async fn config_options(&self, thread: Option<&ThreadHandle>) -> Result<Vec<SessionConfigOption>, DriverError>;
    async fn set_config_option(&self, thread: &ThreadHandle, id: &str, value: ConfigValue) -> Result<Vec<SessionConfigOption>, DriverError>;

    // Optional. Default impls return Err(DriverError::Unsupported(..)).
    async fn rename(..); async fn compact(..); async fn delete(..); async fn archive(..);
    async fn fork(..); async fn context_usage(..);
    async fn extension(&self, thread: Option<&ThreadHandle>, call: ExtensionCall) -> Result<Json, DriverError>;

    async fn shutdown(&self);
}
```

A trait, not an enum, so the ACP driver and a test fake are additions rather
than edits to every match. `extension()` carries Codex-only verbs (queue,
revert, sections, projects, goals, skills, MCP status, rate limits, remote
control) as `{ ns: "codex", method, params }` onto the existing
`requests.rs` builders; every other driver returns `Unsupported`.

`DriverError` is `NotRunning | Unsupported(Capability) | Rejected { code,
message, data } | ActiveTurn { turn_id } | NotFound { thread } |
Transport(String)`. `child.rs` stops formatting `"Codex request failed: "`
and returns the structured error; `compat.rs` and `threads/turn.rs` read it.
`Unsupported` serialises as `harness-unsupported:<capability>`; the five
`codex-*-unsupported` prefixes survive one release as aliases for
`api.ts`.

### Capabilities

`Capability` is an enum: `Rename, Compact, Fork, Delete, Archive,
ContextUsage, Queue, Revert, Sections, Projects, Goals, TurnSettings,
ThinkingBudget, Rewind, Hooks, Plan, UserInput, Elicitation`. `CapabilitySet`
has a declared half (set at construction) and a probed half (today's
`Feature`, renamed `Probe { capability, method_prefix, since }`, private to
the Codex driver; Claude reads `system/init.capabilities[]` once per
process). The bootstrap payload carries the declared set so the frontend hides
controls before the first call.

### Processes

- Codex: one child per Home, lazily spawned, as now. `CodexSession` becomes
  `CodexDriver`; `ChildSink` stays as its JSON-RPC sink.
- Claude: one `claude` process per active thread, in a map keyed by Pingex
  thread id. Spawned on `start_thread` with `--session-id <uuid>`, or on the
  first `start_turn`/`respond` for a thread with no live process with
  `--resume <native_id>` from the stored cwd. Reaped after 10 idle minutes
  with no pending request (close stdin, SIGTERM after 30 s). `read_thread`
  never spawns; it reads the `.jsonl`. `claude/child.rs` has its own NDJSON
  reader with `on_frame`, `on_control_request`, `on_closed`.
- `HomeContext.session` becomes `driver: Arc<dyn Driver>`.

## Approvals and user input on Claude

Every `can_use_tool` becomes one `request_permission`. The driver picks the
card by `tool_name`, builds the options and translates the chosen `optionId`
back into a `PermissionResult`. The frontend never sees `PermissionUpdate` or
tool names.

| `tool_name` | ToolCall | Card |
|---|---|---|
| `Bash` | execute; title = description or command; terminal content with cwd | command approval |
| `Edit`, `MultiEdit` | edit; one diff per edit from `old_string`/`new_string` | file-change approval |
| `Write` | edit; diff with `oldText` = file on disk or null | file-change approval |
| `NotebookEdit` | edit; diff on `notebook_path` | file-change approval |
| `ExitPlanMode` | switch_mode; content = plan text | plan approval |
| `AskUserQuestion` | not a permission: `request_user_input`; answered as `allow` with `updatedInput.answers` | question card |
| `WebFetch`, `WebSearch` | fetch | generic |
| `Read`, `Glob`, `Grep` | read / search | generic |
| `Agent`, `Task`, `Skill` | think / other | generic |
| `mcp__<server>__<tool>` | other; `rawInput`; `ext.claude.mcp` | generic with JSON expander |
| anything else | other; title from `display_name`, `title`, `tool_name` | generic |

Options, in order: `allow_once` "Allow"; `allow_always` "Always allow" only
when `permission_suggestions` is non-empty and `suppress_always_allow_rule`
is not set, returning the suggestions verbatim as `updatedPermissions`;
`reject_once` "Decline" (`deny`, message "User declined"). `ExitPlanMode`
offers "Implement" (`setMode default`), "Implement, auto-accept edits"
(`setMode acceptEdits`) and "Keep planning" (deny, "Revise the plan").
`default_to_no` focuses Decline. `decision_reason` (ANSI stripped) and
`blocked_path` show under the title.

Interrupt: the driver sends `interrupt`, then answers every pending
`can_use_tool` with `deny { message: "Interrupted", interrupt: true }` and
emits `tool_call_update { status: cancelled }` for each. An unanswered prompt
blocks the process forever, so nothing is ever left pending. Claude's own
`control_cancel_request` becomes `request_cancelled`. Driver restart with a
card open denies everything. Auto-denies (`permission_denied`) are notices.

The generic permission card is one new component that also takes over Codex's
`item/permissions/requestApproval`. Codex option ids are `accept |
acceptForSession | decline`, so `respond_approval` is unchanged underneath.

`request_user_dialog` is not declared in `initialize.supportedDialogKinds`
and therefore never arrives.

## Profiles, Homes and storage

Database at `~/Library/Application Support/pingex/profiles/<name>/pingex.db`
(`dirs::data_dir()` elsewhere), default name `default`. New tables:

```sql
homes(home_id PK, harness, config_dir, binary_path, label, created_at,
      UNIQUE(harness, config_dir));
project_defaults(project_path PK, home_id REFERENCES homes);
threads(thread_id PK, home_id REFERENCES homes, native_id, cwd,
        origin 'created'|'imported', created_at, UNIQUE(home_id, native_id));
```

Every existing thread-keyed table keeps its shape; `thread_id` now means the
Pingex id. `thread_items.payload` stores the `kind`-shaped item.
`turn_settings` gains `mode`. The journal moves to `harness/journal.rs` and
observes `HarnessEvent`s: it buffers thought and tool-call deltas and writes on
`tool_call_update { status: completed | failed | cancelled }` or `turn_ended`,
with the same `after_item_id` anchoring. For Claude the journal is the primary
store of rendered items; for Codex it keeps patching `thread/read` gaps.

First launch with no profile database: create it; for each known Codex home
(`--codex-home`, `CODEX_HOME`, the saved override, `~/.codex`, every
`recentHomes` entry) that has a `pingex.db`, attach it, add a `homes` row and
copy every table with `thread_id` rewritten to a fresh Pingex uuid and a
`threads` row `(native_id = old id, origin = created)`; rename the old file to
`pingex.db.imported`; add a `homes` row for `~/.claude` if the `claude`
binary resolves, importing no sessions. `canonical_home` stays as the
`config_dir` normaliser and the `homeKey` tag; `home_id` is the registry key.

Window binding: `AppState.contexts` becomes `HashMap<ProfileId,
ProfileContext>`, each holding the db and a map of `home_id` to `HomeContext`.
`bindings` maps window label to profile. `HomePicker` becomes
`ProfilePicker`, shown only when no profile exists and nothing can be imported.
`deno task dev` reads `PINGEX_PROFILE` (default `default`); `dev:work` sets
`work`. `--codex-home` and `CODEX_HOME` remain as a shortcut meaning "ensure
this Codex home exists and make it the project default for this launch".

Claude Home: `CLAUDE_CONFIG_DIR` from Pingex's own env, else `~/.claude`,
passed into the child's env so session slugs match. Binary:
`PINGEX_CLAUDE_CLI_PATH`, then `homes.binary_path`, then `binary.rs` with
`~/.claude/local/claude` and `~/.local/bin/claude` added to the fallback
dirs.

Sidebar lists only threads Pingex created. External Claude sessions arrive
through an explicit import action (Settings, Data), which walks
`<config>/projects/*/<uuid>.jsonl` per `docs/research/claude-sessions.md`
and inserts `threads` rows with `origin = imported`.

## Composer options

The composer renders `SessionConfigOption`s from `Driver::config_options` and
nothing else. Four ids are known by name:

| id | category | Codex | Claude |
|---|---|---|---|
| `mode` | mode | `read-only`, `auto`, `full-access`, `plan` | `default`, `acceptEdits`, `plan`, `auto`, `dontAsk`, `bypassPermissions` |
| `model` | model | `model/list` ids | `initialize.models[]` ids plus `default` |
| `effort` | thought_level | `ReasoningEffortOption` values | `low`, `medium`, `high`, `xhigh`, `max` when the model supports effort |
| `fast_mode` | model_config | boolean when offered | boolean from `fast_mode_state` |

Any other option renders as a generic popover entry in array order.
`PERMISSION_PRESETS`, `Model`, `ReasoningEffortOption` and the plan-mode
toggle become instances of this shape; the preset descriptions move into the
Codex driver. `ModelPopover` and `PermissionsPopover` become one
`ConfigOptionPopover`. Plan mode is `mode === "plan"` on both harnesses.
Slash commands come from `available_commands`; skills stay a Codex
extension.

`turnOptionsFrom` becomes `turnConfigFrom`, returning `Record<configId,
value>` from the same `fallback / projects / threads` scoping. Prefs are
stored as `{ harness, config }` per scope and ignored when the draft's harness
differs. Subagent policies stay Codex `ext` options.

The draft composer shows a harness chip listing the profile's Homes grouped by
harness, defaulting to `project_defaults` then the profile default, with
"Make default for this project". After the thread exists the chip becomes a
static label in `ThreadHeader` (`home › harness`). Existing threads never
switch. Quick chat uses the profile default Home.

Mid-turn changes go through `set_config_option` only. Codex uses
`turn/settings/update` when the `TurnSettings` probe passes, else next turn
with the existing notice; Claude uses `set_model`, `set_permission_mode`,
`set_max_thinking_tokens` live.

Settings: `agent` becomes per-Home with a Home selector; `modelFeatures`
folds into it; `integrations` hides for a Claude Home; `data` gains the
profile path and the import action.

## Testing and version policy

Golden fixtures in `tests/fixtures/protocol/<harness>/<case>/` with
`wire.ndjson` (raw frames, recorded by the live suite under
`PINGEX_RECORD_FIXTURES=1`, redacted) and `events.json` (`HarnessEvent[]` and
the resulting `Turn[]`). A Rust test asserts translate(wire) = events; a
Vitest test, reading the same files through a Vite alias, asserts
reduce(events) = turns. First cases: plain reply, command with streamed
output, file edit with approval, interrupted turn, plan-mode exit, compaction,
error. Claude adds AskUserQuestion and a resumed turn; Codex adds review mode
and a queued message. `src/lib/testing/codexEvents.ts` and
`preview/stream.ts` play these fixtures.

`deno task test:e2e:claude` runs `src-tauri/tests/live_claude/`, gated on
`PINGEX_E2E_CLAUDE`, on the user's own OAuth `~/.claude` (never `--bare`),
model `haiku`, `--max-turns 2`, `--no-session-persistence` except the resume
test, which uses a throwaway `CLAUDE_CONFIG_DIR`. `expect_legacy` for Claude
uses CLI versions (`1.0.59` for the stdio permission prompt, `2.1.211` for
mid-turn `set_model`).

`docs/SUPPORTED_VERSIONS.md` gains a per-harness table: Codex keeps its three
tiers; Claude Code has a tested version (`2.1.251`) and a protocol floor
(`1.0.59`) below which the driver refuses to start. Above the tested version
the app warns. `deno task versions:check` compares the doc with `claude
--version` when a binary resolves. ADR 0001 gets one amendment: it applies
to every harness; tiers exist only where there is a mirror to test against.

## Prototype findings folded in

From `prototype/neutral-reducer`: plan-driving `think` calls (TodoWrite,
Task*) complete with `content: null`; omitted `content` on an update means
unchanged; an orphan update creates a pending call; thoughts render from
whichever channel has text; turn-level cancellation marks unfinished calls;
Claude text-block item ids are synthesised as `<turn>-m<n>-b<index>` and the
journal accepts driver-made ids.

## Implementation status

Branch `feat/claude-harness`, first slice, verified against Claude Code
2.1.251 on 2026-08-30. What is built:

- `src-tauri/src/harness/`: the neutral `HarnessEvent` and `HarnessRequest`
  types (tauri-specta, emitted on `harness:event` and `harness:request`) and
  `project.rs`, which turns neutral events into Codex-shaped notifications.
- `src-tauri/src/claude/`: the process wrapper (`child.rs`), the stream-json
  translator (`translate.rs`), the per-tool card and permission mapping
  (`tools.rs`, `permissions.rs`) and the driver (`driver.rs`): one process per
  active thread, `--session-id` on first use and `--resume` after a restart,
  `set_model` / `set_permission_mode` control requests when the composer's
  choice changes, interrupt that denies every pending prompt, a protocol
  floor check against `1.0.59`.
- Storage: `harness_threads` (thread id, harness, cwd, title, timestamps,
  archived) and a `harness` column on `thread_summaries`. The journal is the
  transcript for a Claude thread; `read_thread` rebuilds turns from it.
- Commands route by `storage::thread_harness`: `start_thread` takes a
  `harness`, and `start_turn`, `interrupt_turn`, `read_thread`,
  `respond_approval`, `respond_user_input`, `respond_server_request`,
  `rename_thread`, `archive_thread`, `delete_thread`,
  `threads_with_active_turns` do the right thing for a Claude thread.
  Codex-only verbs (queue, goals, subagents, auto-naming) answer without a
  Codex round trip. New: `list_harness_models`, `read_claude_status`.
- Frontend: a harness chip on the draft composer (persisted in composer
  prefs, per project), Claude's model aliases and permission presets in the
  existing popovers, option buttons on the approval card, a "Claude" badge in
  the sidebar, and `harness:request` feeding the approval and question stores.
- Tests: unit tests per module, plus two golden streams recorded from the
  real CLI under `tests/fixtures/protocol/claude/` (a Bash call, a Write
  that prompts for permission).

Deliberate differences from the sections above, to land a working slice:

- The transcript still consumes Codex-shaped notifications. The Claude
  driver emits neutral events and `harness/project.rs` projects them; the
  reducer migration to `kind`-shaped items is the next slice.
- No Profile database yet. Claude threads live in the current Codex home's
  `pingex.db` under `harness_threads`, and the thread id doubles as the
  Claude session id. The Profile migration is unchanged as a plan.
- Idle reaping of Claude processes, importing external sessions, the
  `test:e2e:claude` live suite, `versions:check` for Claude, and the
  settings-page Home selector are not built.
- `MultiEdit` diffs, `AskUserQuestion` and `ExitPlanMode` are mapped and unit
  tested but not yet exercised live.

## Not yet specified

- Pingex app-owned subagents on Claude (likely an SDK-hosted MCP server over
  `mcp_message`).
- The generic ACP driver: process model, capability negotiation, which agents
  to validate against. The interface above must not rule it out; nothing here
  builds it.
- Import UX for external Claude sessions beyond the Settings action.
- Codex `thread/read` projection defects under the neutral item model.
- Handoff commands and deep links per harness (`codex resume` vs
  `claude --resume`).
- Which of Claude's hooks, `rewind_files`, thinking budget and per-turn cost
  become Pingex features.
- Quick chat harness choice.

## Out of scope

Driving Codex through ACP. Native drivers for harnesses other than Codex and
Claude Code. Voice, browser and computer use, scheduled tasks. Claude Code
remote control and teleport.

## Acceptance checks

- A new thread on a Claude Home streams text, thoughts, a Bash call with
  output and an Edit with a diff into the same transcript components a Codex
  thread uses, with no harness branch in `threadStream.ts` or `turnSegments.ts`.
- A Bash `can_use_tool` shows the command approval card; Allow runs it,
  Decline returns a deny the model sees; Always allow appears only when Claude
  suggested a rule and returns that rule.
- Interrupting a Claude turn with a permission card open ends the turn
  `cancelled`, closes the card, and leaves no unanswered control request.
- Closing and reopening the app restores a Claude thread from the journal and
  `--resume`s it on the next message.
- First launch on an install with a `<codex_home>/pingex.db` produces a
  profile database with every thread present, pinned and named as before, and
  leaves `pingex.db.imported` behind.
- The draft composer offers both harnesses when both Homes exist, remembers
  the per-project default, and shows Claude's permission modes and Codex's
  presets from the same popover component.
- `deno task test` passes the golden fixtures for both harnesses;
  `deno task test:e2e:claude` passes against the installed CLI;
  `deno task versions:check` fails when `docs/SUPPORTED_VERSIONS.md` is behind
  the installed `claude`.
