# Claude Code session storage and listing

Research for wayfinder ticket PIN-8. Question: what does Pingex need to know
about Claude Code's on-disk sessions to (a) resume a session it created and
(b) import one it didn't?

Verified against Claude Code **2.1.251** and `@anthropic-ai/claude-agent-sdk`
**0.3.251** on 2026-08-29 (macOS). Every claim is tagged with its source:

- **[docs]** https://code.claude.com/docs/en/sessions.md, `cli-reference.md`,
  `env-vars.md`, `claude-directory.md`, `agent-sdk/sessions.md`,
  `agent-sdk/typescript.md`, `checkpointing.md`, `headless.md`.
- **[sdk]** the bundled `sdk.mjs` / `sdk.d.ts` of the Agent SDK, read after
  prettifying (line numbers below refer to that prettified copy).
- **[disk]** structural inspection (`jq` key-sets, type counts) of ~150 real
  transcripts written by 2.1.81 - 2.1.251, plus ~10 throwaway `claude -p`
  sessions created for this research. All message content is redacted here;
  shapes are shown with placeholder strings.
- **[live]** behaviour observed by running `claude -p` with specific flags.

The docs state the transcript format "is internal to Claude Code and changes
between versions". The SDK itself reads the files directly (it does not shell
out to the CLI for listing), so the SDK's reading strategy is the best
available definition of "stable enough to depend on". Section 7 separates the
fields that strategy relies on from the ones that are clearly internal.

## 1. Where sessions live

### 1.1 Config directory

`<config>` is `$CLAUDE_CONFIG_DIR` if set, else `~/.claude`. The SDK
NFC-normalises the path. [docs env-vars; sdk `Gt`, l.33476]

```js
function Lh() { return process.env.CLAUDE_CONFIG_DIR; }
var Gt = memo(() => (Lh() ?? join(homedir(), ".claude")).normalize("NFC"));
```

Consequence for Pingex: it must resolve the config dir exactly the way the CLI
it spawns will, i.e. honour the same `CLAUDE_CONFIG_DIR` it passes in the
child's environment. On this machine there are two config dirs in use
(`~/.claude` and one set via `CLAUDE_CONFIG_DIR`) and their session sets are
disjoint. [disk]

### 1.2 Project directory name ("slug")

Path: `<config>/projects/<project>/<sessionId>.jsonl`.

`<project>` is the **absolute, realpath'd** cwd with every character outside
`[A-Za-z0-9]` replaced by `-`. Over 200 characters it is truncated to 200 and
`-<base36 hash of full path>` is appended. [docs sessions; sdk l.48523-48584]

```js
var ks = 200;
function vu(e) { return e.replace(/[^a-zA-Z0-9]/g, "-"); }
function Jf(e) {
  let t = vu(e);
  if (t.length <= ks) return t;
  return `${t.slice(0, ks)}-${Math.abs(hash(e)).toString(36)}`;
}
function Rs(e) { return c0() ?? Jf(e); }          // c0() = CLAUDE_CODE_PROJECT_DIR_NAME
function wu(e) { return join(Gt(), "projects", Rs(e)); }
```

Verified live: cwd `/private/tmp/.../scratchpad/cwdA` produced the directory
`-private-tmp-...-scratchpad-cwdA` (dots and slashes all become `-`; the
leading `/` becomes a leading `-`). [live T1]

Notes:

- The mapping is lossy (`/a/b` and `/a-b` collide). The SDK guards against
  collisions when listing by checking the `cwd` of the first line (or a
  `relocated` record) against the requested dir. [sdk `Y4`, l.54523]
- The hash function is the CLI's internal string hash; the docs only say
  "appends a hash". For a >200-char slug Pingex should match the first 200
  chars as a prefix and then verify `cwd` from the file head, which is what
  the SDK does. [docs agent-sdk/sessions; sdk `er`, l.48667]
