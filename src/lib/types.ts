export interface ThreadSummary {
  id: string;
  cwd: string;
  title: string;
  updatedAt: number;
  status: string;
  pinned: boolean;
  parentThreadId?: string | null;
  agentNickname?: string | null;
  agentRole?: string | null;
  subagentCount?: number;
}

/** An attached folder or file that contributes searchable content to a project. */
export interface ProjectSource {
  id: string;
  projectPath: string;
  sourcePath: string;
  kind: "folder" | "file";
  addedAt: number;
  status: "pending" | "indexed" | "error";
  indexedAt?: number | null;
  docCount: number;
  error?: string | null;
}

export interface Project {
  path: string;
  name: string;
  kind: "folder" | "worktree" | "multiProject";
  /** Present only for a virtual multi-project workspace. */
  workspaceId?: string;
  pinned: boolean;
  archived?: boolean;
  threads: ThreadSummary[];
  /** Free-form project instructions; empty string when none are stored. */
  instructions?: string;
  sources?: ProjectSource[];
  /** Ordered members exposed below a virtual workspace hub. */
  members?: WorkspaceMember[];
}

export interface WorkspaceMember {
  sourcePath: string;
  effectivePath: string;
  alias: string;
  isolated: boolean;
  branch?: string | null;
  available: boolean;
}

export interface WorkspaceMemberInput {
  sourcePath: string;
  alias: string;
  isolated: boolean;
}

export interface CreateWorkspaceInput {
  name: string;
  members: WorkspaceMemberInput[];
}

/** One project-file match: a file-name hit (no line) or a content-line hit. */
export interface WorkspaceFileMatch {
  path: string;
  fileName: string;
  lineNumber?: number | null;
  preview?: string | null;
  nameMatch: boolean;
}

/** One local-chat / message match from cached thread data. */
export interface WorkspaceThreadMatch {
  threadId: string;
  title: string;
  cwd: string;
}

export interface WorkspaceSearchGroup<T> {
  items: T[];
  nextCursor?: string | null;
  hasMore: boolean;
}

/** Grouped workspace search results with per-group cursor pagination. */
export interface WorkspaceSearchResults {
  projectFiles: WorkspaceSearchGroup<WorkspaceFileMatch>;
  threads: WorkspaceSearchGroup<WorkspaceThreadMatch>;
  messages: WorkspaceSearchGroup<WorkspaceThreadMatch>;
  /** Echo of the client generation token so stale responses can be dropped. */
  generation: number;
}

export type MenuTarget =
  | { kind: "project"; project: Project }
  | { kind: "thread"; project: Project; thread: ThreadSummary };

export type MenuAction =
  | "reveal"
  | "rename"
  | "togglePin"
  | "toggleArchive"
  | "archive"
  | "delete"
  | "remove"
  | "moveUp"
  | "moveDown"
  | "moveToWorkspace"
  | "fork"
  | "openDetails";

/**
 * One item of a `turn/start` input as the app-server defines it
 * (`app-server-protocol` v2 `UserInput`, `text_elements` defaulted server-side).
 * Deliberately strict: a `skill` without a `path` once shipped and was only
 * rejected at runtime, so outbound builders type against this union rather
 * than the loose `UserInputPart` used for reading messages back.
 */
export type TurnInputItem =
  | { type: "text"; text: string }
  | { type: "image"; url: string; detail?: "low" | "high" | "original" | "auto" }
  | { type: "localImage"; path: string; detail?: "low" | "high" | "original" | "auto" }
  | { type: "audio"; url: string }
  | { type: "localAudio"; path: string }
  | { type: "skill"; name: string; path: string }
  | { type: "mention"; name: string; path: string };

/** A user message part as read back from Codex; loose because it may carry
 * types and fields the app does not model. Send with `TurnInputItem`. */
export interface UserInputPart {
  type: string;
  text?: string;
  url?: string;
  path?: string;
  name?: string;
}

/** The goal a long-running thread is working towards (`/goal`). */
export interface ThreadGoal {
  threadId: string;
  objective: string;
  status: "active" | "paused" | "blocked" | "usageLimited" | "budgetLimited" | "complete" | (string & {});
  tokenBudget: number | null;
  tokensUsed: number;
  timeUsedSeconds: number;
}

export interface FileUpdateChange {
  path: string;
  kind: { type: string; movePath?: string | null };
  diff: string;
}

