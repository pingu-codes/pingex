# ACP schema as Pingex's neutral model (PIN-6)

Research note, 2026-08-29. Question: which exact Agent Client Protocol (ACP)
types should Pingex's internal, harness-neutral event and item model mirror?

## Sources and pinned revisions

All claims below were checked against these primary sources on 2026-08-29.
Line references are into these revisions.

| Source | Revision |
| --- | --- |
| `agentclientprotocol/agent-client-protocol` (spec, schema, docs) | `main` @ `3ea1fdee59ff06df173e065036af3529af9dcb98` (2026-08-29); latest release tag `schema-v1.21.0` (2026-08-20); Rust crate CHANGELOG top entry 1.7.0 |
| `agentclientprotocol/claude-agent-acp` | `main` @ `c3ff3438844f5249d6a7f5c297906e2cd3d5fa7f` (2026-08-28); release v0.70.0; depends on `@agentclientprotocol/sdk` 1.3.0 (`package.json`) |
| `agentclientprotocol/codex-acp` | `main` @ `69ca755d9878238aecf0737c0e4568b3bab37be2` (2026-08-28); release v1.7.0; depends on `@agentclientprotocol/sdk` ^1.4.0 (`package.json`) |

Schema paths cited as `schema/v1/schema.json#/$defs/<Name>` are in the spec
repo. Doc pages cited as `docs/protocol/v1/<page>.mdx` are the sources of
<https://agentclientprotocol.com/protocol/v1/<page>>.

## Headline finding: mirror ACP **v1**, and watch v2

ACP now ships two schema generations side by side:

- **v1** (`schema/v1/schema.json`) is the stable, released protocol. Both
  official adapters answer `initialize` with `protocolVersion: 1`
  (`claude-agent-acp/src/acp-agent.ts:1705`, `codex-acp/src/CodexAcpServer.ts:323`
  via `acp.PROTOCOL_VERSION`), and everything the ticket asks about
  (`session/load`, `session/set_mode`, `current_mode_update`, `tool_call` +
  `tool_call_update`, `diff{oldText,newText}`) is v1 vocabulary.
- **v2** (`schema/v2/schema.json`) is a draft consolidation: "GitHub releases
  for this schema are not published yet" (`docs/protocol/v2/schema.mdx`), and
  the migration guide says to gate v2 "behind explicit version negotiation and
  feature flags until it stabilizes" (`docs/protocol/v2/migration.mdx`).

v2 changes the model in ways Pingex should anticipate (all from
`docs/protocol/v2/migration.mdx`):

| v1 | v2 |
| --- | --- |
| `session/prompt` response carries `stopReason` and ends the turn | Response is an ack (`{}`); stop reason moves into a new `state_update` (`running` / `idle` / `requires_action`) |
| `tool_call` creates, `tool_call_update` patches | `tool_call` removed; first `tool_call_update` for an id creates (upsert: omit = unchanged, `null` = cleared, value = replaced) |
| `plan` (whole entries array) | `plan_update` with `planId` and `type` discriminator |
| `current_mode_update` + `session/set_mode` | Removed; modes are config options (`config_option_update`) |
| `diff{path,oldText,newText}` | `diff{changes:[{operation,path,oldPath?,fileType?,mimeType?}], patch?:{format:"git_patch",text}}` |
| `session/load` | Removed; `session/resume` with `replayFrom:{type:"start"}` |
| `request_permission{toolCall}` | `request_permission{title, description?, subject?:{type:"tool_call"|"command",…}}` |
| `agentCapabilities`/`clientCapabilities` booleans | one `capabilities` + required `info`; support markers are objects |
| `fs/*`, `terminal/*` client methods | Removed; agent-owned display terminals via `terminal_update` / `terminal_output_chunk` |
| `ToolCallStatus` 4 values | adds `cancelled` (CHANGELOG 1.6.0, "unstable-v2") |

**Recommendation.** Pingex's neutral model should be the v1 `session/update`
union plus `RequestPermissionRequest`, with two deliberate v2-isms baked in:
(1) treat every `tool_call_update` as an upsert keyed by `toolCallId` (also
what both adapters actually rely on), and (2) carry `stopReason` as a
turn-level event rather than a request result. That keeps a v2 transport
adapter a pure rename job later.

## 1. Method surface (v1)

From `schema/v1/meta.json` (stable) and `schema/v1/meta.unstable.json`
(unstable additions marked *).

Agent methods (client calls agent): `initialize`, `authenticate`,
`session/new`, `session/load`, `session/set_mode`,
`session/set_config_option`, `session/prompt`, `session/cancel`
(notification), `session/list`, `session/delete`, `session/resume`,
`session/close`, `logout`; unstable*: `session/fork`, `providers/list`,
`providers/set`, `providers/disable`, `mcp/message`, `nes/*`, `document/*`.