- `CLAUDE_CODE_PROJECT_DIR_NAME` (v2.1.234+) pins the directory name. It is
  **ignored unless `CLAUDE_CONFIG_DIR` is also set**, must match
  `^[A-Za-z0-9_-]{1,64}$` and not be a Windows device name, and is read only
  from the process environment (never from a settings `env` block).
  [docs env-vars; sdk `HE`, l.33510]

  ```js
  var tre = /^[A-Za-z0-9_-]{1,64}$/, nre = /^(?:con|prn|aux|nul|com[0-9]|lpt[0-9])$/i;
  var c0 = memo(() => (Lh() ? HE(a0()) : void 0));
  ```

  This is the intended mechanism for "a host that embeds Claude Code and
  gives each session its own config directory" [docs sessions]. If Pingex ever
  wants a per-workspace config dir it should use this pair rather than
  re-deriving slugs.

### 1.3 Per-session directory and sidecars

Beside `<sessionId>.jsonl` there may be a directory `<sessionId>/`: [docs claude-directory; disk]

| path | content |
|---|---|
| `<id>/subagents/agent-<agentId>.jsonl` | one transcript per subagent (sidechain), same line format, every line `isSidechain:true` + `agentId`. |
| `<id>/subagents/agent-<agentId>.meta.json` | `{"agentType": "...", "description": "..."}` (seen in files from 2.1.81-2.1.138; not present in 2.1.22x+ files). |
| `<id>/subagents/agent-<hex>prompt_suggestion-<hex>.jsonl` | prompt-suggestion sidechains (old versions). |
| `<id>/tool-results/<id>.txt\|json` | large tool outputs spilled out of the transcript. The in-transcript `tool_result` then contains a `<persisted-output>` notice with the absolute path. |
| `<id>/custom-title.json` | `{"customTitle": "..."}` - written by `/rename` (seen live). Read by the SDK only when no `customTitle` is found in the transcript tail. |

Other places under `<config>` that reference sessions:

| path | what it is | useful to Pingex? |
|---|---|---|
| `projects/<project>/memory/` | auto-memory; excluded from the retention sweep | no |
| `projects/<project>/sessions-index.json` | `{version:1, originalPath, entries:[{sessionId, fullPath, fileMtime, firstPrompt, summary, messageCount, created, modified, gitBranch, projectPath, isSidechain}]}`. Present only in directories last written by 2.1.81-2.1.138; **not written by 2.1.22x+** and **not read by the SDK** (zero references). Treat as legacy. [disk; sdk grep] |
| `sessions/<pid>.json` | one file per **running** session: `{pid, sessionId, cwd, startedAt, version, kind, entrypoint, name, nameSource, status, bridgeSessionId, messagingSocketPath, ...}`. Removed on exit. Good for "is this session live?" but not a history index. [docs claude-directory; disk] |
| `history.jsonl` | one line per typed prompt: `{display, pastedContents, timestamp, project, sessionId}`. `project` is the raw cwd. **Not written for `-p`/SDK sessions** (live check: 0 lines for the `-p` sessions created here). Up-arrow recall only. |
| `.claude.json` -> `projects[<raw cwd>].lastSessionId` | the most recent session per cwd, plus `lastSessionMetrics`, cost/duration counters. This is a per-project pointer, not a list. [disk] |
| `file-history/<sessionId>/<hash>@v<N>` | pre-edit file snapshots for `/rewind`; referenced from `file-history-snapshot` / `file-history-delta` transcript lines via `backupFileName`. [docs checkpointing; disk] |
| `session-env/<sessionId>/` | per-session env metadata (empty dirs observed). |
| `tasks/<sessionId>/<n>.json` | task-tool items `{id, subject, description, status, activeForm, blockedBy, blocks}`. |
| `plans/<slug>.md` | plan files; `<slug>` equals the `slug` field stamped on transcript records (three random words). |
| `todos/` | legacy, no longer written. [docs claude-directory] |

Retention: everything above except `sessions/` and `memory/` is swept after
`cleanupPeriodDays` (default 30, min 1). Pingex must not assume a session it
created last month is still on disk. [docs claude-directory]

## 2. Transcript line format

Each line is one JSON object with a `type`. Types observed on this machine
(all projects, 2.1.220-2.1.251 files, counts are lines): [disk]

```
13260 assistant      9707 attachment     8646 user        2041 ai-title
 2032 mode           2003 last-prompt    1785 permission-mode
 1074 agent-name      514 file-history-delta   492 atis-latch   441 system
  431 bridge-session  342 queue-operation      325 file-history-snapshot
   37 cost-state
```