/** One step of the todo list Codex maintains for the turn it is working on. */
export interface TurnPlanStep {
  step: string;
  status: "pending" | "inProgress" | "completed";
}

export interface TurnPlan {
  turnId: string;
  explanation: string | null;
  steps: TurnPlanStep[];
}

/**
 * The extra access Codex is asking for on an `item/permissions/requestApproval`.
 * Granting means echoing the same profile back; declining means sending an
 * empty one.
 */
export interface RequestPermissionProfile {
  network?: { enabled?: boolean } | null;
  fileSystem?: {
    read?: string[] | null;
    write?: string[] | null;
    entries?: { path: FileSystemPath; access: string }[] | null;
  } | null;
}

export type FileSystemPath = { path?: string; pattern?: string; value?: string };

/** The `form`-mode schema an MCP elicitation asks the app to draw fields for. */
export interface McpElicitationSchema {
  type: "object";
  properties: Record<string, McpElicitationField>;
  required?: string[];
}

/**
 * One field of an elicitation form. Upstream models these as untagged unions,
 * so a field is identified by `type` plus which of the choice shapes it
 * carries: `enum` (+ optional `enumNames`) or `oneOf` for a single select,
 * `type: "array"` with `items` for a multi select.
 */
export interface McpElicitationField {
  type: "string" | "number" | "integer" | "boolean" | "array";
  title?: string;
  description?: string;
  enum?: string[];
  enumNames?: string[];
  oneOf?: McpElicitationOption[];
  items?: { type?: string; enum?: string[]; anyOf?: McpElicitationOption[]; oneOf?: McpElicitationOption[] };
  default?: string | number | boolean | string[];
  minimum?: number;
  maximum?: number;
  minLength?: number;
  maxLength?: number;
  minItems?: number;
  maxItems?: number;
  format?: string;
}

export interface McpElicitationOption {
  const: string;
  title: string;
}

/**
 * Every item type this app knows about: the `ThreadItem` variants the
 * app-server v2 protocol defines, plus `userInputAnswered`, which the app
 * builds itself because Codex's thread projection has no item for an answered
 * question.
 *
 * Codex adds item types faster than this app adopts them, so an item's `type`
 * is still any string — see `ThreadItem.type`. What this list buys is the
 * exhaustiveness check in `turnSegments.ts`: adding a member here without
 * saying how it should be drawn fails the build.
 */
export const THREAD_ITEM_TYPES = [
  "userMessage",
  "hookPrompt",
  "agentMessage",
  "plan",
  "reasoning",
  "commandExecution",
  "fileChange",
  "mcpToolCall",
  "dynamicToolCall",
  "collabAgentToolCall",
  "subAgentActivity",
  "webSearch",
  "imageView",
  "imageGeneration",
  "sleep",
  "enteredReviewMode",
  "exitedReviewMode",
  "contextCompaction",
  "userInputAnswered",
] as const;

export type KnownThreadItemType = (typeof THREAD_ITEM_TYPES)[number];

