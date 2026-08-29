# Claude Code stdio control protocol (serde-ready)

Ticket: PIN-7. Researched 2026-08-29 against Claude Code CLI **2.1.251** and
`@anthropic-ai/claude-agent-sdk` **0.3.251** (the SDK version tracks the CLI version).

The driving command this document describes:

```
claude -p \
  --input-format stream-json --output-format stream-json --verbose \
  --include-partial-messages --replay-user-messages \
  --permission-prompt-tool stdio
```

## Sources (in order of trust)

| Tag | Source |
|-----|--------|
| **[SDK-D]** | `sdk.d.ts` in `@anthropic-ai/claude-agent-sdk@0.3.251` (npm). All wire types below are transcribed from it; the doc comments are verbatim excerpts. |
| **[SDK-JS]** | `sdk.mjs` in the same package — how the official SDK actually spawns the CLI and serialises frames. Used to confirm envelope shapes the `.d.ts` leaves implicit. |
| **[DOC-H]** | https://code.claude.com/docs/en/headless.md |
| **[DOC-S]** | https://code.claude.com/docs/en/agent-sdk/streaming-output.md |
| **[DOC-C]** | https://code.claude.com/docs/en/cli-reference.md |
| **[CLI]** | `claude --help` from the installed 2.1.251 binary. |
| **[CHG]** | https://github.com/anthropics/claude-code/blob/main/CHANGELOG.md (raw fetch). |
| **[OLD]** | `@anthropic-ai/claude-code@1.0.58/1.0.59` `cli.js` from npm, grepped for the `stdio` permission-tool switch. |

No `claude -p` run with a real prompt was performed; nothing in this document is
derived from a live capture. See "Unverified" at the end.

## 1. Transport model

- One JSON object per line on stdin and stdout (NDJSON). The SDK writes
  `JSON.stringify(frame) + "\n"` and reads line by line. **[SDK-JS]**
- Every frame is discriminated by a top-level `type`; `system` and `result`
  frames add `subtype`; `control_request` frames put the subtype inside
  `request.subtype`. **[SDK-D]** `SDKMessage` doc: "discriminated by type (and
  subtype for system/result messages). Consumers should ignore types and
  subtypes they do not recognize: the set grows over time."
- Control requests flow **both ways** on the same stream: the client sends
  `initialize`/`interrupt`/… and the CLI sends `can_use_tool`/`hook_callback`/
  `request_user_dialog`/`mcp_message`/`elicitation`. Each is answered by exactly
  one `control_response` echoing `request_id`; either side may withdraw its own
  request with `control_cancel_request`. **[SDK-D]** `SDKControlRequest`,
  `SDKControlCancelRequest`.
- `{"type":"keep_alive"}` may appear in either direction at any time and must
  be ignored. **[SDK-D]** `SDKKeepAliveMessage`.
- Blank / whitespace-only / CRLF lines on stdin are tolerated since 2.1.208-ish
  (CHANGELOG line "Fixed stream-json input killing the session on blank CRLF or
  whitespace-only lines"). **[CHG]**
- Piped stdin is capped at 10 MB per message; multibyte UTF-8 split across
  chunks is handled since a 2.1.x fix. **[DOC-H]**, **[CHG]**

Ordering guarantees worth encoding in the Rust state machine **[SDK-D]**:

- `system/init` is emitted "at the start of each turn, normally ahead of every
  other message of that turn" (it can be preceded by `plugin_install` and
  `hook_started/progress/response` for SessionStart/Setup hooks). **[DOC-H]**
- With `--include-partial-messages` the CLI emits one `assistant` message per
  **completed content block** (several consecutive `assistant` frames sharing
  `message.id`, each with a single block, `stop_reason: null`); the turn's final
  stop reason and usage come on `result`.
- Exactly one `result` per turn, after that turn's `assistant`/`user`/
  `stream_event` frames. Informational `system` frames may still follow it.
  `system/session_state_changed {state:"idle"}` is described as the
  "authoritative turn-over signal".
- `user_message_uuid` is stamped on the turn's first reply frame (first
  non-ping `stream_event` when partial messages are on) and echoed on `result`,
  so a client can bind replies to the send that caused them.

## 2. stdin frames (client → CLI)

### 2.1 `user` — submit a prompt

`SDKUserMessage` **[SDK-D]**. The official SDK writes exactly this for a string
prompt **[SDK-JS]** (`function AD` in `sdk.mjs`):

```json
{"type":"user","session_id":"","message":{"role":"user","content":[{"type":"text","text":"hello"}]},"parent_tool_use_id":null}
```

Full field set (`SDKUserMessage`):

```ts
{
  type: 'user';
  message: MessageParam;              // Anthropic Messages API user message:
                                      // { role: "user", content: string | ContentBlockParam[] }
                                      // blocks: text, image, document, tool_result, ...
  parent_tool_use_id: string | null;  // null for main-thread sends
  isSynthetic?: boolean;
  tool_use_result?: unknown;          // CLI-emitted only (see stdout §3.4)
  priority?: 'now' | 'next' | 'later';
  origin?: SDKMessageOrigin;          // e.g. {kind:'human'}; "absent origin is treated as
                                      // unattributed and fails closed at strict isHuman() trust gates"
  shouldQuery?: boolean;              // false = append to transcript without starting a turn
  timestamp?: string;                 // ISO
  uuid?: UUID;                        // client-chosen; echoed back as user_message_uuid
  session_id?: string;                // SDK sends "" — the CLI fills it
  subagent_type?: string;
  task_description?: string;
}
```

Image content block (Messages API shape — `MessageParam` is re-exported from
`@anthropic-ai/sdk`) **[SDK-D]**:

```json
{"type":"image","source":{"type":"base64","media_type":"image/png","data":"<base64>"}}
```

`content` may also be a plain string. `parent_tool_use_id` is documented as a
routing key for subagent-addressed messages; for a desktop client sending main
thread input, always `null`.