Client methods (agent calls client): `session/request_permission`,
`session/update` (notification), `fs/read_text_file`, `fs/write_text_file`,
`terminal/create`, `terminal/output`, `terminal/release`,
`terminal/wait_for_exit`, `terminal/kill`, `elicitation/create`,
`elicitation/complete`; unstable*: `mcp/connect`, `mcp/message`,
`mcp/disconnect`.

Protocol-level: `$/cancel_request`.

Conventions (`docs/protocol/v2/overview.mdx`, same in v1 practice): object
keys are `camelCase`; discriminator string values are `snake_case`; all file
paths are absolute; line numbers are 1-based.

## 2. `session/update` (`SessionNotification`)

`schema/v1/schema.json#/$defs/SessionNotification`:

```json
{ "sessionId": "SessionId", "update": "SessionUpdate", "_meta"?: object|null }
```

`SessionUpdate` is a `oneOf` discriminated on `sessionUpdate`. Variants in
the **stable** v1 schema (`#/$defs/SessionUpdate`, checked against
`schema/v1/schema.json`):

```
user_message_chunk | agent_message_chunk | agent_thought_chunk
| tool_call | tool_call_update | plan
| available_commands_update | current_mode_update | config_option_update
| session_info_update | usage_update
```

Additional variants only in `schema/v1/schema.unstable.json` (marked
`**UNSTABLE**` in their descriptions): `plan_update`, `plan_removed`,
`notice`, `compaction_update`, `compaction_summary_chunk`.

Each variant is `{ "sessionUpdate": "<const>" } allOf <payload>`; the payload
shapes follow. `_meta` is `object | null` on every type and is omitted below
unless it matters.

### 2.1 Message chunks (`ContentChunk`)

`#/$defs/ContentChunk`; used verbatim by `user_message_chunk`,
`agent_message_chunk`, `agent_thought_chunk`:

```json
{
  "sessionUpdate": "agent_message_chunk",
  "content": ContentBlock,          // required
  "messageId": "MessageId" | null,  // optional in v1; required in v2
  "_meta": object | null
}
```

`ContentBlock` (`#/$defs/ContentBlock`, MCP-compatible) is `oneOf` on `type`:

```json
{ "type": "text", "text": string, "annotations"?: Annotations }
{ "type": "image", "data": string, "mimeType": string, "uri"?: string, "annotations"?: … }
{ "type": "audio", "data": string, "mimeType": string, "annotations"?: … }
{ "type": "resource_link", "uri": string, "name": string, "title"?: string, "mimeType"?: string, "size"?: int, "annotations"?: … }
{ "type": "resource", "resource": { "uri": string, "text": string, "mimeType"?: string } | { "uri": string, "blob": string, "mimeType"?: string } }
```

(`TextContent`, `ResourceLink`, `EmbeddedResource` required fields verified
from `#/$defs/TextContent` (`text`), `#/$defs/ResourceLink` (`name`, `uri`),
`#/$defs/EmbeddedResource` (`resource`); image/audio fields from the
`ContentBlock` doc page.)

### 2.2 `tool_call` (`ToolCall`)

`#/$defs/ToolCall`, required `toolCallId`, `title`:

```json
{
  "sessionUpdate": "tool_call",
  "toolCallId": "ToolCallId",
  "title": string,
  "kind"?: ToolKind,                 // default "other"
  "status"?: ToolCallStatus,         // default "pending"
  "content"?: ToolCallContent[],
  "locations"?: ToolCallLocation[],
  "rawInput"?: any,
  "rawOutput"?: any,
  "name"?: string | null,            // UNSTABLE only (CHANGELOG 1.6.0 "add tool call name"); absent from stable schema.json
  "_meta"?: object | null
}
```

### 2.3 `tool_call_update` (`ToolCallUpdate`)

`#/$defs/ToolCallUpdate`, required `toolCallId` only; "All fields except the
tool call ID are optional - only changed fields need to be included."

```json
{
  "sessionUpdate": "tool_call_update",
  "toolCallId": "ToolCallId",
  "kind"?: ToolKind | null,
  "status"?: ToolCallStatus | null,
  "title"?: string | null,
  "name"?: string | null,            // UNSTABLE
  "content"?: ToolCallContent[] | null,
  "locations"?: ToolCallLocation[] | null,
  "rawInput"?: any,
  "rawOutput"?: any,
  "_meta"?: object | null
}
```

### 2.4 `ToolCallStatus` lifecycle

`#/$defs/ToolCallStatus`: `"pending" | "in_progress" | "completed" | "failed"`.
Meanings (`docs/protocol/v1/tool-calls.mdx` "Status"):