Not observed here but known to the SDK: `summary`, `custom-title` (seen live
after `-n`/`/rename`), `tag`, `relocated`, `content-replacement`,
`history-suppression`, `attribution-snapshot`, `progress`, `agent_metadata`.
[sdk section 4 of the agent report; live T7]

### 2.1 Two kinds of line

**Chain records** carry `uuid`, `parentUuid`, `timestamp`, `sessionId`, `cwd`,
`version`, `gitBranch`, `isSidechain`, `userType`, `entrypoint` and (2.1.22x+)
usually `slug`. Types: `user`, `assistant`, `system`, `attachment`,
(`progress`). These form the conversation DAG.

**Bare state records** carry only `type`, `sessionId` and their payload - no
`uuid`/`parentUuid`/`timestamp` (except `queue-operation` and
`file-history-*`, which have a `timestamp`). They are "latest value wins"
key/value updates: `mode`, `permission-mode`, `ai-title`, `agent-name`,
`custom-title`, `last-prompt`, `atis-latch`, `bridge-session`, `cost-state`,
`queue-operation`, `file-history-snapshot`, `file-history-delta`.

### 2.2 Common envelope (chain records)

```json
{"parentUuid":"<uuid|null>","isSidechain":false,"type":"user",
 "message":{...},"uuid":"<uuid>","timestamp":"<ISO-8601>",
 "userType":"external","entrypoint":"cli","cwd":"<abs path>",
 "sessionId":"<uuid>","version":"2.1.251","gitBranch":"<branch>",
 "slug":"<three-word-slug>"}
```

- `entrypoint`: `cli` for interactive, `sdk-cli` for `claude -p`, `sdk-ts` /
  `sdk-py` for SDK hosts (the SDK sets `CLAUDE_CODE_ENTRYPOINT=sdk-ts`). The
  SDK's `includeProgrammatic:false` filter keys on this set. [live T1; sdk l.48198]
- `gitBranch` may be `HEAD` outside a repo. [live]
- Some records carry both `sessionId` and a duplicate `session_id`. [disk]
- Timestamps are **not** strictly monotonic within a file (15 out-of-order
  lines in a 1,643-line file). Order by chain, not by timestamp. [disk]

### 2.3 `user` records

`message: {role:"user", content: string | Block[]}` where blocks are `text`,
`image` (`{type,source}`) or `tool_result`
(`{type, tool_use_id, content: string|Block[], is_error?}`). [disk]

Optional envelope flags seen: [disk]

| field | meaning |
|---|---|
| `isMeta: true` | synthetic/injected user message (e.g. `<local-command-caveat>`, `[Image: ...]` stubs, hook output). Often the **first** user line of a file (61 of 91 files). Skip for display. |
| `isCompactSummary: true` + `isVisibleInTranscriptOnly: true` | the post-compaction summary message. |
| `toolUseResult` | structured result of the tool the `tool_result` block answers (object, array or `"Error: ..."` string). Tool-specific keys, e.g. Bash `{stdout, stderr, interrupted, isImage, noOutputExpected}`; Edit `{filePath, oldString, newString, originalFile, structuredPatch, ...}`; Agent `{agentId, outputFile, status, ...}`. |
| `sourceToolAssistantUUID` | `uuid` of the `assistant` record whose `tool_use` this result answers (verified equal live). |
| `promptId` | groups all records produced by one user turn. |
| `permissionMode`, `origin: {kind:"human"\|"task-notification"}`, `promptSource: "typed"\|"queued"\|"system"` | how the prompt entered (2.1.24x+). |
| `imagePasteIds`, `thinkingMetadata`, `todos` | rare / internal. |

### 2.4 `assistant` records

`message` is the API response: `{id:"msg_...", model, role:"assistant",
content: Block[], stop_reason, stop_sequence, usage, ...}` with blocks
`text`, `thinking` (`{thinking, signature}`), `tool_use`
(`{id:"toolu_...", name, input, caller:{type:"direct"|...}}`), rarely
`fallback`. Envelope extras: `requestId`, `effort`, `attributionSkill`,
`attributionPlugin`, `attributionMcpServer/Tool`; on errors
`isApiErrorMessage: true` and `error`. [disk]