export interface ThreadItem {
  /**
   * Deliberately open: a type Codex added after this app last caught up still
   * has to round-trip through the store and the journal. `KnownThreadItemType`
   * narrows the ones the app actually handles.
   */
  type: KnownThreadItemType | (string & {});
  id: string;
  /**
   * `userMessage` carries its parts here; `reasoning` carries the model's
   * unabridged reasoning here, as plain strings. Codex reuses the field name
   * across both item types, so the type has to cover both — read the reasoning
   * side through `reasoningContent`.
   */
  content?: UserInputPart[] | string[];
  // agentMessage / plan
  text?: string;
  // set while agentMessage deltas are still arriving; cleared when the item
  // completes, so the transcript knows when Codex went quiet to do tool work
  streaming?: boolean;
  // hookPrompt
  fragments?: { text: string; hookRunId?: string }[];
  // reasoning: the version shown by default; the unabridged one lives in
  // `content` above, behind an expander
  summary?: string[];
  /**
   * Codex's own risk assessment of this action, from
   * `item/autoApprovalReview/completed`. Explains a command that was blocked
   * without the user ever being asked.
   */
  guardianReview?: {
    status: string;
    riskLevel?: string | null;
    userAuthorization?: string | null;
    rationale?: string | null;
  };
  // commandExecution
  command?: string;
  cwd?: string;
  status?: string;
  aggregatedOutput?: string | null;
  exitCode?: number | null;
  durationMs?: number | null;
  // fileChange
  changes?: FileUpdateChange[];
  // mcpToolCall / dynamicToolCall
  server?: string;
  tool?: string;
  /** Latest progress line from a long-running MCP tool; gone once it finishes. */
  progress?: string;
  /** What the model passed the tool. Present on `dynamicToolCall`. */
  arguments?: Record<string, unknown>;
  // webSearch
  query?: string;
  // userInputAnswered
  questions?: {
    id: string;
    header?: string;
    question: string;
    isSecret?: boolean;
    options?: { label: string; description?: string }[] | null;
  }[];
  answers?: Record<string, { answers: string[] }>;
  // set when the user skipped the questions and steered Codex in their own words
  steer?: string;
  // set when the session ended before the question was answered; the original
  // request is gone, so it can only be answered as a new turn
  unanswered?: boolean;
  // set when the user gave up on a question stranded that way
  dismissed?: boolean;
  // subAgentActivity: "started" | "interacted" | "interrupted"
  kind?: string;
  agentThreadId?: string;
  agentPath?: string;
  // imageView (sleep reuses durationMs above)
  path?: string;
  // imageGeneration
  revisedPrompt?: string | null;
  savedPath?: string;
  // enteredReviewMode / exitedReviewMode
  review?: string;
  // collabAgentToolCall
  senderThreadId?: string;
  receiverThreadIds?: string[];
  prompt?: string | null;
  model?: string | null;
  reasoningEffort?: string | null;
  agentsStates?: Record<string, { status: string; message?: string | null }>;
}

export interface Turn {
  id: string;
  items: ThreadItem[];
  status: string;
  error?: { message: string } | null;
  startedAt?: number | null;
  completedAt?: number | null;
  durationMs?: number | null;
  /** What the turn ran on, as the composer resolved it; absent on older turns. */
  model?: string | null;
  reasoningEffort?: string | null;
}

/** One side of `thread/tokenUsage/updated` — either the session total or the last request. */
export interface TokenUsageBreakdown {
  totalTokens: number;
  inputTokens: number;
  cachedInputTokens: number;
  cacheWriteInputTokens?: number;
  outputTokens: number;
  reasoningOutputTokens: number;
}

export interface ThreadTokenUsage {
  total: TokenUsageBreakdown;
  last: TokenUsageBreakdown;
  modelContextWindow?: number | null;
}

/** One row of the per-model usage breakdown in `ThreadUsage`. */
export interface ThreadUsageBreakdownGroup {
  model?: string | null;
  reasoningEffort?: string | null;
  speed?: string | null;
  estimatedUsageCreditsMicros: number;
  netNewInputTokens?: number | null;
  cachedInputTokens?: number | null;
  inputTokens?: number | null;
  outputTokens?: number | null;
  totalTokens?: number | null;
}

/** `threadUsage` from `account/usage/read` when called with a `threadId`. */
export interface ThreadUsage {
  threadId: string;
  estimatedUsageCreditsMicros: number;
  estimatedUsageUsdMicros?: number | null;
  groups: ThreadUsageBreakdownGroup[];
}

/** A message waiting in the server-side thread queue (`thread/queue/*`). */
export interface QueuedSubmission {
  id: string;
  input: UserInputPart[];
  clientUserMessageId: string;
}

/** One rolling rate-limit window from `account/rateLimits/read`. */
export interface RateLimitWindow {
  usedPercent: number;
  /** Window length in minutes — 300 for the 5h limit, 10080 for the weekly one. */
  windowDurationMins?: number | null;
  /** Unix seconds at which the window resets. */
  resetsAt?: number | null;
}

export interface RateLimitSnapshot {
  limitId?: string | null;
  limitName?: string | null;
  primary?: RateLimitWindow | null;
  secondary?: RateLimitWindow | null;
  planType?: string | null;
}

/** Response of `account/rateLimits/read`. */
export interface AccountRateLimits {
  rateLimits: RateLimitSnapshot;
  rateLimitsByLimitId?: Record<string, RateLimitSnapshot> | null;
}

export interface ThreadDetail {
  id: string;
  preview: string;
  name?: string | null;
  cwd: string;
  turns: Turn[];
  subagentModelPolicy?: SubagentPolicy | null;
  subagentReasoningEffortPolicy?: SubagentPolicy | null;
}