- `pending` — hasn't started because input is streaming or awaiting approval
- `in_progress` — running
- `completed` — finished successfully
- `failed` — failed with an error

There is no `cancelled` status in v1. On `session/cancel` the *client*
"SHOULD preemptively mark all non-finished tool calls pertaining to the
current turn as cancelled" and "MUST respond to all pending
`session/request_permission` requests with the `cancelled` outcome"
(`docs/protocol/v1/prompt-turn.mdx` "Cancellation"). v2 adds a real
`cancelled` status (CHANGELOG 1.6.0).

### 2.5 `ToolKind`

`#/$defs/ToolKind`:
`"read" | "edit" | "delete" | "move" | "search" | "execute" | "think" | "fetch" | "switch_mode" | "other"`.
"Tool kinds help clients choose appropriate icons and optimize how they
display tool execution progress."

### 2.6 `ToolCallContent`

`#/$defs/ToolCallContent`, `oneOf` on `type`:

```json
{ "type": "content", "content": ContentBlock }
{ "type": "diff", "path": string, "oldText": string | null, "newText": string, "_meta"?: … }   // required: path, newText
{ "type": "terminal", "terminalId": "TerminalId", "_meta"?: … }
```

`Diff` (`#/$defs/Diff`): `oldText: null` means a new file; both adapters use
`oldText: null` for creates (`claude-agent-acp/src/tools.ts:205-210`,
`codex-acp/src/CodexToolCallMapper.ts` `createAddFileContent`) and
`newText: ""` for deletes (codex-acp `createDeleteFileContent`). `Terminal`
"Embed[s] a terminal created with `terminal/create` by its id" — in v1 this
refers to a *client-created* terminal; both adapters instead reuse the tool
call id as `terminalId` and push output through `_meta` (see §7).

### 2.7 `ToolCallLocation`

`#/$defs/ToolCallLocation`: `{ "path": string, "line"?: uint32 | null, "_meta"?: … }`
(required: `path`). Purpose: "follow-along" features
(`docs/protocol/v1/tool-calls.mdx` "Following the Agent").

### 2.8 `plan` (`Plan`)

`#/$defs/Plan` → `{ "sessionUpdate": "plan", "entries": PlanEntry[] }` (required).
`PlanEntry`: `{ "content": string, "priority": "high"|"medium"|"low", "status": "pending"|"in_progress"|"completed" }`
(all three required). Each `plan` replaces the whole list. Unstable `plan_update`
carries `{ "plan": PlanUpdateContent }` where content is `type: "markdown"`
(codex-acp emits `{type:"markdown", planId, content}` in
`CodexEventHandler.ts:861`) or `type: "items"`; `plan_removed` carries
`{ "planId" }`; both gated on `clientCapabilities.plan` (`#/$defs/PlanCapabilities`).

### 2.9 `available_commands_update`

`#/$defs/AvailableCommandsUpdate` → `{ "availableCommands": AvailableCommand[] }`.
`AvailableCommand`: `{ "name": string, "description": string, "input"?: { "hint": string } | null }`
(required `name`, `description`; input is the `unstructured` variant, `#/$defs/UnstructuredCommandInput`).

### 2.10 `current_mode_update`

`#/$defs/CurrentModeUpdate` → `{ "currentModeId": "SessionModeId" }`.
Deprecated in favour of config options (`docs/protocol/v1/session-modes.mdx`
note); removed in v2.

### 2.11 `config_option_update`

`#/$defs/ConfigOptionUpdate` → `{ "configOptions": SessionConfigOption[] }`.
"This notification also contains the complete configuration state"
(`docs/protocol/v1/session-config-options.mdx`). Shape of
`SessionConfigOption` in §4.

### 2.12 `usage_update`

`#/$defs/UsageUpdate` (stable):

```json
{ "sessionUpdate": "usage_update", "used": uint64, "size": uint64, "cost"?: { "amount": double, "currency": string } | null }
```

`used`/`size` are context-window tokens. Per-turn `Usage`
(`totalTokens`, `inputTokens`, `outputTokens`, `thoughtTokens?`,
`cachedReadTokens?`, `cachedWriteTokens?`) is **UNSTABLE** and only appears on
`PromptResponse.usage`.

### 2.13 `session_info_update`

`#/$defs/SessionInfoUpdate`: `{ "title"?: string | null, "updatedAt"?: string | null, "_meta"?: … }`
— all optional, partial update.

### 2.14 Stop reasons (`PromptResponse`)