**One API response is written as several `assistant` lines** - roughly one per
content block - all sharing `message.id` and `requestId`, chained by
`parentUuid` (608 assistant lines, 258 distinct `message.id` in one file). A
reader that wants "one assistant message per response" must merge consecutive
assistant records with the same `message.id`. The SDK's `zje` does exactly
this re-attachment. [disk; live T8; sdk l.50159]

### 2.5 Tool call pairing

Observed live for a single Bash call (redacted): [live T8]

```
user        uuid=U1  parent=null        content:"<prompt>"
attachment  uuid=A1  parent=U1          deferred_tools_delta
attachment  uuid=A2  parent=A1          agent_listing_delta
attachment  uuid=A3  parent=A2          skill_listing
attachment  uuid=A4  parent=A3          total_tokens_reminder
assistant   uuid=S1  parent=A4  msg=M1  [thinking/text]
assistant   uuid=S2  parent=S1  msg=M1  [tool_use id=T1 name=Bash]
user        uuid=U2  parent=S2          [tool_result tool_use_id=T1]   sourceToolAssistantUUID=S2  toolUseResult={stdout,...}
attachment  uuid=A5  parent=U2          total_tokens_reminder
assistant   uuid=S3  parent=A5  msg=M2  [...]
assistant   uuid=S4  parent=S3  msg=M2  [text]                            stop_reason=end_turn
last-prompt                             leafUuid=S4
```

Pairing rule: `tool_result.tool_use_id` == `tool_use.id`. Additionally
`sourceToolAssistantUUID` points straight at the assistant record. Note that
**`attachment` records sit inside the parent chain** - the assistant's
`parentUuid` is the last attachment, not the user message - so a walker must
traverse through attachments rather than expect user->assistant adjacency.

### 2.6 `attachment` records

`{..envelope.., "attachment": {"type": "<kind>", ...}}`. 21 kinds observed:
`total_tokens_reminder` (dominant), `batching_reminder_sent`,
`bash_output_audience_note`, `task_reminder`, `skill_listing`,
`agent_listing_delta`, `deferred_tools_delta`, `plan_mode`, `plan_mode_exit`,
`edited_text_file`, `file`, `command_permissions`, `queued_command`,
`auto_mode`, `compact_file_reference`, `date_change`, `hook_system_message`,
`mcp_instructions_delta`, `plan_file_reference`, `silent_turn_reminder`,
`plan_mode_reentry`. These are system-reminder injections; the SDK parses them
only to keep the chain intact and then drops them from `getSessionMessages`.
Pingex should do the same. [disk; sdk l.50314]

### 2.7 `system` records

Chain records with `subtype` and `isMeta`: [disk]

| subtype | shape |
|---|---|
| `turn_duration` | `{durationMs, messageCount, pendingBackgroundAgentCount?}` after each turn |
| `local_command` | `{content, level}` - a slash command was run locally |
| `away_summary` | `{content}` |
| `informational` | `{content, level:"warning"}` |
| `model_refusal_fallback` | rare |
| `compact_boundary` | see 2.8 |

### 2.8 Compaction

A `system`/`compact_boundary` record has `parentUuid: null`,
`logicalParentUuid: <last pre-compaction uuid>` and
`compactMetadata: {trigger:"manual"|"auto", preTokens, postTokens,
cumulativeDroppedTokens, durationMs, preCompactDiscoveredTools,
preservedSegment:{headUuid, anchorUuid, tailUuid},
preservedMessages:{anchorUuid, uuids[], allUuids[]}}`. The next line is a
`user` record with `isCompactSummary:true`, `isVisibleInTranscriptOnly:true`,
`parentUuid` = the boundary's uuid, whose content is the summary. The
conversation continues from there. [disk]

Pre-compaction lines stay in the file. The SDK rewires the chain across the
boundary using `compactMetadata` (`$je`, l.50159) and, for files > 5 MiB,
skips everything before the last `compact_boundary` unless
`CLAUDE_CODE_DISABLE_PRECOMPACT_SKIP` is set (`Uje`, l.50361). For **resume**
Pingex needs nothing here - the CLI handles it. For **rendering history** it
can either show everything (full DAG) or show from the last boundary.

### 2.9 Subagents / sidechains

- The main file never contains `isSidechain:true` lines in 2.1.22x+ (14,514
  of 14,514 chain lines were `false`); sidechains live in
  `<id>/subagents/agent-<agentId>.jsonl`. [disk]