export interface Account {
  label: string;
  plan: string | null;
  kind: string;
}

export interface SideQuestion {
  sideThreadId: string;
  parentThreadId: string;
  title: string;
  createdAt: number;
}

export interface BootstrapData {
  codexHome: string;
  codexBinary: string;
  projects: Project[];
  account: Account | null;
  sideQuestions: SideQuestion[];
  subagents: ThreadSummary[];
}

export interface ReasoningEffortOption {
  reasoningEffort: string;
  description: string;
}

export interface Model {
  id: string;
  model: string;
  displayName: string;
  description: string;
  hidden: boolean;
  supportedReasoningEfforts: ReasoningEffortOption[];
  defaultReasoningEffort: string;
  isDefault: boolean;
  /** Id of the suggested replacement model, when this one is being upgraded away. */
  upgrade?: string | null;
  upgradeInfo?: ModelUpgradeInfo | null;
}

export interface ModelUpgradeInfo {
  model: string;
  upgradeCopy?: string | null;
  modelLink?: string | null;
  migrationMarkdown?: string | null;
  /** Unix seconds at which this model is scheduled to retire. */
  retirementAt?: number | null;
}

/** Per-turn overrides picked in the composer popovers. */
export interface TurnOptions {
  model?: string;
  effort?: string;
  approvalPolicy?: string;
  sandboxMode?: string;
  collaborationMode?: { mode: string; settings?: unknown };
  subagentModelPolicy?: SubagentPolicy | null;
  subagentReasoningEffortPolicy?: SubagentPolicy | null;
  /** What the turn will actually run on, including defaults the composer did
   *  not have to override. Recorded locally so the transcript can label each
   *  reply; not forwarded to Codex. */
  resolvedModel?: string | null;
  resolvedEffort?: string | null;
}

export type SubagentPolicy = { allowed: string[] } | { excluded: string[] };

export interface SubagentDetail {
  id: string;
  parentThreadId: string;
  title: string;
  cwd: string;
  status: string;
  agentNickname: string | null;
  agentRole: string | null;
  model: string | null;
  reasoningEffort: string | null;
  /**
   * Who owns this agent: `codex` for one Codex spawned itself, `app` for one
   * the app spawned as its own process. Only `app` agents can be killed from
   * here, since only those have a process we control.
   */
  source?: "codex" | "app";
  /** Set for `app` agents: the `agent_runs` row behind this entry. */
  runId?: string;
}

/** One agent the app spawned as its own Codex process. */
export interface AgentRun {
  runId: string;
  parentThreadId: string;
  parentTurnId: string;
  callId: string | null;
  /** The agent's own Codex thread, once it has started one. */
  childThreadId: string | null;
  name: string;
  prompt: string;
  cwd: string;
  model: string | null;
  reasoningEffort: string | null;
  /** `running` | `done` | `failed` | `killed` | `orphaned`. */
  status: string;
  result: string | null;
  error: string | null;
  createdAt: number;
  finishedAt: number | null;
}

/** How app-owned subagents are configured globally. */
export interface AgentSettings {
  enabled: boolean;
  /** The widest sandbox a spawned agent may run under. */
  sandbox: string;
  maxConcurrent: number;
  timeoutSeconds: number;
  sandboxOptions: string[];
}

export interface ArchivedThread {
  id: string;
  title: string;
  cwd: string;
  updatedAt: number;
}

export interface FileHit {
  path: string;
  fileName: string;
  score: number;
  isDir: boolean;
}

export interface Mention {
  name: string;
  path: string;
}

/** A validated, staged file/image the composer can attach to a turn. */
export interface Attachment {
  id: string;
  filename: string;
  mime: string;
  size: number;
  /** Absolute path of the staged copy (passed to the turn for images). */
  stagedPath: string;
  /** "image" | "file". */
  kind: "image" | "file";
}

export interface RuntimeSettings {
  codexHome: string;
  codexBinary: string;
  overrideCodexHome: string | null;
  overrideCodexBinary: string | null;
  settingsPath: string;
  restartRequired: boolean;
}

export interface RecentHome {
  path: string;
  lastUsed: number;
  /** Whether the folder still exists on disk. */
  exists: boolean;
}