Also accepted on stdin (not needed for Pingex but part of the union):
client-injected `assistant` frames (CHANGELOG 2.1.251: "client-injected
assistant tool calls sent without a message id …") **[CHG]**, and
`queued_notification` frames when the `queued_notifications` capability is
advertised **[SDK-D]** `SDKSystemMessage.capabilities`.

### 2.2 `control_request` envelope

```ts
{ type: 'control_request'; request_id: string; request: SDKControlRequestInner }
```

The SDK generates `request_id` as `Math.random().toString(36).substring(2,15)`;
any string unique among the sender's in-flight requests is fine. **[SDK-JS]**

Subtypes a client sends (all `SDKControl*Request` in **[SDK-D]**):

#### `initialize`

Sent once by the SDK before the first prompt. Everything optional.

```ts
{
  subtype: 'initialize';
  hooks?: Partial<Record<HookEvent, SDKHookCallbackMatcher[]>>;
      // SDKHookCallbackMatcher = { matcher?: string; hookCallbackIds: string[]; timeout?: number }
      // "Opaque ids chosen by the client, one per hook function ... When the hook fires the
      //  CLI sends a hook_callback control request carrying one of these ids as callback_id"
  sdkMcpServers?: string[];
  sdkMcpServerConfigs?: Record<string, { timeout?: number }>;
  jsonSchema?: Record<string, unknown>;      // structured output schema
  systemPrompt?: string[];                    // SDK wraps a string into [string]
  appendSystemPrompt?: string;
  planModeInstructions?: string;
  toolAliases?: Record<string, string>;
  excludeDynamicSections?: boolean;
  agents?: Record<string, AgentDefinition>;
  title?: string;
  skills?: string[];
  promptSuggestions?: boolean;
  agentProgressSummaries?: boolean;
  forwardSubagentText?: boolean;
  supportedDialogKinds?: string[];           // must list every request_user_dialog kind you render
  perTaskStopAffordance?: boolean;
}
```

`HookEvent` = `'PreToolUse' | 'PostToolUse' | 'PostToolUseFailure' | 'PostToolBatch' | 'Notification' | 'UserPromptSubmit' | 'UserPromptExpansion' | 'SessionStart' | 'SessionEnd' | 'Stop' | 'StopFailure' | 'SubagentStart' | 'SubagentStop' | 'PreCompact' | 'PostCompact' | 'PreModelSwitch' | 'PostModelSwitch' | 'PermissionRequest' | 'PermissionDenied' | 'Setup' | 'TeammateIdle' | 'TaskCreated' | 'TaskCompleted' | 'Elicitation' | 'ElicitationResult' | 'ConfigChange' | 'WorktreeCreate' | 'WorktreeRemove' | 'InstructionsLoaded' | 'CwdChanged' | 'FileChanged' | 'DirectoryAdded' | 'MessageDisplay'`.

`AgentDefinition` (abridged): `{ description: string; prompt: string; tools?: string[]; disallowedTools?: string[]; model?: 'sonnet'|'opus'|'haiku'|'inherit'|string; ... }`.

Note: MCP servers for the *subprocess* are passed on the command line
(`--mcp-config '{"mcpServers":{...}}'`), not in `initialize`; `sdkMcpServers`
is only for servers hosted in-process by the client and bridged via
`mcp_message`. **[SDK-JS]** A later `mcp_set_servers` control request can
replace the dynamically managed set.

Success response payload (`SDKControlInitializeResponse`):

```ts
{
  commands: SlashCommand[]; agents: AgentInfo[]; output_style: string;
  available_output_styles: string[]; models: ModelInfo[]; account: AccountInfo;
  hooks_applied?: boolean; fast_mode_state?: FastModeState; fast_mode_disabled_reason?: ...
}
```

plus, on the envelope (not inside `response`), `pending_permission_requests?:
SDKControlRequest[]` and `pending_user_dialog_requests?: SDKControlRequest[]`
for a client joining an already-running session.

#### `interrupt`

```ts
{ subtype: 'interrupt'; cancel_queued?: boolean }
```

Response: `{ still_queued: string[]; cancelled?: string[] }` when the
`interrupt_receipt_v1` / `interrupt_cancel_queued_v1` capabilities are present on
`system/init`; older CLIs return an empty success. The SDK also ends stdin before
signalling, and the CLI "cancels the prompt as soon as the input ends". **[DOC-H]**

#### `set_permission_mode`

```ts
{ subtype: 'set_permission_mode'; mode: PermissionMode }
// PermissionMode = 'default' | 'acceptEdits' | 'bypassPermissions' | 'plan' | 'dontAsk' | 'auto'
```

#### `set_model`

```ts
{ subtype: 'set_model'; model?: string | null }   // omitted/null/'default' resets
```

Applied mid-turn since 2.1.211 ("the next model round-trip uses the new model").
**[CHG]** A non-string payload used to hang the session; since 2.1.208 it is
answered with an error response. **[CHG]**

#### `set_max_thinking_tokens`

```ts
{ subtype: 'set_max_thinking_tokens'; max_thinking_tokens?: number | null;
  thinking_display?: ('summarized' | 'omitted') | null }
```

#### `get_context_usage`

```ts
{ subtype: 'get_context_usage' }
```

Response `SDKControlGetContextUsageResponse` (camelCase!):
`{ categories: {name,tokens,color,isDeferred?}[]; totalTokens; maxTokens; rawMaxTokens; percentage; gridRows; model; memoryFiles; mcpTools; deferredBuiltinTools?; systemTools?; systemPromptSections?; agents; slashCommands?; skills?; autoCompactThreshold?; isAutoCompactEnabled; messageBreakdown?; apiUsage: {input_tokens,output_tokens,cache_creation_input_tokens,cache_read_input_tokens} | null }`.

#### `rewind_files`

```ts
{ subtype: 'rewind_files'; user_message_id: string; dry_run?: boolean }
```

Response `RewindFilesResult` (defined at sdk.d.ts:2945; not transcribed here).

#### `rename_session`

```ts
{ subtype: 'rename_session'; title: string }
```

#### `mcp_message` (client → CLI direction)

```ts
{ subtype: 'mcp_message'; server_name: string; message: JSONRPCMessage }
```

"Flows in both directions … the CLI acknowledges a client-sent one with an
empty success." Only relevant if Pingex hosts an in-process MCP server.

#### Other client-originated subtypes present in 0.3.251 (for completeness)

`set_color`, `mcp_status`, `get_session_cost`, `list_models`, `get_usage`,
`get_binary_version`, `mcp_call`, `file_suggestions`, `cancel_async_message
{message_uuid}`, `read_file {path,max_bytes?,encoding?}`, `seed_read_state
{path,mtime}`, `mcp_set_servers {servers}`, `register_repo_root {directory,
reload_claude_md?, reload_plugins?, reload_skills?}`, `reload_plugins`,
`reload_skills`, `mcp_reconnect {serverName}`, `mcp_toggle {serverName,enabled}`,
`stop_task {task_id}`, `background_tasks {tool_use_id?}`, `apply_flag_settings
{settings}`, `get_settings`. Each is one `subtype` string plus the listed fields.

### 2.3 `control_response` (client → CLI, answering CLI-originated requests)

Envelope **[SDK-D]** `SDKControlResponse`, serialisation confirmed in **[SDK-JS]**:

```json
{"type":"control_response","response":{"subtype":"success","request_id":"<id>","response":{...}}}
{"type":"control_response","response":{"subtype":"error","request_id":"<id>","error":"<message>"}}
```

`response` is "the success payload, shaped as documented for the answered
request's subtype; absent or {} for requests that are merely acknowledged."

#### Answering `can_use_tool`

Payload is `PermissionResult` **[SDK-D]**; the SDK spreads the callback result
and adds `toolUseID` **[SDK-JS]** (`return {...n, toolUseID: e.request.tool_use_id}`):

```ts
// allow
{ behavior: 'allow'; updatedInput?: Record<string, unknown>; updatedPermissions?: PermissionUpdate[];
  toolUseID?: string; decisionClassification?: 'user_temporary' | 'user_permanent' | 'user_reject' }
// deny
{ behavior: 'deny'; message: string; interrupt?: boolean;
  toolUseID?: string; decisionClassification?: ... }
```

Concrete allow:

```json
{"type":"control_response","response":{"subtype":"success","request_id":"abc",
  "response":{"behavior":"allow","updatedInput":{"command":"ls"},"toolUseID":"toolu_01…",
              "updatedPermissions":[{"type":"addRules","rules":[{"toolName":"Bash","ruleContent":"ls *"}],"behavior":"allow","destination":"session"}]}}}
```

Concrete deny:

```json
{"type":"control_response","response":{"subtype":"success","request_id":"abc",
  "response":{"behavior":"deny","message":"User declined","interrupt":false,"toolUseID":"toolu_01…"}}}
```

`PermissionUpdate` union:

```ts
| { type: 'addRules' | 'replaceRules' | 'removeRules'; rules: {toolName: string; ruleContent?: string}[];
    behavior: 'allow' | 'deny' | 'ask'; destination: PermissionUpdateDestination }
| { type: 'setMode'; mode: PermissionMode; destination }
| { type: 'addDirectories' | 'removeDirectories'; directories: string[]; destination }
// PermissionUpdateDestination = 'userSettings' | 'projectSettings' | 'localSettings' | 'session' | 'cliArg'
```

"Typically if presenting the user an option 'always allow' … this full set of
suggestions [permission_suggestions] should be returned as the
`updatedPermissions`." **[SDK-D]** `CanUseTool`.

Fail-closed warning from the docs: never leave a `can_use_tool` unanswered —
"permission prompts have no park deadline" — the tool blocks forever. To drop a
prompt after an interrupt, send `control_cancel_request` or answer deny.

#### Answering `hook_callback`

Payload is `HookJSONOutput` = `AsyncHookJSONOutput | SyncHookJSONOutput` **[SDK-D]**:

```ts
{ async: true; asyncTimeout?: number }
// or
{ continue?: boolean; suppressOutput?: boolean; stopReason?: string;
  decision?: 'approve' | 'block'; systemMessage?: string; reason?: string; terminalSequence?: string;
  hookSpecificOutput?: { hookEventName: 'PreToolUse'; permissionDecision?: 'allow'|'deny'|'ask'|'defer';
                         permissionDecisionReason?: string; updatedInput?: Record<string,unknown>;
                         additionalContext?: string } | /* other *HookSpecificOutput */ }
```

An empty `{}` success is a valid "no opinion" answer.

#### Answering `request_user_dialog`

Payload `UserDialogResult` **[SDK-D]**:

```ts
{ behavior: 'completed'; result: unknown } | { behavior: 'cancelled' }
```

Rule from the request doc: only answer kinds you declared in
`initialize.supportedDialogKinds`; for an undeclared kind **do not answer at
all** ("never with {behavior: 'cancelled'}, which is a real settlement").

#### Answering `mcp_message` / `elicitation`

`mcp_message`: `{ mcp_response: JSONRPCMessage }`. `elicitation`: an
`ElicitationResult` (MCP `ElicitResult`). Both only matter for SDK-hosted MCP.

### 2.4 `control_cancel_request`

```json
{"type":"control_cancel_request","request_id":"<id>"}
```

Either side may send it for a request it originated; no reply. **[SDK-D]**

## 3. stdout frames (CLI → client)

All CLI-emitted frames carry `uuid: UUID` and `session_id: string` unless noted.

### 3.1 `system` subtypes

| `subtype` | Extra fields **[SDK-D]** |
|-----------|------------------------|
| `init` | `apiKeySource; claude_code_version; cwd; tools: string[]; mcp_servers: {name,status}[]; model; permissionMode; slash_commands: string[]; terminal_slash_commands?; output_style; skills: string[]; plugins: {name,path,version?}[]; agents?: string[]; betas?; fast_mode_state?; fast_mode_disabled_reason?; effort?: 'low'\|'medium'\|'high'\|'xhigh'\|'max'\|null; capabilities?: string[]`. Docs add `plugin_errors?` and `mcp_server_errors?: {name,type,message}[]` (2.1.219+). **[DOC-H]** |
| `compact_boundary` | `compact_metadata: { trigger: 'manual'\|'auto'; pre_tokens: number; post_tokens?; duration_ms?; preserved_segment?: {head_uuid,anchor_uuid,tail_uuid}; preserved_messages?: {anchor_uuid, uuids: UUID[]} }` |
| `status` | `status: 'compacting'\|'requesting'\|null; permissionMode?; compact_result?: 'success'\|'failed'; compact_error?` |
| `api_retry` | `attempt; max_retries; retry_delay_ms; error_status: number\|null; error: SDKAssistantMessageError` |
| `control_request_progress` | `request_id; status: 'started'\|'api_retry'; attempt?; max_retries?; retry_delay_ms?; error_status?` |
| `task_started` | `task_id; tool_use_id?; description; subagent_type?; is_backgrounded?; spawn_depth?; task_type?; workflow_name?; prompt?; skip_transcript?; ambient?` |
| `task_progress` | `task_id; tool_use_id?; description; subagent_type?; usage: {total_tokens,tool_uses,duration_ms}; last_tool_name?; summary?` |
| `task_updated` | `task_id; patch: { status?: 'pending'\|'running'\|'completed'\|'failed'\|'killed'\|'paused'; description?; end_time?; total_paused_ms?; error?; is_backgrounded? }` |
| `task_notification` | `task_id; tool_use_id?; status: 'completed'\|'failed'\|'stopped'; output_file; summary; usage?; skip_transcript?; ambient?` |
| `background_tasks_changed` | `tasks: {task_id, task_type, description, ambient?}[]` (REPLACE semantics) |
| `hook_started` | `hook_id; hook_name; hook_event` |
| `hook_progress` | `hook_id; hook_name; hook_event; stdout; stderr; output` |
| `hook_response` | `hook_id; hook_name; hook_event; output; stdout; stderr; exit_code?; outcome: 'success'\|'error'\|'cancelled'` |
| `plugin_install` | `status: 'started'\|'installed'\|'failed'\|'completed'; name?; error?` |
| `session_state_changed` | `state: 'idle'\|'running'\|'requires_action'` |
| `thinking_tokens` | `estimated_tokens; estimated_tokens_delta` |
| `permission_denied` | `tool_name; tool_use_id; agent_id?; decision_reason_type?; decision_reason?; message` |
| `informational` | `content; level: 'info'\|'notice'\|'suggestion'\|'warning'; tool_use_id?; prevent_continuation?` |
| `notification` | `key; text; priority: 'low'\|'medium'\|'high'\|'immediate'; color?; timeout_ms?` |
| `local_command_output` | `content` |
| `commands_changed` | `commands: SlashCommand[]` |
| `model_refusal_fallback` | `trigger:'refusal'; direction; scope?; original_model; fallback_model; request_id; api_refusal_category?; api_refusal_explanation?; retracted_message_uuids?; refused_user_message_uuid?; content` |
| `model_refusal_no_fallback` | `original_model; request_id; api_refusal_category?; api_refusal_explanation?; refused_user_message_uuid?; content` |
| `elicitation_complete` | `mcp_server_name; elicitation_id` |
| `files_persisted` | `files: {filename,file_id}[]; failed: {filename,error}[]; processed_at` |
| `memory_recall` | `mode: 'select'\|'synthesize'; memories: {path, scope, content?}[]` |
| `mirror_error` | `error; key: {projectKey, sessionId, subpath?}` |
| `worker_shutting_down` | `reason` |

`SDKAssistantMessageError` = `'authentication_failed' | 'oauth_org_not_allowed' | 'account_on_hold' | 'billing_error' | 'rate_limit' | 'overloaded' | 'invalid_request' | 'model_not_found' | 'server_error' | 'unknown' | 'max_output_tokens'`.

### 3.2 `assistant`

```ts
{
  type: 'assistant';
  message: BetaMessage;    // Messages API: { id, type:'message', role:'assistant', model,
                           //   content: (text|thinking|redacted_thinking|tool_use|...)[],
                           //   stop_reason: string|null, stop_sequence, usage }
  parent_tool_use_id: string | null;
  error?: SDKAssistantMessageError;
  uuid; session_id;
  request_id?: string;
  user_message_uuid?: string;
  resumed_from_incomplete_thinking?: true;
  supersedes?: UUID[];
  aborted?: true;
  subagent_type?: string; task_description?: string;
  timestamp?: string;
  context_usage?: SDKContextUsage;
}
```

Key behaviour: one frame per completed content block during streaming; frames
share `message.id`; `stop_reason` is `null` on those; final usage is on
`result`. **[SDK-D]**

### 3.3 `stream_event` (only with `--include-partial-messages`)

```ts
{ type: 'stream_event'; event: BetaRawMessageStreamEvent; parent_tool_use_id: string | null;
  uuid; session_id; ttft_ms?: number; user_message_uuid?: string }
```

`event` is a raw Messages API streaming event: `message_start`,
`content_block_start`, `content_block_delta` (`delta.type` ∈ `text_delta`,
`input_json_delta{partial_json}`, `thinking_delta`, `signature_delta`),
`content_block_stop`, `message_delta`, `message_stop`, plus `ping`. Stream events
are main-session only; `parent_tool_use_id` is always `null`. **[DOC-S]**

### 3.4 `user` (CLI-emitted: tool results and replays)

Same shape as §2.1. Two flavours:

- **Tool results**: `message.content` holds `tool_result` blocks; extra
  `tool_use_result?: unknown` carries the tool's structured Output object
  (per-tool shape; see `sdk-tools.d.ts`). `parent_tool_use_id` is set for
  subagent tool results.
- **Replays** (`--replay-user-messages`): `SDKUserMessageReplay` = the client's
  message echoed with `isReplay: true`, `uuid`, `session_id` filled in, and
  optional `file_attachments?: unknown[]`. Added in 1.0.86; `isReplay` in 2.0.5
  "to prevent duplicate messages". **[CHG]**

### 3.5 `result`

Union on `subtype` **[SDK-D]**:

```ts
// success
{ type: 'result'; subtype: 'success';
  duration_ms; duration_api_ms; ttft_ms?; ttft_stream_ms?; time_to_request_ms?;
  user_message_uuid?; request_sent_wall_ms?; time_to_request_from_spawn_ms?; warm_spare_claimed?; time_origin_ms?;
  is_error: boolean; api_error_status?: number | null;
  num_turns: number; result: string; stop_reason: string | null;
  total_cost_usd: number;                       // cumulative across turns in a streaming-input session
  usage: NonNullableUsage;                      // main loop only, per-turn
  modelUsage: Record<string, ModelUsage>;       // cumulative; "the correct field for token/cost accounting"
  permission_denials: { tool_name; tool_use_id; tool_input: Record<string,unknown> }[];
  queued_turn_count?: number;
  structured_output?: unknown;                  // when --json-schema / initialize.jsonSchema
  deferred_tool_use?: { id; name; input };
  terminal_reason?: TerminalReason; fast_mode_state?; fast_mode_disabled_reason?; origin?;
  uuid; session_id }
// error
{ type: 'result'; subtype: 'error_during_execution' | 'error_max_turns' | 'error_max_budget_usd' | 'error_max_structured_output_retries';
  duration_ms; duration_api_ms; is_error; num_turns; stop_reason; total_cost_usd; usage; modelUsage;
  permission_denials; queued_turn_count?; errors: string[]; user_message_uuid?; terminal_reason?; ...; uuid; session_id }
```

`ModelUsage` (sdk.d.ts:1306): `{ inputTokens; outputTokens; cacheReadInputTokens; cacheCreationInputTokens; webSearchRequests; costUSD; contextWindow; maxOutputTokens }` (camelCase).
`NonNullableUsage` is the Messages API `Usage` with nulls removed
(`input_tokens`, `output_tokens`, `cache_creation_input_tokens`,
`cache_read_input_tokens`, `server_tool_use?`, `service_tier?`, …).
`TerminalReason` = `'blocking_limit' | 'rapid_refill_breaker' | 'prompt_too_long' | 'image_error' | 'model_error' | 'api_error' | 'malformed_tool_use_exhausted' | 'aborted_streaming' | 'aborted_tools' | 'stop_hook_prevented' | 'hook_stopped' | 'tool_deferred' | 'max_turns' | 'background_requested' | 'completed' | 'budget_exhausted' | 'structured_output_retry_exhausted' | 'tool_deferred_unavailable' | 'turn_setup_failed'`.

### 3.6 `rate_limit_event`

```ts
{ type: 'rate_limit_event';
  rate_limit_info: { status: 'allowed'|'allowed_warning'|'rejected'; resetsAt?: number;
    rateLimitType?: 'five_hour'|'seven_day'|'seven_day_opus'|'seven_day_sonnet'|'seven_day_overage_included'|'overage';
    utilization?: number; overageStatus?; overageResetsAt?; overageDisabledReason?; isUsingOverage?;
    overageInUse?; surpassedThreshold?; errorCode?: 'credits_required'; canUserPurchaseCredits?; hasChargeableSavedPaymentMethod? };
  uuid; session_id }
```

### 3.7 Other top-level `type`s the CLI can emit

`tool_progress {tool_use_id; tool_name; parent_tool_use_id; elapsed_time_seconds; task_id?; heartbeat?; subagent_type?; subagent_retry?}`,
`tool_use_summary {summary; preceding_tool_use_ids}`, `auth_status {isAuthenticating; output: string[]; error?}`,
`prompt_suggestion {suggestion}` (with `--prompt-suggestions`), `conversation_reset {new_conversation_id}`,
`active_goal {value|null}`, `keep_alive`, `transcript_mirror` (SDK-internal, `--session-mirror` only).
Model these with a catch-all `Other(serde_json::Value)` variant.

### 3.8 `control_request` (CLI → client)

Same envelope as §2.2. Subtypes the client must handle:

#### `can_use_tool`

```ts
{ subtype: 'can_use_tool';
  tool_name: string; input: Record<string, unknown>;
  permission_suggestions?: PermissionUpdate[];
  blocked_path?: string;
  decision_reason?: string;              // "May carry ANSI escapes; sanitize before rendering."
  decision_reason_type?: 'rule'|'mode'|'subcommandResults'|'permissionPromptTool'|'hook'|'asyncAgent'|'sandboxOverride'|'workingDir'|'safetyCheck'|'classifier'|'other';
  classifier_approvable?: boolean;
  suppress_always_allow_rule?: boolean;  // don't offer "always allow"
  default_to_no?: boolean;               // don't pre-select approve
  matched_ask_rule?: { source: string; tool_name: string; rule_content?: string };
  title?: string; display_name?: string; description?: string;   // pre-rendered prompt text
  tool_use_id: string; agent_id?: string;
  requires_user_interaction?: boolean }
```

Only the `ask` path reaches the client; auto-denies are reported as
`system/permission_denied` and in `result.permission_denials`. **[SDK-D]**

#### `hook_callback`

```ts
{ subtype: 'hook_callback'; callback_id: string; input: HookInput; tool_use_id?: string }
// HookInput = BaseHookInput & { hook_event_name: HookEvent; ...per-event fields }
// BaseHookInput = { session_id; transcript_path; cwd; prompt_id?; permission_mode?; agent_id?; agent_type?; effort?: {level} }
// PreToolUse adds { tool_name; tool_input: unknown; tool_use_id }
```

Only emitted for hooks registered via `initialize.hooks`.

#### `request_user_dialog`

```ts
{ subtype: 'request_user_dialog'; dialog_kind: string; payload: Record<string, unknown>; tool_use_id?: string }
```

Only sent for kinds declared in `initialize.supportedDialogKinds`; known kind
mentioned in the docs: `refusal_fallback_prompt`.

#### `mcp_message` / `elicitation`

`mcp_message` as §2.2; `elicitation {mcp_server_name; message; mode?: 'form'|'url'; url?; elicitation_id?; requested_schema?; title?; display_name?; description?}`.

### 3.9 `control_cancel_request` (CLI → client)

`{type:'control_cancel_request', request_id}` — e.g. the CLI withdraws a pending
`can_use_tool` after an interrupt, or one another client already answered. On
receipt, dismiss the dialog and stop waiting.

## 4. CLI flags that matter

Source: **[CLI]** unless noted. Flags marked *hidden* are absent from
`claude --help` but accepted (the SDK passes them). **[SDK-JS]**

| Flag | Notes |
|------|-------|
| `-p, --print` | Required for everything below. |
| `--input-format stream-json` | "realtime streaming input (only works with --print)". |
| `--output-format stream-json` | NDJSON on stdout. |
| `--verbose` (*hidden*) | The SDK always passes it with stream-json; docs pair it with `--include-partial-messages`. **[DOC-H]** Likely needed for the full event stream (unverified whether omitting it drops frames). |
| `--include-partial-messages` | Emits `stream_event`. Added 1.0.109. **[CHG]** |
| `--replay-user-messages` | Echoes stdin `user` frames with `isReplay: true`. Added 1.0.86. **[CHG]** |
| `--permission-prompt-tool stdio` (*hidden*) | Routes `ask` decisions to `control_request/can_use_tool` on stdout. The literal `stdio` is special-cased in the CLI (`if(A==="stdio")return B.createCanUseTool()` in cli.js). Minimum version **1.0.59** — present in 1.0.59, absent in 1.0.58; CHANGELOG 1.0.59: "SDK: Added tool confirmation support with canUseTool callback". **[OLD]**, **[CHG]** Any other value names an MCP tool (`mcp__server__tool`) instead. |
| `--session-id <uuid>` | Pre-choose the session id (must be a UUID). |
| `--resume <id>` / `-c, --continue` | Resume; `--resume` finds ids across all projects since 2.1.223. **[DOC-C]** |
| `--fork-session` | With `--resume`/`--continue`: new session id. Combinable with `--session-id` since 2.0.73. **[CHG]** |
| `--no-session-persistence` | Print-mode only; nothing written to disk, cannot be resumed. |
| `--bare` | Skips hooks, LSP, plugin sync, auto-memory, keychain, CLAUDE.md discovery; **auth is strictly `ANTHROPIC_API_KEY`/apiKeyHelper — OAuth login is never read**, so unsuitable for subscription users. Added 2.1.81. **[CLI]**, **[CHG]** |
| `--setting-sources user,project,local` | SDK passes `--setting-sources=<csv>`. |
| `--mcp-config <json-or-file>` | SDK passes `'{"mcpServers":{...}}'` inline. `-p` waits for pending servers (≤ `MCP_TIMEOUT`, 30 s) since 2.1.221. **[DOC-C]** |
| `--strict-mcp-config` | Only `--mcp-config` servers. |
| `--effort low\|medium\|high\|xhigh\|max` | Docs also list `ultracode` (2.1.203+). **[DOC-C]** |
| `--model <alias-or-id>` | e.g. `fable`, `opus`, `sonnet`, `haiku`. |
| `--permission-mode` | `acceptEdits, auto, bypassPermissions, manual, dontAsk, plan` (`manual` = `default`, 2.1.200+; `default` still accepted). **[DOC-C]** |
| `--forward-subagent-text` | Subagent text/thinking as `assistant`/`user` frames with `parent_tool_use_id`. 2.1.211+; nested depth 2.1.219+. |
| `--include-hook-events` | `hook_started/progress/response` for all hooks (SessionStart/Setup always included). |
| `--max-turns <n>` (*hidden*), `--max-budget-usd`, `--max-thinking-tokens` (*hidden*), `--thinking adaptive\|disabled` (*hidden*), `--thinking-display` (*hidden*), `--json-schema`, `--fallback-model`, `--agents <json>`, `--system-prompt`, `--append-system-prompt`, `--add-dir`, `--tools`, `--allowedTools`, `--disallowedTools`, `--allow-dangerously-skip-permissions`, `--plugin-dir`, `--betas`, `--session-mirror` (*hidden*), `--managed-settings` (*hidden*) | Also constructed by the SDK. |

Exact argv the SDK builds (order preserved) **[SDK-JS]**:
`--output-format stream-json --verbose --input-format stream-json [--thinking …] [--effort …] [--max-turns …] [--max-budget-usd …] [--model …] [--agent …] [--betas …] [--json-schema …] [--debug-file …] --permission-prompt-tool stdio [--continue] [--resume=ID] [--allowedTools …] [--disallowedTools …] [--tools …] [--mcp-config JSON] [--setting-sources=csv] [--strict-mcp-config] [--permission-mode …] [--allow-dangerously-skip-permissions] [--fallback-model …] [--include-hook-events] [--include-partial-messages] [--session-mirror] [--add-dir …]* [--plugin-dir …]* [--fork-session] [--session-id=UUID] [--no-session-persistence] …`.
Note the SDK does **not** pass `--replay-user-messages`; it is optional and only
affects whether stdin `user` frames are echoed.

Exit codes: 0 success, non-zero on failure, 143 on SIGTERM. In streaming-input
mode the process stays alive until stdin closes; then it drains queued output
(≤ 30 s) and exits. **[DOC-H]**

## 5. Proposed Rust serde sketch

Design notes:

- Externally tagged by `type`, then inner enums tagged by `subtype`. serde does
  not support two-level tags natively; use `#[serde(tag = "type")]` on the
  outer enum and put the `subtype`-tagged enum in a flattened field.
- Every struct gets `#[serde(flatten)] extra: serde_json::Map` or
  `#[serde(other)]` variants so new fields/subtypes never break parsing (the
  spec explicitly says the set grows).
- Messages-API objects (`BetaMessage`, `MessageParam`, stream events) should
  reuse the crate's existing `Json` newtype (`crate::util::json::Json`) rather
  than be fully typed on day one; only `content[].type` and `tool_use` fields
  need typed access for the UI.
- Field names are mixed snake_case (wire-level) and camelCase (nested payloads
  such as `PermissionResult`, `ModelUsage`, `get_context_usage` response). Do
  **not** apply a blanket `rename_all`; annotate per struct.

```rust
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

// ---------- stdout (CLI -> client) ----------

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Outbound {
    System(SystemMsg),
    Assistant(AssistantMsg),
    User(UserMsg),
    StreamEvent(StreamEventMsg),
    Result(ResultMsg),
    RateLimitEvent(RateLimitEventMsg),
    ToolProgress(Json),
    ToolUseSummary(Json),
    AuthStatus(Json),
    PromptSuggestion(Json),
    ConversationReset(Json),
    ActiveGoal(Json),
    KeepAlive,
    ControlRequest(ControlRequest<CliRequest>),
    ControlResponse(ControlResponse),
    ControlCancelRequest { request_id: String },
    #[serde(other)]
    Unknown,
}

#[derive(Deserialize)]
pub struct Envelope { pub uuid: String, pub session_id: String }

#[derive(Deserialize)]
#[serde(tag = "subtype", rename_all = "snake_case")]
pub enum SystemMsg {
    Init(SystemInit),
    CompactBoundary { compact_metadata: CompactMetadata, #[serde(flatten)] env: Envelope },
    Status { status: Option<StatusKind>, #[serde(rename = "permissionMode")] permission_mode: Option<PermissionMode>,
             compact_result: Option<String>, compact_error: Option<String>, #[serde(flatten)] env: Envelope },
    ApiRetry { attempt: u32, max_retries: u32, retry_delay_ms: u64, error_status: Option<u16>,
               error: AssistantError, #[serde(flatten)] env: Envelope },
    ControlRequestProgress(Json),
    TaskStarted(TaskStarted), TaskProgress(TaskProgress), TaskUpdated(TaskUpdated), TaskNotification(TaskNotification),
    BackgroundTasksChanged(Json),
    HookStarted(HookLifecycle), HookProgress(HookLifecycle), HookResponse(HookLifecycle),
    PluginInstall(Json),
    SessionStateChanged { state: SessionState, #[serde(flatten)] env: Envelope },
    ThinkingTokens { estimated_tokens: u64, estimated_tokens_delta: u64, #[serde(flatten)] env: Envelope },
    PermissionDenied(PermissionDenied),
    Informational { content: String, level: String, tool_use_id: Option<String>,
                    prevent_continuation: Option<bool>, #[serde(flatten)] env: Envelope },
    Notification(Json), LocalCommandOutput { content: String, #[serde(flatten)] env: Envelope },
    CommandsChanged(Json), ModelRefusalFallback(Json), ModelRefusalNoFallback(Json),
    ElicitationComplete(Json), FilesPersisted(Json), MemoryRecall(Json), MirrorError(Json),
    WorkerShuttingDown { reason: String, #[serde(flatten)] env: Envelope },
    #[serde(other)]
    Unknown,
}

#[derive(Deserialize)]
pub struct SystemInit {
    #[serde(rename = "apiKeySource")] pub api_key_source: String,
    pub claude_code_version: String,
    pub cwd: String,
    pub tools: Vec<String>,
    pub mcp_servers: Vec<McpServerStatus>,          // { name, status }
    #[serde(default)] pub mcp_server_errors: Vec<Json>,
    pub model: String,
    #[serde(rename = "permissionMode")] pub permission_mode: PermissionMode,
    pub slash_commands: Vec<String>,
    pub output_style: String,
    #[serde(default)] pub skills: Vec<String>,
    #[serde(default)] pub plugins: Vec<Json>,
    #[serde(default)] pub plugin_errors: Vec<Json>,
    pub agents: Option<Vec<String>>,
    pub effort: Option<Effort>,
    #[serde(default)] pub capabilities: Vec<String>,   // "interrupt_receipt_v1", "interrupt_cancel_queued_v1", "queued_notifications"
    #[serde(flatten)] pub env: Envelope,
}

#[derive(Deserialize)]
pub struct AssistantMsg {
    pub message: Json,                       // BetaMessage; content[].type in {text, thinking, redacted_thinking, tool_use, ...}
    pub parent_tool_use_id: Option<String>,
    pub error: Option<AssistantError>,
    pub user_message_uuid: Option<String>,
    pub aborted: Option<bool>,
    pub supersedes: Option<Vec<String>>,
    pub subagent_type: Option<String>,
    pub timestamp: Option<String>,
    #[serde(flatten)] pub env: Envelope,
}

#[derive(Deserialize)]
pub struct StreamEventMsg {
    pub event: Json,                         // raw Messages API stream event, tagged by event.type
    pub parent_tool_use_id: Option<String>,
    pub ttft_ms: Option<u64>,
    pub user_message_uuid: Option<String>,
    #[serde(flatten)] pub env: Envelope,
}

#[derive(Serialize, Deserialize)]
pub struct UserMsg {
    pub message: Json,                       // { role: "user", content: string | ContentBlockParam[] }
    pub parent_tool_use_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] pub tool_use_result: Option<Json>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "isReplay")] pub is_replay: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "isSynthetic")] pub is_synthetic: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")] pub uuid: Option<String>,
    #[serde(default)] pub session_id: String,              // send "" ; CLI fills it
    #[serde(skip_serializing_if = "Option::is_none")] pub origin: Option<Json>,   // {"kind":"human"} for keyboard input
    #[serde(skip_serializing_if = "Option::is_none", rename = "shouldQuery")] pub should_query: Option<bool>,
}

#[derive(Deserialize)]
#[serde(tag = "subtype")]
pub enum ResultMsg {
    #[serde(rename = "success")] Success(ResultSuccess),
    #[serde(rename = "error_during_execution")] ErrorDuringExecution(ResultError),
    #[serde(rename = "error_max_turns")] ErrorMaxTurns(ResultError),
    #[serde(rename = "error_max_budget_usd")] ErrorMaxBudgetUsd(ResultError),
    #[serde(rename = "error_max_structured_output_retries")] ErrorMaxStructuredOutputRetries(ResultError),
}

#[derive(Deserialize)]
pub struct ResultCommon {
    pub duration_ms: u64, pub duration_api_ms: u64,
    pub is_error: bool, pub num_turns: u32, pub stop_reason: Option<String>,
    pub total_cost_usd: f64,
    pub usage: Json,                                      // NonNullableUsage (snake_case)
    #[serde(rename = "modelUsage")] pub model_usage: std::collections::BTreeMap<String, ModelUsage>, // camelCase inner
    #[serde(default)] pub permission_denials: Vec<PermissionDenial>,
    pub queued_turn_count: Option<u32>,
    pub user_message_uuid: Option<String>,
    pub terminal_reason: Option<String>,
    #[serde(flatten)] pub env: Envelope,
}
#[derive(Deserialize)] pub struct ResultSuccess {
    pub result: String, pub structured_output: Option<Json>, pub ttft_ms: Option<u64>,
    pub api_error_status: Option<u16>, pub deferred_tool_use: Option<Json>,
    #[serde(flatten)] pub common: ResultCommon }
#[derive(Deserialize)] pub struct ResultError { #[serde(default)] pub errors: Vec<String>, #[serde(flatten)] pub common: ResultCommon }

#[derive(Deserialize)] pub struct PermissionDenial { pub tool_name: String, pub tool_use_id: String, pub tool_input: Json }
#[derive(Deserialize)] #[serde(rename_all = "camelCase")]
pub struct ModelUsage { pub input_tokens: u64, pub output_tokens: u64, pub cache_read_input_tokens: u64,
    pub cache_creation_input_tokens: u64, pub web_search_requests: u64, #[serde(rename = "costUSD")] pub cost_usd: f64,
    pub context_window: u64, pub max_output_tokens: u64 }

// ---------- control envelopes (both directions) ----------

#[derive(Serialize, Deserialize)]
pub struct ControlRequest<R> { pub request_id: String, pub request: R }

#[derive(Serialize, Deserialize)]
pub struct ControlResponse { pub response: ControlResponseBody }

#[derive(Serialize, Deserialize)]
#[serde(tag = "subtype", rename_all = "snake_case")]
pub enum ControlResponseBody {
    Success { request_id: String,
              #[serde(skip_serializing_if = "Option::is_none")] response: Option<Json>,
              #[serde(default, skip_serializing_if = "Vec::is_empty")] pending_permission_requests: Vec<ControlRequest<CliRequest>>,
              #[serde(default, skip_serializing_if = "Vec::is_empty")] pending_user_dialog_requests: Vec<ControlRequest<CliRequest>> },
    Error   { request_id: String, error: String },
}

// Requests the CLI sends us.
#[derive(Deserialize)]
#[serde(tag = "subtype", rename_all = "snake_case")]
pub enum CliRequest {
    CanUseTool(CanUseTool),
    HookCallback { callback_id: String, input: Json, tool_use_id: Option<String> },
    RequestUserDialog { dialog_kind: String, payload: Json, tool_use_id: Option<String> },
    McpMessage { server_name: String, message: Json },
    Elicitation(Json),
    #[serde(other)] Unknown,
}

#[derive(Deserialize)]
pub struct CanUseTool {
    pub tool_name: String,
    pub input: Json,
    pub tool_use_id: String,
    #[serde(default)] pub permission_suggestions: Vec<PermissionUpdate>,
    pub blocked_path: Option<String>,
    pub decision_reason: Option<String>,          // may contain ANSI; sanitize
    pub decision_reason_type: Option<String>,
    pub classifier_approvable: Option<bool>,
    pub suppress_always_allow_rule: Option<bool>,
    pub default_to_no: Option<bool>,
    pub matched_ask_rule: Option<Json>,
    pub title: Option<String>, pub display_name: Option<String>, pub description: Option<String>,
    pub agent_id: Option<String>,
    pub requires_user_interaction: Option<bool>,
}

// Requests we send the CLI.
#[derive(Serialize)]
#[serde(tag = "subtype", rename_all = "snake_case")]
pub enum ClientRequest {
    Initialize(InitializeRequest),                     // fields are camelCase on the wire
    Interrupt { #[serde(skip_serializing_if = "Option::is_none")] cancel_queued: Option<bool> },
    SetPermissionMode { mode: PermissionMode },
    SetModel { model: Option<String> },
    SetMaxThinkingTokens { max_thinking_tokens: Option<u32>,
                           #[serde(skip_serializing_if = "Option::is_none")] thinking_display: Option<String> },
    GetContextUsage,
    RewindFiles { user_message_id: String, #[serde(skip_serializing_if = "Option::is_none")] dry_run: Option<bool> },
    RenameSession { title: String },
    McpMessage { server_name: String, message: Json },
    McpStatus, GetUsage, ListModels,
    StopTask { task_id: String },
    CancelAsyncMessage { message_uuid: String },
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct InitializeRequest {
    #[serde(skip_serializing_if = "Option::is_none")] pub hooks: Option<std::collections::BTreeMap<String, Vec<HookMatcher>>>,
    #[serde(skip_serializing_if = "Option::is_none")] pub system_prompt: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")] pub append_system_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] pub agents: Option<Json>,
    #[serde(skip_serializing_if = "Option::is_none")] pub json_schema: Option<Json>,
    #[serde(skip_serializing_if = "Option::is_none")] pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] pub supported_dialog_kinds: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")] pub forward_subagent_text: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")] pub per_task_stop_affordance: Option<bool>,
}
#[derive(Serialize)] #[serde(rename_all = "camelCase")]
pub struct HookMatcher { #[serde(skip_serializing_if = "Option::is_none")] pub matcher: Option<String>,
    pub hook_callback_ids: Vec<String>, #[serde(skip_serializing_if = "Option::is_none")] pub timeout: Option<u64> }

// Payload for answering can_use_tool (goes in ControlResponseBody::Success.response).
#[derive(Serialize)]
#[serde(tag = "behavior", rename_all = "lowercase")]
pub enum PermissionResult {
    Allow { #[serde(rename = "toolUseID")] tool_use_id: String,
            #[serde(rename = "updatedInput", skip_serializing_if = "Option::is_none")] updated_input: Option<Json>,
            #[serde(rename = "updatedPermissions", skip_serializing_if = "Option::is_none")] updated_permissions: Option<Vec<PermissionUpdate>>,
            #[serde(rename = "decisionClassification", skip_serializing_if = "Option::is_none")] decision_classification: Option<DecisionClassification> },
    Deny  { #[serde(rename = "toolUseID")] tool_use_id: String, message: String,
            #[serde(skip_serializing_if = "Option::is_none")] interrupt: Option<bool>,
            #[serde(rename = "decisionClassification", skip_serializing_if = "Option::is_none")] decision_classification: Option<DecisionClassification> },
}
#[derive(Serialize, Deserialize, Clone, Copy)] #[serde(rename_all = "snake_case")]
pub enum DecisionClassification { UserTemporary, UserPermanent, UserReject }

#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PermissionUpdate {
    AddRules     { rules: Vec<PermissionRule>, behavior: PermissionBehavior, destination: PermissionDestination },
    ReplaceRules { rules: Vec<PermissionRule>, behavior: PermissionBehavior, destination: PermissionDestination },
    RemoveRules  { rules: Vec<PermissionRule>, behavior: PermissionBehavior, destination: PermissionDestination },
    SetMode      { mode: PermissionMode, destination: PermissionDestination },
    AddDirectories    { directories: Vec<String>, destination: PermissionDestination },
    RemoveDirectories { directories: Vec<String>, destination: PermissionDestination },
}
#[derive(Serialize, Deserialize, Clone)] #[serde(rename_all = "camelCase")]
pub struct PermissionRule { pub tool_name: String, #[serde(skip_serializing_if = "Option::is_none")] pub rule_content: Option<String> }
#[derive(Serialize, Deserialize, Clone, Copy)] #[serde(rename_all = "lowercase")]
pub enum PermissionBehavior { Allow, Deny, Ask }
#[derive(Serialize, Deserialize, Clone, Copy)] #[serde(rename_all = "camelCase")]
pub enum PermissionDestination { UserSettings, ProjectSettings, LocalSettings, Session, CliArg }
#[derive(Serialize, Deserialize, Clone, Copy)] #[serde(rename_all = "camelCase")]
pub enum PermissionMode { Default, AcceptEdits, BypassPermissions, Plan, DontAsk, Auto }

// Payload for answering request_user_dialog.
#[derive(Serialize)] #[serde(tag = "behavior", rename_all = "lowercase")]
pub enum UserDialogResult { Completed { result: Json }, Cancelled }

// ---------- stdin (client -> CLI) ----------
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Inbound {
    User(UserMsg),
    ControlRequest(ControlRequest<ClientRequest>),
    ControlResponse(ControlResponse),
    ControlCancelRequest { request_id: String },
    KeepAlive,
}
```

Mapping to the Pingex `Feature`/compat layer: `capabilities` on `system/init`
is the intended feature-detection surface ("check each capability for exactly
the behavior you use"); prefer it over version sniffing for interrupt receipts.
`--permission-prompt-tool stdio`, `--replay-user-messages` and
`--include-partial-messages` all predate 2.0 and need no probe.

## 6. Unverified / open points

1. **No live capture.** Every shape is from the type definitions and the SDK's
   own serialiser. A single confirmation run (`claude -p --model haiku
   --max-turns 1 --no-session-persistence --input-format stream-json
   --output-format stream-json --verbose --include-partial-messages
   --replay-user-messages --permission-prompt-tool stdio`) was not executed.
   Highest-value things to confirm live: (a) whether omitting `--verbose` drops
   any frames in stream-json mode; (b) the exact `usage` object on `result`;
   (c) that `session_id: ""` on a stdin `user` frame is accepted when no
   `initialize` was sent first.
2. **Is `initialize` mandatory?** The SDK always sends it before the first
   prompt; the 1.0.59 CLI answered `can_use_tool` without one. Not verified for
   2.1.251 whether a bare `user` frame works without `initialize`, though
   `hooks_applied?` docs imply `initialize` is optional and repeatable.
3. **`--permission-prompt-tool stdio` minimum version = 1.0.59** is established
   by grepping the published CLI bundles (present in 1.0.59, absent in 1.0.58)
   and the 1.0.59 CHANGELOG entry; the string `stdio` is never mentioned in the
   CHANGELOG or public docs, so treat it as a semi-private SDK contract.
4. `RewindFilesResult`, `SlashCommand`, `ModelInfo`, `AccountInfo`, `AgentInfo`
   and the per-tool `tool_use_result` shapes (`sdk-tools.d.ts`, 4133 lines)
   were not transcribed; they are reachable from the scratchpad install.
5. `--bare` disables OAuth entirely (help text: "OAuth and keychain are never
   read"), so it cannot be the default for subscription-authenticated users of
   Pingex.