`#/$defs/PromptResponse` (result of `session/prompt`):
`{ "stopReason": StopReason, "usage"?: Usage | null }`.
`#/$defs/StopReason`: `"end_turn" | "max_tokens" | "max_turn_requests" | "refusal" | "cancelled"`
(`docs/protocol/v1/prompt-turn.mdx` "Stop Reasons": end_turn = model finished
without requesting more tools; max_tokens; max_turn_requests = too many model
requests in one turn; refusal = agent refuses to continue; cancelled = client
cancelled). claude-agent-acp emits all but `max_turn_requests`
(`grep stopReason` in `src/`: cancelled, end_turn, max_tokens, refusal);
codex-acp emits only `end_turn` and `cancelled` (`CodexAcpServer.ts:2584-2922`).

## 3. `session/request_permission`

`#/$defs/RequestPermissionRequest` (required: `sessionId`, `toolCall`, `options`):

```json
{
  "sessionId": "SessionId",
  "toolCall": ToolCallUpdate,        // same upsert shape as tool_call_update
  "options": PermissionOption[],
  "_meta"?: object | null
}
```

`PermissionOption` (`#/$defs/PermissionOption`, all required):

```json
{ "optionId": "PermissionOptionId", "name": string, "kind": "allow_once" | "allow_always" | "reject_once" | "reject_always", "_meta"?: … }
```

`#/$defs/RequestPermissionResponse` → `{ "outcome": RequestPermissionOutcome }`:

```json
{ "outcome": "cancelled" }
{ "outcome": "selected", "optionId": "PermissionOptionId" }
```

`optionId` values are agent-defined; the `kind` is the only portable
semantic. Adapter option ids, for reference:

- claude-agent-acp (`src/permissions/options/shared.ts:4-17`): `allow-once`,
  `allow-with-updates` (kind `allow_always`), `allow-skill-exact`,
  `allow-skill-prefix`, `exit-plan-{bypass,auto,accept-edits,default,clear-auto,clear-bypass,clear-accept-edits}`, `reject`.
  Options are built per tool (`src/permissions/options.ts`) and sorted
  allow_once → allow_always → reject.
- codex-acp (`src/permissions/option-ids.ts`): `allow_once`,
  `allow_for_session`, `decline`, `cancel`, `accept_execpolicy_amendment`,
  `apply_network_policy_amendment:<n>`, `allow_permissions_turn`,
  `allow_permissions_turn_strict_auto_review`, `allow_permissions_session`,
  `reject_permissions`; MCP: `allow_once`, `allow_session`, `allow_always`,
  `decline`, `cancel`. Options derive from the app-server's
  `availableDecisions` (`src/permissions/options.ts` `commandDecisionOptions`).
  Plan review is a `kind: "switch_mode"` tool call with
  `implement`/`revise` options and `_meta.codex.kind = "plan_review"`
  (`src/permissions/plan-review.ts`).

## 4. Config options, modes, models, effort

### 4.1 `SessionConfigOption`

`#/$defs/SessionConfigOption`; base required `id`, `name`, plus a `type`
discriminator:

```json
{
  "id": "SessionConfigId", "name": string, "description"?: string,
  "category"?: "mode" | "model" | "model_config" | "thought_level" | string,
  "type": "select", "currentValue": "SessionConfigValueId",
  "options": SessionConfigSelectOption[] | SessionConfigSelectGroup[]    // "Ungrouped" | "Grouped"
}
{ "id", "name", "description"?, "category"?, "type": "boolean", "currentValue": boolean }
```

`SessionConfigSelectOption`: `{ "value": "SessionConfigValueId", "name": string, "description"?: string, "_meta"?: … }`.
`SessionConfigSelectGroup`: `{ "group": "SessionConfigGroupId", "name": string, "options": SessionConfigSelectOption[] }`.
`boolean` options may only be sent when the client advertised
`clientCapabilities.session.configOptions.boolean: {}`
(`docs/protocol/v1/session-config-options.mdx` "Boolean Config Options").
Categories are "for UX purposes only and MUST NOT be required for
correctness"; `_`-prefixed categories are free for custom use, others
reserved (`#/$defs/SessionConfigOptionCategory` description). Array order is
the agent's display priority.

### 4.2 `session/set_config_option`

`#/$defs/SetSessionConfigOptionRequest`: `{ "sessionId", "configId": "SessionConfigId", "value": "SessionConfigValueId" }`
or `{ "sessionId", "configId", "type": "boolean", "value": boolean }`.
Response `#/$defs/SetSessionConfigOptionResponse`: `{ "configOptions": SessionConfigOption[] }`
— always the complete state.

### 4.3 Modes (legacy)