/** Whether the configured Codex CLI can actually be spawned. */
export interface BinaryStatus {
  /** The configured value (an override, an env var, or bare `codex`). */
  binary: string;
  /** Absolute path it resolved to, when found. */
  resolved: string | null;
  found: boolean;
  /** Guidance to show when `found` is false. */
  message: string | null;
}

/** Read once at startup to decide whether to show the home picker or boot. */
export interface LaunchState {
  codexHome: string;
  codexBinary: string;
  defaultHome: string;
  /** Home came from `--codex-home`/`CODEX_HOME`; boot without a picker. */
  explicit: boolean;
  /** Show the picker before booting (inverse of `explicit`). */
  needsPicker: boolean;
  recentHomes: RecentHome[];
  /** A home cannot be opened until the Codex CLI resolves. */
  codexBinaryStatus: BinaryStatus;
}

export interface McpServerInfo {
  name: string;
  command: string | null;
}

export interface SkillInfo {
  name: string;
}

/** Read-only snapshot of the active Codex home's default configuration. */
export interface HomeOverview {
  codexHome: string;
  codexBinary: string;
  configExists: boolean;
  model: string | null;
  reasoningEffort: string | null;
  approvalPolicy: string | null;
  sandboxMode: string | null;
  mcpServers: McpServerInfo[];
  skills: SkillInfo[];
}

/** UI control kind for a `config.toml` setting. */
export type SettingKind = "enum" | "string" | "bool";

/** Where a setting's current value comes from. */
export type SettingSource = "default" | "config";

/** One whitelisted `config.toml` setting with its source and restart semantics. */
export interface ConfigSetting {
  key: string;
  section: string;
  label: string;
  kind: SettingKind;
  /** Effective value: the config value if set, else the default. */
  value: string | null;
  /** Codex's built-in default, shown when the value is inherited. */
  default: string | null;
  source: SettingSource;
  options: string[];
  /** A change needs a restart; otherwise it applies to the next thread. */
  restartRequired: boolean;
}

/** Working-tree status counts shared by worktree cards and the branch chip. */
export interface StatusCounts {
  staged: number;
  unstaged: number;
  untracked: number;
  conflicted: number;
}

export interface StatusFile {
  path: string;
  /** "staged" | "unstaged" | "untracked" | "conflicted" | "ignored". */
  state: string;
  /** Two-letter porcelain-v2 XY code; empty for untracked/ignored. */
  code: string;
}

/** Full working-tree status for one directory (from `git status`). */
export interface GitStatus {
  branch: string | null;
  detached: boolean;
  upstream: string | null;
  ahead: number;
  behind: number;
  counts: StatusCounts;
  files: StatusFile[];
  truncated: boolean;
  /** Milliseconds since the Unix epoch when this snapshot was taken. */
  refreshedAt: number;
}

/** High-level repository facts for a directory. */
export interface GitRepoInfo {
  dir: string;
  isGitRepo: boolean;
  root: string | null;
  commonDir: string | null;
  branch: string | null;
  detached: boolean;
  upstream: string | null;
  ahead: number;
  behind: number;
  /** "merge" | "rebase" | "cherry-pick" | "revert" | "bisect" when mid-op. */
  inProgress: string | null;
  error: string | null;
}

/** One worktree from `git worktree list --porcelain`, enriched by the backend. */
export interface WorktreeEntry {
  path: string;
  head: string | null;
  branch: string | null;
  detached: boolean;
  bare: boolean;
  locked: boolean;
  lockReason: string | null;
  prunable: boolean;
  prunableReason: string | null;
  isMain: boolean;
  /** True only when the canonical path is under `<codex_home>/worktrees/`. */
  isCodexManaged: boolean;
  missingDir: boolean;
  branchCheckedOutElsewhere: boolean;
  upstream: string | null;
  ahead: number;
  behind: number;
  status: StatusCounts | null;
  /** A distinct per-worktree problem to surface instead of dropping the row. */
  state: string | null;
}

export interface GitCommit {
  hash: string;
  shortHash: string;
  subject: string;
  author: string;
  timestamp: number;
}

/** One branch offered by the review target picker. */
export interface GitBranch {
  /** Remote-tracking branches keep their remote prefix, e.g. `origin/main`. */
  name: string;
  isRemote: boolean;
  isCurrent: boolean;
}

/**
 * What a `/review` turn should look at. Mirrors the app-server's `ReviewTarget`
 * and is forwarded to `review/start` untouched, so the member names have to
 * stay as they are.
 */