- Every line there has `isSidechain:true`, `agentId`, the parent's `sessionId`,
  and its own chain starting at `parentUuid:null`. [disk]
- Link from main transcript: the Agent tool's `toolUseResult.agentId` equals
  `<agentId>` (109/109 matched on this machine). [disk]
- Older files (2.1.81-2.1.138) sometimes also had sidechain lines inline;
  the SDK's `getSessionInfo` returns `undefined` if the **first line** of a
  file has `isSidechain:true`, and `getSessionMessages` drops sidechain
  lines. [sdk `Du`, l.54384]

### 2.10 Session-level state records

| type | payload | notes |
|---|---|---|
| `ai-title` | `{aiTitle}` | auto-generated title (Haiku), rewritten several times per session; last wins |
| `agent-name` | `{agentName}` | display name; set from `-n`, `/rename`, or auto |
| `custom-title` | `{customTitle}` | user-set title; written by `-n`/`/rename`/SDK `renameSession` |
| `summary` | `{summary, leafUuid}` | older format; SDK still reads the field |
| `last-prompt` | `{lastPrompt?, leafUuid}` | leaf of the chain at last turn; SDK uses `lastPrompt` as summary fallback |
| `mode` | `{mode:"normal"\|...}` | first line of every interactive file |
| `permission-mode` | `{permissionMode:"default"\|"plan"\|"auto"\|...}` | restored on terminal `--resume` (except bypass/plan) [docs sessions] |
| `queue-operation` | `{operation:"enqueue"\|"dequeue"\|"remove", content?, reason?, timestamp}` | prompt queue; `content` is the queued prompt text |
| `file-history-snapshot` | `{messageId, snapshot:{messageId, trackedFileBackups:{<path>:{backupFileName, version, backupTime, realParentDir}}, timestamp}, isSnapshotUpdate}` | `/rewind` checkpoint per user prompt |
| `file-history-delta` | `{messageId, snapshotMessageId, trackingPath, backup:{...}, timestamp}` | incremental checkpoint |
| `cost-state` | totals + `modelUsage` | |
| `bridge-session` | `{bridgeSessionId, lastSequenceNum, ownerAccountUuid, ownerOrganizationUuid}` | Remote Control / claude.ai bridge; account identifiers, do not surface |
| `atis-latch` | `{atis: "<opaque token>"}` | opaque; the SDK copies it verbatim on fork |
| `tag`, `relocated` (`relocatedCwd`), `content-replacement`, `history-suppression` | | written by SDK/CLI features not exercised here |

## 3. Reading a session (what to rebuild turns from)

Minimum algorithm, mirroring `getSessionMessages` [sdk l.50132-50361]:

1. Parse every line; keep `type in {user, assistant, system, attachment,
   progress}` that have a string `uuid`. Everything else is state.
2. Build `uuid -> record`. Leaves are uuids never referenced as a
   `parentUuid`. Prefer a leaf that is not `isSidechain`/`isMeta`; if several,
   take the one latest in file order (`last-prompt.leafUuid` is a strong hint
   for the current leaf). Walk `parentUuid` to the root and reverse.
   Multiple leaves happen on rewind/branch; the abandoned branch is still in
   the file.
3. Drop `attachment`, `progress`, `isMeta`, `isSidechain`; keep `system`
   only if wanted.
4. Merge consecutive `assistant` records sharing `message.id`.
5. Pair `tool_use.id` with `tool_result.tool_use_id`; use `toolUseResult`
   for a structured view and `<id>/tool-results/*` when the result text is a
   `<persisted-output>` stub.
6. Title precedence (SDK `Du`): last `customTitle` in tail -> sidecar
   `custom-title.json` -> `customTitle` in head -> `aiTitle` -> `lastPrompt`
   -> `summary` -> first non-meta, non-tool-result user text (truncated to
   200 chars). `createdAt` = first `timestamp` in the head; `lastModified` =
   file mtime; `cwd` = `relocated.relocatedCwd` else first `cwd` in head;
   `gitBranch` = last in tail.

The SDK only ever reads the first and last 64 KiB of a file for listing
(`Or = 65536`, l.48489) and uses raw substring scans (`"key":"value"`), not
JSON parsing, for these fields. A Rust port can do the same for a cheap
listing pass and full-parse on open.