`#/$defs/SessionModeState`: `{ "currentModeId": "SessionModeId", "availableModes": SessionMode[] }`;
`SessionMode`: `{ "id", "name", "description"?, "_meta"? }`.
`session/set_mode` (`#/$defs/SetSessionModeRequest`): `{ "sessionId", "modeId" }` → `{}`.
Spec guidance: if `configOptions` is present clients "SHOULD use them instead
of the `modes` field. Modes will be removed in a future version"; agents
should send both and keep them in sync
(`docs/protocol/v1/session-config-options.mdx` "Relationship to Session Modes").

### 4.4 Models and effort are config options, not schema types

There is no `models`/`set_model` in the v1 schema. `session/set_model` is a
**legacy codex-acp extension** (`LEGACY_SET_SESSION_MODEL_METHOD = "session/set_model"`,
`codex-acp/src/AcpExtensions.ts`), with `models?: {availableModels, currentModelId}`
grafted onto `NewSessionResponse` as `LegacyNewSessionResponse`. Both adapters
now expose them as config options:

| Concept | claude-agent-acp (`src/acp-agent.ts:7728-7800`, `src/session-config-ids.ts`, `src/session-mode.ts`) | codex-acp (`src/AgentMode.ts`, `src/ModelConfigOption.ts`, `src/CollaborationModeConfig.ts`, `src/FastModeConfig.ts`) |
| --- | --- | --- |
| Mode | `id: "mode"`, `category: "mode"`, values = Claude permission modes (`default`, `acceptEdits`, `plan`, `auto`, `dontAsk`, `bypassPermissions`); option `_meta.kind` ∈ `standard`/`plan`/`auto_review`/`full_access` | `id: "mode"`, `category: "mode"`, values `read-only` / `agent` / `agent-full-access`; option `_meta.kind` ∈ `standard`/`auto_review`/`full_access`/`plan`; each maps to an approval policy + sandbox policy |
| Model | `id: "model"`, `category: "model"`, values = SDK model ids incl. `default` | `id: "model"`, `category: "model"`, values = app-server `Model.id` |
| Effort | `id: "effort"`, `category: "thought_level"`, values `default` + `supportedEffortLevels` for the current model; only present when the model `supportsEffort` | `id: "reasoning_effort"`, `category: "thought_level"`, values from `ReasoningEffortOption` |
| Fast mode | `id: "fast-mode"`, `category: "model_config"`, boolean if client supports it else select `on`/`off` | identical shape (`FAST_MODE_CONFIG_ID = "fast-mode"`) |
| Collaboration mode | — | `id: "collaboration_mode"`, `category: "collaboration_mode"` (non-spec, non-underscore category), values `default`/`plan` |
| Agent (subagent persona) | `id` via `DEFAULT_AGENT_ID = "default"` | — |

Both adapters also feed `modes` (legacy) alongside config options.

## 5. `initialize` and the session lifecycle family

### 5.1 `initialize`

`#/$defs/InitializeRequest`: `{ "protocolVersion": uint16, "clientCapabilities"?: ClientCapabilities, "clientInfo"?: Implementation }`.
`#/$defs/ClientCapabilities`:

```json
{
  "fs"?: { "readTextFile": bool, "writeTextFile": bool },
  "terminal"?: bool,
  "session"?: { "configOptions"?: { "boolean"?: {} }, … },   // ClientSessionCapabilities
  "plan"?: {},              // UNSTABLE: receive plan_update/plan_removed
  "auth"?: { "terminal": bool },
  "elicitation"?: {…},
  "nes"?, "positionEncodings"?,                                // UNSTABLE
  "_meta"?: object | null
}
```

`#/$defs/InitializeResponse`: `{ "protocolVersion", "agentCapabilities"?, "authMethods"?: AuthMethod[], "agentInfo"?: Implementation, "_meta"? }`.
`#/$defs/AgentCapabilities`:

```json
{
  "loadSession"?: bool,                                       // gates session/load
  "promptCapabilities"?: { "image": bool, "audio": bool, "embeddedContext": bool },
  "mcpCapabilities"?: { "http": bool, "sse": bool, "acp": bool },
  "sessionCapabilities"?: { "list"?: {}, "delete"?: {}, "additionalDirectories"?: {}, "fork"?: {}, "resume"?: {}, "close"?: {} },
  "auth"?: { "logout"?: {} },
  "providers"?: {},                                           // UNSTABLE
  "nes"?, "positionEncoding"?,                                // UNSTABLE
  "_meta"?: object | null
}
```

`Implementation`: `{ "name": string, "version": string, "title"?: string }`.
Baseline: "all Agents MUST support `session/new`, `session/prompt`,
`session/cancel`, and `session/update`"; `session/load` is gated by the
top-level `loadSession` ("will be unified in future versions")
(`#/$defs/SessionCapabilities` description).

What the adapters advertise:

- claude-agent-acp (`src/acp-agent.ts:1694-1731`): `loadSession: true`,
  prompt `image`+`embeddedContext`, MCP `http`+`sse`, `auth.logout`,
  `providers: {}`, session `additionalDirectories`, `close`, `delete`, `fork`,
  `list`, `resume`, and a non-spec `subagents: {}`;
  `agentCapabilities._meta.claudeCode.promptQueueing: true`.
- codex-acp (`src/CodexAcpServer.ts:313-345`): `loadSession: true`, prompt
  `image`+`embeddedContext`, MCP `http` only, `auth.logout`, `providers: {}`,
  session `resume`, `list`, `close`, `delete`, `fork`, `additionalDirectories`,
  `subagents: {}`.

### 5.2 `session/new` / `load` / `resume` / `list` / `fork`

| Method | Request (`#/$defs/…Request`) | Response |
| --- | --- | --- |
| `session/new` | `{ cwd, mcpServers: McpServer[], additionalDirectories?: string[] }` (required `cwd`, `mcpServers`) | `{ sessionId, modes?: SessionModeState, configOptions?: SessionConfigOption[] }` |
| `session/load` | `{ sessionId, cwd, mcpServers, additionalDirectories? }` (required all but `additionalDirectories`) — agent replays history as `session/update`s before responding | `{ modes?, configOptions? }` |
| `session/resume` | `{ sessionId, cwd, additionalDirectories?, mcpServers? }` — "without returning previous messages (unlike `session/load`)" | `{ modes?, configOptions? }` |
| `session/list` | `{ cwd?: string, cursor?: string }` | `{ sessions: SessionInfo[], nextCursor?: string }`; `SessionInfo = { sessionId, cwd, additionalDirectories?, title?, updatedAt? }` |
| `session/fork` (UNSTABLE) | `{ sessionId, cwd, additionalDirectories?, mcpServers? }` | `{ sessionId, modes?, configOptions? }` — "new session based on the context of an existing one" |
| `session/close`, `session/delete` | `{ sessionId }` | `{}` |

`session/prompt` request: `{ sessionId, prompt: ContentBlock[] }`;
`session/cancel` notification: `{ sessionId }`.

## 6. Adapter mapping tables

### 6.1 claude-agent-acp: Claude tool → ACP tool call

`src/tools.ts` `toolInfoFromToolUse` (lines 131-493) sets `title`, `kind`,
initial `content`, `locations`; `toolUpdateFromToolResult` (577-919) and the
PostToolUse hook (`toolUpdateFromDiffToolResponse`, 1298-1337) fill in results.