export type ReviewTarget =
  | { type: "uncommittedChanges" }
  | { type: "baseBranch"; branch: string }
  | { type: "commit"; sha: string; title?: string | null }
  | { type: "custom"; instructions: string };

/** New-worktree branch selection sent to the backend. */
export type WorktreeBranchRequest =
  | { kind: "existing"; name: string }
  | { kind: "new"; name: string; base?: string | null };

/** Payload of the `handoff://open` event when a `codex://` link is received. */
export interface HandoffOpen {
  /** "thread" | "new". */
  kind: string;
  threadId: string | null;
  path: string | null;
  /** Resolved (tilde-expanded) requested home, if the link carried one. */
  requestedHome: string | null;
  label: string | null;
  /** The requested home equals the running home (or none was supplied). */
  homeMatches: boolean;
  /** The requested home exists on disk (true when none was supplied). */
  homeExists: boolean;
}

// --- Pull-request review (provider-neutral; GitHub adapter first) ---

/** Availability and auth state of the active review provider. */
export interface ProviderStatus {
  installed: boolean;
  authenticated: boolean;
  message: string | null;
}

/** One open pull request in the picker. */
export interface PrSummary {
  number: number;
  title: string;
  author: string;
  /** "OPEN" | "CLOSED" | "MERGED". */
  state: string;
  isDraft: boolean;
  baseRef: string;
  headRef: string;
  updatedAt: string;
  url: string;
}

export interface PrCommit {
  oid: string;
  shortOid: string;
  headline: string;
  author: string;
}

/** One line inside a parsed diff hunk, with stable old/new line anchors. */
export interface DiffLine {
  /** "context" | "add" | "del". */
  kind: string;
  content: string;
  oldLine: number | null;
  newLine: number | null;
}

export interface DiffHunk {
  header: string;
  oldStart: number;
  oldLines: number;
  newStart: number;
  newLines: number;
  lines: DiffLine[];
}

/** One changed file in a PR (or local branch diff). */
export interface PrFile {
  path: string;
  oldPath: string | null;
  /** "added" | "modified" | "removed" | "renamed". */
  status: string;
  additions: number;
  deletions: number;
  /** Raw unified diff, fed to DiffBlock for rendering. */
  patch: string;
  hunks: DiffHunk[];
  /** The patch was omitted (binary or too large). */
  patchTruncated: boolean;
}

/** One review or conversation comment (flattened; grouped into threads in UI). */
export interface PrComment {
  id: number;
  author: string;
  body: string;
  createdAt: string;
  path: string | null;
  line: number | null;
  /** "RIGHT" (head) | "LEFT" (base) for an inline comment. */
  side: string | null;
  /** The review-thread node id shared by every comment in a thread. */
  threadId: string | null;
  isResolved: boolean;
}

export interface ChecksSummary {
  total: number;
  passing: number;
  failing: number;
  pending: number;
}

/** The full review payload for one PR. */
export interface PrDetail {
  summary: PrSummary;
  body: string;
  headSha: string;
  commits: PrCommit[];
  files: PrFile[];
  comments: PrComment[];
  checks: ChecksSummary | null;
  filesTruncated: boolean;
}

/** Result of comparing a locally-open PR against the current remote. */
export interface PrFreshness {
  stale: boolean;
  remoteHead: string;
  remoteUpdatedAt: string;
}

/** A saved local review draft (pending comments + chosen review event). */
export interface ReviewDraft {
  headSha: string;
  /** Opaque JSON owned by the frontend. */
  payload: string;
  updatedAt: number;
}

/** One inline comment the user has staged but not yet submitted. */
export interface PendingComment {
  path: string;
  line: number;
  side: string;
  body: string;
}

export interface PairingInfo {
  qrSvg: string;
  pairingCode: string;
  manualPairingCode: string | null;
  expiresAt: number | null;
}

// --- Remote connections management ---

/**
 * A paired remote client, merging the live relay's `remoteControl/client/list`
 * with locally-stored metadata (custom name, cached last-seen). `source` is
 * `"protocol"` when the relay still reports the device and `"local"` when it is
 * only known from a recorded pairing claim.
 */
export interface RemoteConnection {
  clientId: string;
  name: string;
  platform: string | null;
  deviceModel: string | null;
  appVersion: string | null;
  pairedAt: number | null;
  lastSeen: number | null;
  scope: string | null;
  source: "protocol" | "local";
}