## 4. Resume semantics

All [docs cli-reference, sessions] unless marked [live].

| flag | behaviour |
|---|---|
| `--resume <id>` | Searches current project dir + its git worktrees, then **every other project under `<config>/projects`** (v2.1.223+). Cross-project match must be unique or it reports not-found. Works from any cwd. [live T5: resumed from a different cwd; new lines appended to the **original** file with the new `cwd` value; the file did not move]. Accepts a name or title instead of an id. |
| `--continue` | Most recent session **for the current directory**, skipping `-p`/SDK, background and `/loop` sessions. `claude -p --continue` includes `-p`/SDK sessions. |
| `--fork-session` | With `--resume`/`--continue`: new session id. [live T6: new file in the **current cwd's** project dir; prior chain records are **copied** with their original uuids (11 shared uuids) and re-stamped with the new `sessionId` and `cwd`; state records rewritten]. The SDK `forkSession` (l.55096) instead remaps every uuid and adds `forkedFrom:{sessionId, messageUuid}` + a `custom-title` "<title> (fork)". |
| `--session-id <uuid>` | Use this id for a **new** session. [live T3: file `<uuid>.jsonl` created]. Re-using an existing id fails: `Error: Session ID <id> is already in use.` [live T4] |
| `--no-session-persistence` | `-p` only. Nothing written; `--resume <id>` afterwards prints `No conversation found with session ID: <id>` (exit 0, on stdout in text mode). [live T2] `CLAUDE_CODE_SKIP_PROMPT_HISTORY=1` does the same in any mode. |
| `-n/--name <name>` | Writes **both** `{"type":"custom-title","customTitle":<name>}` and `{"type":"agent-name","agentName":<name>}` at the top of the file. [live T7] Interactive sessions with a clashing live name get a two-word suffix. |
| `/rename <name>` (also in `-p`, v2.1.205+) | Appends a new `custom-title` + `agent-name` pair **and** writes `<id>/custom-title.json`. Resuming by the new name works. [live T7b/T7c] |
| SDK `renameSession` | Appends only `{"type":"custom-title","customTitle","sessionId"}` (no `agent-name`, no sidecar). [sdk l.54716] |

Not restored on resume: `--mcp-config`, `--settings`, `--plugin-dir`,
`--fallback-model`, `--add-dir` and mid-session `/add-dir`. Permission mode
is restored by terminal `--resume <id>`/`--continue` except `bypassPermissions`
and `plan`. Resuming one session in two processes interleaves writes into one
file. [docs sessions]

The SDK's `query()` maps options to flags 1:1: `resume` -> `--resume=<id>`,
`continue` -> `--continue`, `forkSession` -> `--fork-session`,
`sessionId` -> `--session-id=<uuid>`, `persistSession:false` ->
`--no-session-persistence`, `resumeSessionAt` -> `--resume-session-at=<uuid>`.
[sdk l.46154-46200] The `--resume-session-at` and `--resume-drops-turn` flags
are not in `claude --help` but are accepted (the SDK relies on them).

## 5. What the Agent SDK reads (for the Rust port)

`listSessions({dir?, limit?, offset?, includeWorktrees=true,
includeProgrammatic=true})` [sdk `J4`, l.54676]:

1. With `dir`: realpath it; if `includeWorktrees`, run
   `git -c core.hooksPath=/dev/null -c core.fsmonitor= worktree list --porcelain`
   and collect worktree paths; scan `projects/<slug(path)>` for each.
   Without `dir`: `readdir(<config>/projects)`, scan every subdirectory.
2. Per dir: `readdir`, keep `*.jsonl` whose basename matches
   `^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$`. Subagent
   files are in a subdirectory so never listed.
3. Per file: `stat` + read head/tail 64 KiB; drop if
   `includeProgrammatic:false` and `entrypoint` in `{sdk-cli, sdk-ts, sdk-py}`
   or `sessionKind` in `{daemon, daemon-worker}`; drop if first line is a
   sidechain; drop if the head `cwd` slugs to this dir but is a different real
   path; compute `SDKSessionInfo` as in section 3 step 6.
4. Sort by `lastModified` desc, then sessionId desc; dedupe by sessionId. With
   limit/offset: sort by mtime, process in batches of 32 until `limit` valid
   entries after skipping `offset`.

`getSessionInfo(id, {dir?})`: locate `<id>.jsonl` (given dir + worktrees, else
every project dir; must be non-empty) and compute the same struct; no
programmatic filter. [sdk `X4`, l.54692]

`getSessionMessages(id, {dir?, limit?, offset?, includeSystemMessages?})`:
section 3 steps 1-4, returning `{type, uuid, session_id, message,
parent_tool_use_id:null, parent_agent_id:null, timestamp}`. [sdk `Gq`, l.50361]

`SDKSessionInfo` type: [sdk.d.ts l.4863]

```ts
{ sessionId: string; summary: string; lastModified: number; fileSize?: number;
  customTitle?: string; firstPrompt?: string; gitBranch?: string; cwd?: string;
  tag?: string; createdAt?: number }
```

## 6. Answers

**(a) Resume a session Pingex created.** Spawn with `--session-id <uuid>` so
Pingex owns the id (or read `session_id` from the `init`/`result` stream
message). Persist `<uuid>`, the cwd, and the `CLAUDE_CONFIG_DIR` used. To
resume, spawn `claude --resume <uuid>` with the same config dir - cwd may
differ (2.1.223+) but the process cwd still becomes the new `cwd` stamped on
subsequent lines, so pass the original cwd for sane tool behaviour. Do not
use `--no-session-persistence` for anything that should be resumable. Expect
the file to be gone after `cleanupPeriodDays`; detect the
`No conversation found with session ID` failure and fall back to a fresh
session. Set a title with `-n` (writes `custom-title` + `agent-name`) or by
appending a `custom-title` line yourself exactly as the SDK does.

**(b) Import a session Pingex didn't create.** Enumerate
`<config>/projects/*/<uuid>.jsonl` (skip `subagents/`), read head/tail to get
`cwd`, `entrypoint`, `gitBranch`, titles and first prompt, `stat` for mtime
and size. Decide whether to show `-p`/SDK sessions (the CLI picker hides
them). To render history, walk the `parentUuid` chain from the `last-prompt`
leaf (or computed leaf), merge assistant chunks by `message.id`, pair tool
calls by id, treat `attachment`/`isMeta`/`compact_boundary` as non-display,
and resolve `<persisted-output>` stubs from `<id>/tool-results/`. Resume it
with `--resume <uuid>` from the cwd recorded in the file (use
`relocated.relocatedCwd` if present, else the first `cwd`).

## 7. Stable vs internal

Reasonably stable (documented, or load-bearing for the SDK's own reader):
file layout (`projects/<slug>/<uuid>.jsonl`, `<uuid>/subagents/`,
`<uuid>/tool-results/`), slug rule, `CLAUDE_CONFIG_DIR`,
`CLAUDE_CODE_PROJECT_DIR_NAME`; on records: `type`, `uuid`, `parentUuid`,
`timestamp`, `sessionId`, `cwd`, `gitBranch`, `entrypoint`, `isSidechain`,
`isMeta`, `isCompactSummary`, `message` (Anthropic Messages API shapes),
`customTitle`, `aiTitle`, `lastPrompt`, `summary`, `tag`, `relocated`,
`compact_boundary` + `compactMetadata`, `agentId`.

Internal / drifting (changed across the 2.1.8x-2.1.25x files on this machine):
`sessions-index.json` (gone), `*.meta.json` (gone), `summary` records
(replaced by `ai-title`), inline sidechains (moved to `subagents/`), the
`attachment` kinds, `bridge-session`, `atis-latch`, `cost-state`,
`queue-operation`, `file-history-*`, `promptId`/`origin`/`promptSource`,
`slug`, `session_id` duplicate, `effort`, `attribution*`, `toolUseResult`
key sets, `version`.

## 8. Unverified

- The exact base36 hash used for >200-char slugs (SDK `yA`); only its use
  is quoted. Prefix-match plus `cwd` verification avoids needing it.
- Behaviour of `--resume` when the same id exists in two project dirs
  (docs say not-found; not reproduced).
- Whether `/cd` writes a `relocated` record (SDK reads it; not observed).
- `resumeSessionAt` / `--resume-session-at` behaviour on disk.
- Interactive-only `mode`/`bridge-session` first lines were only observed,
  not documented; `-p` files start with `queue-operation` or `custom-title`.