| Claude tool | `kind` | Initial `content` | `locations` | Result handling |
| --- | --- | --- | --- | --- |
| `Agent` / `Task` | `think` | `content:text` = prompt | — | structured `AgentOutput.content` when completed, else raw text with `<usage>`/`agentId` trailer stripped |
| `Bash` | `execute` | `terminal{terminalId: toolUse.id}` if client `_meta.terminal_output`, else `content:text` = description | — | terminal: `_meta.terminal_info/terminal_output/terminal_exit`; fallback: ```` ```console ```` block |
| `Read` | `read` | `[]` | `{path, line: offset ?? 1}` | line-numbered text rebuilt from structured `FileReadOutput`, wrapped in a fenced block |
| `Write` | `edit` | `diff{path, oldText: null, newText: content}` | `{path}` | `{}` (diff comes from PostToolUse hook `structuredPatch` → one `diff` + `location{line:newStart}` per hunk) |
| `Edit` | `edit` | `diff{path, oldText: old_string ?? null, newText: new_string}` | `{path}` | same hook path as Write |
| `Glob` | `search` | `[]` | `{path}` if given | raw content |
| `Grep` | `search` | `[]` | — | raw content; title is a synthesized `grep …` command line |
| `WebFetch` | `fetch` | `content:text` = prompt | — | raw content |
| `WebSearch` | `fetch` | `[]` | — | `Title (url)` lines from `WebSearchOutput` |
| `TodoWrite` | `think` | `[]` | — | also drives `plan` (`planEntries`, priority always `medium`) |
| `TaskCreate` / `TaskUpdate` / `TaskList` / `TaskGet` | `think` | `[]` | — | maintain `TaskState` → `plan` entries |
| `ReportFindings` | `think` | one `content:text` per finding | — | raw |
| `ExitPlanMode` | `switch_mode` | `content:text` = plan | — | title → "Exited Plan Mode"; permission options choose the next mode |
| `Skill` | `other` | `[]` | — | `{}` |
| `AskUserQuestion` | `other` | `content:text` per question | — | routed to `elicitation/create`, never permission options |
| MCP / unknown | `other` | `[]` (or JSON dump for `"Other"`) | — | raw content |

Error results (`is_error`) render as fenced text (`toAcpContentBlock`).
Thinking blocks become `agent_thought_chunk`; text becomes
`agent_message_chunk`; context-window usage becomes `usage_update` (with
`cost` from `total_cost_usd` on result messages, `acp-agent.ts:4021-4029`).

### 6.2 codex-acp: app-server item → ACP tool call

`src/CodexEventHandler.ts` `createItemEvent` (686-728) / `completeItemEvent`
(731-790) dispatch to `src/CodexToolCallMapper.ts`.

| app-server `ThreadItem.type` / notification | `kind` | `title` | `content` | `locations` | `_meta` |
| --- | --- | --- | --- | --- | --- |
| `fileChange` | `edit` | "Editing files" | one `diff` per change: add → `{oldText:null,newText:<file>}`, update → old/new computed by applying the unified diff to the file on disk, delete → `{oldText:<file>,newText:""}`; `_meta.kind` = add/update/delete | — | — |
| `commandExecution` with single `commandActions[0].type = read` | `read` | `Read file '<path>'` | — | `{path}` | — |
| … `search` | `search` | `Search for '<q>' in <path>` | — | — | — |
| … `listFiles` | `read` | `List files in '<path>'` | — | — | — |
| … `unknown` or multiple actions | `execute` | command (shell prefix stripped) | `terminal{terminalId: item.id}` | — | `terminal_info{cwd,terminal_id}`; on completion `terminal_output` / `terminal_output_delta` + `terminal_exit`; `rawOutput{formatted_output, exit_code}` |
| `mcpToolCall` | `execute` | `mcp.<server>.<tool>` | — | — | `is_mcp_tool_call: true`; `rawInput{server,tool,arguments}`, `rawOutput{result,error}` |
| `dynamicToolCall` | `execute` | tool name | — | — | `rawInput{arguments}` |
| `webSearch` | `search` | `Web search: <q>` / `Open page: <url>` / `Find in page…` | — | — | — |
| `imageView` | `read` | `View Image <path>` | `content:resource_link` | `{path}` | — |
| `imageGeneration` | `other` | "Image generation" | `content:text` (revised prompt) + `content:image` | — | — |
| `contextCompaction` | `think` | "Compact conversation" | — | — | `contextCompaction{version:1,…}` |
| `collabAgentToolCall` | `other` | tool name | — | — | `codex.collaboration{tool,senderThreadId,receiverThreadIds}` (legacy; newer path emits `subagent_spawned`) |
| `subAgentActivity` | `other` | `Start/Interact with/Interrupt subagent <name>` | — | — | `codex.subagent{threadId,path,activity}` |
| `item/autoApprovalReview/*` (guardian) | `think` | "Guardian Review" | `content:text` status lines | — | id `guardian_assessment:<reviewId>` |
| `fuzzyFileSearch/*` | `search` | `Search for '<q>'` | — | one per file | id `fuzzyFileSearch.<sessionId>` |
| `agentMessage` delta | — | `agent_message_chunk` | | | |
| `reasoning` delta / completed | — | `agent_thought_chunk` (summary preferred over content) | | | |
| `plan` item | — | `plan_update{type:"markdown",planId,content}` if client advertised `plan`, else `agent_message_chunk` | | | |
| `turn/plan/updated` | — | `plan{entries}` with `inProgress` → `in_progress`, priority `medium` | | | |
| `thread/tokenUsage/updated` | — | `usage_update{used: totalTokens, size: modelContextWindow}` | | | |
| `thread/name/updated`, status changes | — | `session_info_update` | | | |

Status mapping (`toAcpStatus`): `inProgress` → `in_progress`, `completed` →
`completed`, `failed`/`declined` → `failed`.

## 7. What is extension, not spec

Spec rule (`docs/protocol/v1/extensibility.mdx`): custom data goes in `_meta`
(root keys `traceparent`, `tracestate`, `baggage` reserved for W3C trace
context); custom methods/notifications MUST start with `_`; custom
capabilities are advertised via `_meta` in capability objects; implementations
"MUST NOT add any custom fields at the root of a type that's part of the
specification". v2 extends this to enum values: `_`-prefixed values are
implementation-specific, unknown non-underscore values are reserved for
future ACP (`docs/protocol/v2/overview.mdx` "Conventions").

Observed extensions Pingex must treat as harness-specific:

| Extension | Where | Compliance |
| --- | --- | --- |
| `_session/steering` request `{sessionId, prompt: ContentBlock[]}` → `{outcome: "injected"|"startedNewTurn"|"failed"}`; advertised as `InitializeResponse._meta.steering.supported` | both adapters (`codex-acp/src/AcpExtensions.ts`, `claude-agent-acp` `STEER_METHOD`) | `_`-prefixed, compliant |
| `_session/goal` (legacy `_codex/session/goal_control`); `_meta.goal` on updates | both | compliant |
| `_session/async_task/stop` | claude-agent-acp | compliant |
| `_claude/sdkMessage`, `_claude/origin`, `_claude/rateLimit`, `_claude/askUserQuestionOption` `_meta` keys | claude-agent-acp | compliant |
| `session/set_model` + `models` field on session responses | codex-acp legacy | **non-compliant** (no underscore, root field); superseded by config options |
| `authentication/status`, `authentication/logout` | codex-acp | **non-compliant** naming (should be `_`-prefixed); `logout` is spec |
| `sessionUpdate: "subagent_spawned"` / `"subagent_state_update"`, `sessionCapabilities.subagents` | both (`claude-agent-acp/src/acp-subagents.ts` cites spec PR #1992 "draft … SDK does not contain it yet") | draft, not in any published schema |
| `sessionUpdate: "async_task_spawned"` / `"async_task_progress"` / `"async_task_state_update"` | claude-agent-acp | not in schema |
| `_meta.terminal_info` / `terminal_output` / `terminal_output_delta` / `terminal_exit` on tool calls; client opt-in `clientCapabilities._meta.terminal_output: true` | both (`claude-agent-acp/src/tools.ts:97-115`, `codex-acp/src/TerminalOutputMode.ts`) | `_meta` convention; effectively the de-facto "agent-owned terminal" that v2 standardises as `terminal_update` / `terminal_output_chunk` |
| `_meta.jetbrains.air.*` (session failure, agent file-change report, native subagent sessions, async tasks) | both | `_meta`, JetBrains-specific |
| `_meta.contextCompaction` on a synthetic `think` tool call | codex-acp | `_meta`; v1-unstable has a first-class `compaction_update` |
| `_meta.is_mcp_tool_call`, `_meta.codex.*`, `_meta.codex_approval_kind`, `_meta.persist` | codex-acp | `_meta` |
| `_meta.claudeCode.*` on `session/new` params (resume, options) and tool calls | claude-agent-acp | `_meta` |
| Config option category `collaboration_mode` | codex-acp | non-underscore custom category (spec says reserved) |
| `providers/*`, `session/fork`, `plan_update`, `notice`, `compaction_update`, `Usage`, `ToolCall.name` | spec unstable | in `schema.unstable.json` only |

## 8. Proposed neutral model for Pingex (derived)

Mirror these v1 types one-to-one, in this priority:

1. `SessionUpdate` union — all 11 stable variants; add `plan_update`,
   `notice`, `compaction_update` as optional (unstable) variants so
   codex-acp's plan markdown and compaction are representable.
2. `ToolCall` / `ToolCallUpdate` / `ToolCallStatus` / `ToolKind` /
   `ToolCallContent` (`content` | `diff` | `terminal`) / `ToolCallLocation`.
   Store tool calls keyed by `toolCallId` and apply updates as upserts.
   Include an optional `name` (unstable) for the raw harness tool name.
3. `ContentBlock` (MCP shape) for message chunks and tool content.
4. `RequestPermissionRequest` / `PermissionOption` / `RequestPermissionOutcome`.
5. `SessionConfigOption` (select + boolean, with `category`) as the single
   representation of mode, model, effort and fast-mode; keep `SessionModeState`
   only as a legacy input, never as internal state.
6. `StopReason` as a turn-ended event payload (v2-compatible).
7. `AgentCapabilities` / `SessionCapabilities` / `ClientCapabilities` for
   feature probing, mapping onto Pingex's existing `Feature` mechanism.
8. A single opaque `_meta: Json | null` on every mirrored type; harness
   adapters may read known keys (`terminal_output*`, `terminal_exit`,
   `contextCompaction`, `_claude/*`, `codex.*`) but the neutral model must not
   depend on them.

Explicitly out of the neutral model: `fs/*`, `terminal/*` client methods
(removed in v2), `session/set_mode` (removed in v2; fold into config
options), `session/load` (fold into a `resume{replay}` flag), and all
`subagent_*` / `async_task_*` draft updates until they land in a published
schema.

## Things not verified

- Live wire traffic: shapes were taken from `schema.json` and adapter source,
  not captured from running adapters.
- `codex-acp` (now TypeScript) vs the older Rust `codex-acp`: only the
  current `main` was examined.
- `docs/protocol/v1/*.mdx` pages were read from the repo, not the rendered
  site; URL anchors quoted in schema descriptions (e.g. `/protocol/tool-calls`)
  redirect to `/protocol/v1/...` on the site and were not clicked.
- Whether Zed or JetBrains actually consume `plan_update` / `subagents` today.