// --- Integrations (MCP servers, skills, plugins) ---

/** Redacted view of one MCP server. Never carries secret env values. */
export interface McpServerSummary {
  name: string;
  transport: "stdio" | "http" | "unknown";
  command: string | null;
  argCount: number;
  url: string | null;
  /** Names of `env` keys (values are kept native-side and never sent here). */
  envKeys: string[];
  bearerTokenEnvVar: string | null;
  enabled: boolean;
  scope: string;
}

/** One skill as Codex resolves it, from `skills/list`. */
export interface SkillSummary {
  /** Possibly namespaced, e.g. `browser-use:browser`. */
  name: string;
  path: string;
  /** `"user"` or `"system"`. */
  scope: string;
  /** The `SKILL.md` description — what the model matches against. */
  description: string | null;
  enabled: boolean;
  /** Presentation overrides from a plugin-provided skill's `interface` block. */
  displayName: string | null;
  shortDescription: string | null;
}

export interface PluginSummary {
  name: string;
  scope: string;
}

export interface IntegrationsList {
  mcpServers: McpServerSummary[];
  skills: SkillSummary[];
  plugins: PluginSummary[];
  pluginsSupported: boolean;
}

/**
 * Whether a server needs — and has — credentials.
 *
 * - `unsupported` — the server does not do auth (every stdio server).
 * - `notLoggedIn` — an OAuth server we have no token for. This is the only
 *   state where "Sign in" applies.
 * - `oAuth` — signed in via OAuth.
 * - `bearerToken` — authenticated from a `bearer_token_env_var`, so there is
 *   nothing to sign into; the credential comes from the environment.
 */
export type McpAuthStatus = "unsupported" | "notLoggedIn" | "bearerToken" | "oAuth";

/** One tool a server exposes, as reported by `mcpServerStatus/list`. */
export interface McpTool {
  name: string;
  title?: string | null;
  description?: string | null;
  /** JSON Schema for the tool's arguments. */
  inputSchema?: McpJsonSchema | null;
  outputSchema?: McpJsonSchema | null;
}

/** The slice of JSON Schema the tool detail view reads. */
export interface McpJsonSchema {
  type?: string;
  title?: string;
  description?: string;
  properties?: Record<string, McpJsonSchema>;
  required?: string[];
  items?: McpJsonSchema;
  enum?: unknown[];
}

/**
 * Live state for one MCP server, straight from Codex. Complements
 * `McpServerSummary`, which reports what `config.toml` *declares*; this reports
 * what actually started. Joined on `name`.
 */
export interface McpServerStatus {
  name: string;
  serverInfo: {
    name: string;
    title?: string | null;
    version?: string | null;
    description?: string | null;
    websiteUrl?: string | null;
  } | null;
  /** Keyed by tool name. Empty when the server failed to start. */
  tools: Record<string, McpTool>;
  resources?: unknown[];
  resourceTemplates?: unknown[];
  authStatus: McpAuthStatus;
  /** Present when the server failed to start. */
  error?: string | null;
}

// --- History search and pagination (feature 11) ---

/** One hit from the local thread search index. */
export interface ThreadSearchItem {
  id: string;
  title: string;
  preview: string;
  cwd: string;
  updatedAt: number;
  archived: boolean;
}

/** A page of search results plus the total match count and echoed generation. */
export interface ThreadSearchPage {
  items: ThreadSearchItem[];
  nextCursor: string | null;
  total: number;
  generation: number;
}

/** Filter narrowing a thread search to a status/project scope. */
export interface ThreadSearchFilter {
  archived?: boolean;
  projectPath?: string | null;
}

/** A page of thread summaries with an opaque cursor for the next page. */
export interface ThreadsPage {
  items: ThreadSummary[];
  nextCursor: string | null;
}

/** One JSON-RPC message exchanged with the Codex app-server, as captured by
 * the (opt-in) message log in Advanced settings. */
export interface WireMessage {
  seq: number;
  /** Unix milliseconds. */
  at: number;
  /** "out" — this app to Codex; "in" — Codex to this app. */
  direction: "out" | "in";
  kind: "request" | "response" | "notification" | "serverRequest" | "error";
  method: string | null;
  id: number | null;
  threadId: string | null;
  payload: unknown;
  /** True when `payload` is a preview of an oversized body. */
  truncated: boolean;
}
