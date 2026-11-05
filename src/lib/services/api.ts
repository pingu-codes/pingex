// Typed wrappers over the generated commands. The `commands` proxy in
// `homeRouting` routes each call to the right Home, or, in the browser
// preview, to the fakes in `preview/commands.ts`; the wrappers here only add
// defaults, unwrap wire shapes and name results. A command that needs none of
// that is called as `commands.x` directly.

import { convertFileSrc } from "@tauri-apps/api/core";
import type { HarnessKind } from "$lib/bindings";
import { commands } from "$lib/services/homeRouting";
import { isTauri } from "$lib/services/tauri";
import type {
  AccountRateLimits,
  Attachment,
  BootstrapData,
  CodexServerInfo,
  CreateWorkspaceInput,
  IntegrationsList,
  McpServerStatus,
  Model,
  PairingInfo,
  QueuedSubmission,
  ReviewTarget,
  SkillSummary,
  SubagentDetail,
  ThreadDetail,
  ThreadGoal,
  ThreadItem,
  ThreadUsage,
  Turn,
  TurnOptions,
  UserInputPart,
  WorktreeBranchRequest,
} from "$lib/types";
import { errorMessage } from "$lib/utils/errors";

export async function updateWorkspace(workspaceId: string, input: CreateWorkspaceInput): Promise<BootstrapData> {
  return commands.updateWorkspace({ ...input, workspaceId });
}

const threadImageSrcs = new Map<string, Promise<string | null>>();

/**
 * A URL the webview can load for an image the agent viewed or generated, or
 * `null` when there is none (browser preview, or the backend refused it). The
 * path may sit outside the static asset scope (`/tmp`, a WSL distro), so the
 * backend opens it up first; that happens once per path.
 */
export function threadImageSrc(path: string): Promise<string | null> {
  if (!isTauri()) return Promise.resolve(null);
  let src = threadImageSrcs.get(path);
  if (!src) {
    src = commands.allowThreadImage(path).then(
      (local) => convertFileSrc(local),
      () => null,
    );
    threadImageSrcs.set(path, src);
  }
  return src;
}

export async function readThread(threadId: string): Promise<ThreadDetail> {
  return commands.readThread(threadId) as Promise<ThreadDetail>;
}

export async function listSubagents(threadId: string): Promise<SubagentDetail[]> {
  return commands.listSubagents(threadId) as Promise<SubagentDetail[]>;
}

export interface StartedThread {
  speedTier?: string | null;
  id: string;
  cwd?: string;
  /** Which harness the thread runs on; absent means Codex. */
  harness?: string | null;
}

export async function startThread(
  cwd: string,
  workspaceId?: string | null,
  harness?: HarnessKind | null,
): Promise<StartedThread> {
  return commands.startThread(cwd, workspaceId ?? null, harness ?? null) as Promise<StartedThread>;
}

/** The models a harness offers. Codex answers `model/list`; Claude has a fixed alias list. */
export async function listHarnessModels(harness: HarnessKind): Promise<Model[]> {
  return normalizeModels((await (commands.listHarnessModels(harness) as Promise<{ data?: ModelWire[] }>)).data);
}

export async function startTurn(threadId: string, input: UserInputPart[], options?: TurnOptions): Promise<Turn> {
  return commands.startTurn(threadId, input, options ?? null) as Promise<Turn>;
}

/**
 * Start a thread and send its first turn: the two calls every new thread
 * takes. `beforeTurn` runs in between with the new thread's id, so what it
 * records (a PR link, say) exists before the turn's first event arrives.
 */
export async function startThreadWithTurn(
  cwd: string,
  input: UserInputPart[],
  {
    workspaceId,
    harness,
    options,
    beforeTurn,
  }: {
    workspaceId?: string | null;
    harness?: HarnessKind | null;
    options?: TurnOptions;
    beforeTurn?: (thread: StartedThread) => Promise<void>;
  } = {},
): Promise<{ thread: StartedThread; turn: Turn }> {
  const thread = await startThread(cwd, workspaceId, harness);
  await beforeTurn?.(thread);
  const turn = await startTurn(thread.id, input, options);
  return { thread, turn };
}

/**
 * `requestId` is null when answering a question left over from a dead session:
 * the request is gone, so the answer is only persisted and the caller sends it
 * on as a fresh turn.
 */
export async function respondUserInput(
  requestId: number | null,
  answers: Record<string, { answers: string[] }>,
  context?: { threadId: string; turnId: string; itemId: string; item: ThreadItem },
): Promise<void> {
  await commands.respondUserInput(
    requestId,
    answers,
    context?.threadId ?? null,
    context?.turnId ?? null,
    context?.itemId ?? null,
    context?.item ?? null,
  );
}

/**
 * Persist a question as soon as Codex asks it. The request itself lives only in
 * the app-server process, so without this the question is unrecoverable if the
 * app exits before it is answered.
 */
export async function recordUserInputRequest(context: {
  threadId: string;
  turnId: string;
  itemId: string;
  afterItemId?: string;
  item: ThreadItem;
}): Promise<void> {
  await commands.recordUserInputRequest(
    context.threadId,
    context.turnId,
    context.itemId,
    context.item,
    context.afterItemId ?? null,
  );
}

/**
 * Start a review turn (`/review`) against the target the picker chose. With no
 * `target` Codex reviews the uncommitted changes.
 *
 * Returns the turn because a review, unlike a send, announces itself with no
 * `turn/started` — this response is the only place its id appears, and Stop
 * needs it.
 */
export async function startReview(threadId: string, target: ReviewTarget | null = null): Promise<Turn> {
  return commands.startReview(threadId, target) as Promise<Turn>;
}

/** Change the fields of the thread's goal that `patch` names; a `null`
 *  `tokenBudget` lifts the cap. */
async function updateThreadGoal(
  threadId: string,
  patch: { objective?: string; status?: "active" | "paused"; tokenBudget?: number | null },
): Promise<ThreadGoal> {
  return commands.threadGoalSet(
    threadId,
    patch.objective ?? null,
    patch.status ?? null,
    patch.tokenBudget ?? null,
    patch.tokenBudget === null ? true : null,
  ) as Promise<ThreadGoal>;
}

/** Set or update the goal for a long-running task (`/goal <objective>`). */
export async function setThreadGoal(threadId: string, objective: string): Promise<ThreadGoal> {
  return updateThreadGoal(threadId, { objective });
}

/** Pause or resume the thread's goal without changing its objective. */
export async function setThreadGoalStatus(threadId: string, status: "active" | "paused"): Promise<ThreadGoal> {
  return updateThreadGoal(threadId, { status });
}

/**
 * Cap the goal's spend at `tokens`, or lift the cap with `null`. Codex stops
 * driving the goal (`budgetLimited`) once `tokensUsed` reaches it; raising the
 * cap lets it carry on. Codex may refuse a cap above its configured maximum.
 */
export async function setThreadGoalBudget(threadId: string, tokens: number | null): Promise<ThreadGoal> {
  return updateThreadGoal(threadId, { tokenBudget: tokens });
}

/** The thread's goal, or null when none is set. */
export async function getThreadGoal(threadId: string): Promise<ThreadGoal | null> {
  return commands.threadGoalGet(threadId) as Promise<ThreadGoal | null>;
}

/** A `model/list` entry before the speed fields are given neutral names. */
type ModelWire = Model & {
  serviceTiers?: Model["speedTiers"];
  additionalSpeedTiers?: string[];
  defaultServiceTier?: string | null;
};

function normalizeModels(data: ModelWire[] | undefined): Model[] {
  return (data ?? []).map(({ serviceTiers, additionalSpeedTiers, defaultServiceTier, ...model }) => ({
    ...model,
    speedTiers:
      model.speedTiers ??
      serviceTiers ??
      additionalSpeedTiers
        ?.filter((tier) => tier === "fast")
        .map(() => ({ id: "priority", name: "Fast", description: "Faster responses" })),
    defaultSpeedTier: model.defaultSpeedTier ?? defaultServiceTier,
  }));
}

export async function listModels(): Promise<Model[]> {
  return normalizeModels((await (commands.listModels() as Promise<{ data?: ModelWire[] }>)).data);
}

export async function readAccountRateLimits(): Promise<AccountRateLimits> {
  return commands.readAccountRateLimits() as Promise<AccountRateLimits>;
}

export async function forkThread(
  threadId: string,
  beforeTurnId?: string,
  lastTurnId?: string,
  cwd?: string,
): Promise<StartedThread> {
  return commands.forkThread(threadId, beforeTurnId ?? null, lastTurnId ?? null, cwd ?? null) as Promise<StartedThread>;
}

interface RevertedThread {
  thread: ThreadDetail;
  turnsBackwardsCursor?: string | null;
  itemsBackwardsCursor?: string | null;
}

/** Replace `threadId`'s durable history with the prefix before `beforeTurnId`.
 *  `keptTurnIds` are the surviving turns, used to prune the local journal
 *  (revert's response carries no turns). */
export async function revertThread(
  threadId: string,
  beforeTurnId: string,
  keptTurnIds: string[],
): Promise<RevertedThread> {
  return commands.revertThread(threadId, beforeTurnId, keptTurnIds) as Promise<RevertedThread>;
}

export async function readThreadUsage(threadId: string): Promise<ThreadUsage | null> {
  const response = await (commands.readThreadUsage(threadId) as Promise<{ threadUsage?: ThreadUsage | null }>);
  return response.threadUsage ?? null;
}

/** Whether a rejection carries one of these `*_UNSUPPORTED` prefixes, i.e.
 *  the harness or this Codex version lacks the API, as opposed to an ordinary
 *  failure of one that has it. */
export function isUnsupported(cause: unknown, ...prefixes: string[]): boolean {
  const message = errorMessage(cause);
  return prefixes.some((prefix) => message.startsWith(prefix));
}

/** Prefix the Rust side puts on `read_context_breakdown` when the thread's
 *  harness cannot report its live context (Codex has no such API; a Claude
 *  thread between processes has nothing to ask). Kept in step with
 *  `CONTEXT_BREAKDOWN_UNSUPPORTED` in `src-tauri/src/usage/commands.rs`. */
export const CONTEXT_BREAKDOWN_UNSUPPORTED = "harness-unsupported:context_breakdown";

export const isContextBreakdownUnsupported = (cause: unknown) => isUnsupported(cause, CONTEXT_BREAKDOWN_UNSUPPORTED);

/** Prefix the Rust side puts on `read_prompt_parts_text` when the thread has
 *  no Codex rollout to read text from — a Claude thread, or a Codex thread
 *  with nothing on disk yet. Kept in step with `PROMPT_TEXT_UNSUPPORTED` in
 *  `src-tauri/src/usage/commands.rs`. */
export const PROMPT_TEXT_UNSUPPORTED = "harness-unsupported:prompt_parts_text";

export const isPromptPartsTextUnsupported = (cause: unknown) => isUnsupported(cause, PROMPT_TEXT_UNSUPPORTED);

/** Prefix the Rust side puts on a queue error when this Codex has no usable
 *  server queue — too old for `thread/queue/*`, missing the experimental
 *  capability, or running without a queue database. Kept in step with
 *  `Feature::QUEUE` in `src-tauri/src/codex/compat.rs`. */
export const QUEUE_UNSUPPORTED = "codex-queue-unsupported";

/** Whether a rejected queue call means "this Codex cannot queue at all", as
 *  opposed to an ordinary failure of a queue that does work. */
export const isQueueUnsupported = (cause: unknown) => isUnsupported(cause, QUEUE_UNSUPPORTED);

/** Prefix the Rust side puts on a `thread/revert` error when this Codex
 *  predates the API (0.146.0 and earlier), as opposed to a revert that failed.
 *  Kept in step with `Feature::REVERT` in `src-tauri/src/codex/compat.rs`. */
export const REVERT_UNSUPPORTED = "codex-revert-unsupported";

export const isRevertUnsupported = (cause: unknown) => isUnsupported(cause, REVERT_UNSUPPORTED);

/** Prefix the Rust side puts on a `turn/settings/update` error when this
 *  Codex predates the API (0.150.1 and earlier). Kept in step with
 *  `Feature::TURN_SETTINGS` in `src-tauri/src/codex/compat.rs`. */
export const TURN_SETTINGS_UNSUPPORTED = "codex-turn-settings-unsupported";
export const LIVE_SPEED_UNSUPPORTED = "codex-live-speed-unsupported";

export const isTurnSettingsUnsupported = (cause: unknown) =>
  isUnsupported(cause, TURN_SETTINGS_UNSUPPORTED, LIVE_SPEED_UNSUPPORTED);

type TurnSettingsUpdateStatus = "applied" | "targetUnavailable";

/** Change the model and/or effort of the turn running right now. Rejects
 *  with a `TURN_SETTINGS_UNSUPPORTED` error on a Codex without the API. */
export async function updateTurnSettings(
  threadId: string,
  turnId: string,
  settings: { model?: string | null; effort?: string | null; speedTier?: string },
): Promise<TurnSettingsUpdateStatus> {
  const response = (await commands.updateTurnSettings(
    threadId,
    turnId,
    settings.model ?? null,
    settings.effort ?? null,
    settings.speedTier ?? null,
  )) as { status?: string } | null;
  return response?.status === "applied" ? "applied" : "targetUnavailable";
}

export async function queueList(threadId: string): Promise<QueuedSubmission[]> {
  const submissions: QueuedSubmission[] = [];
  let cursor: string | null = null;
  do {
    const page = (await commands.queueList(threadId, cursor)) as {
      data?: QueuedSubmission[];
      nextCursor?: string | null;
    };
    submissions.push(...(page.data ?? []));
    cursor = page.nextCursor ?? null;
  } while (cursor);
  return submissions;
}

export async function queueAdd(
  threadId: string,
  input: UserInputPart[],
  clientUserMessageId: string,
): Promise<QueuedSubmission> {
  const response = await (commands.queueAdd(threadId, input, clientUserMessageId) as Promise<{
    queuedSubmission: QueuedSubmission;
  }>);
  return response.queuedSubmission;
}

export async function queueUpdate(
  threadId: string,
  queuedSubmissionId: string,
  input: UserInputPart[],
): Promise<QueuedSubmission> {
  const response = await (commands.queueUpdate(threadId, queuedSubmissionId, input) as Promise<{
    queuedSubmission: QueuedSubmission;
  }>);
  return response.queuedSubmission;
}

export async function queueDelete(threadId: string, queuedSubmissionId: string): Promise<boolean> {
  const response = await (commands.queueDelete(threadId, queuedSubmissionId) as Promise<{ deleted?: boolean }>);
  return response.deleted ?? false;
}

/** Browser/Playwright-only: stage from a `File` object (no native path). */
export async function stageBrowserFile(file: File): Promise<Attachment> {
  if (!import.meta.env.DEV) throw new Error("Browser staging is a preview-only path");
  return (await import("$lib/services/preview/attachments")).previewStageFile(file);
}

/** What the running `codex app-server` reported about itself at `initialize`. */
export async function readCodexServerInfo(): Promise<CodexServerInfo> {
  return (await (commands.readCodexServerInfo() as Promise<CodexServerInfo | null>)) ?? {};
}

export async function gitWorktreeAdd(repoDir: string, path: string, branch: WorktreeBranchRequest): Promise<void> {
  await commands.gitWorktreeAdd(repoDir, { path, branch });
}

export async function remotePairingStart(): Promise<PairingInfo> {
  return commands.remotePairingStart() as Promise<PairingInfo>;
}

export async function remotePairingStatus(pairingCode: string): Promise<{ claimed: boolean }> {
  return commands.remotePairingStatus(pairingCode) as Promise<{ claimed: boolean }>;
}

// The default accelerator (documented in settings.rs) that toggles the quick
// composer when nothing is persisted yet.
export const DEFAULT_QUICK_SHORTCUT = "CmdOrCtrl+Shift+Space";

// --- Integrations (MCP servers, skills, plugins) ---

/**
 * Live per-server state from Codex: startup result, the tools each server
 * actually exposes, and auth status. Keyed by server name so callers can join
 * it onto the config-declared list.
 */
export async function listMcpServerStatus(): Promise<Record<string, McpServerStatus>> {
  const response = await (commands.listMcpServerStatus() as Promise<{ data?: McpServerStatus[] }>);
  const byName: Record<string, McpServerStatus> = {};
  for (const entry of response.data ?? []) byName[entry.name] = entry;
  return byName;
}

/** Skills Codex would offer for these directories, for the `$` picker. */
export async function listSkillsFor(cwds: string[]): Promise<SkillSummary[]> {
  const response = await (commands.listSkillsFor(cwds) as Promise<{ data?: { skills?: SkillSummary[] }[] }>);
  const byName = new Map<string, SkillSummary>();
  for (const group of response.data ?? []) {
    for (const skill of group.skills ?? []) {
      if (!byName.has(skill.name)) byName.set(skill.name, skill);
    }
  }
  return [...byName.values()].sort((a, b) => a.name.localeCompare(b.name));
}

export interface NewSkillInput {
  name: string;
  description: string;
  body?: string | null;
}

/**
 * Scaffold `<codex_home>/skills/<name>/SKILL.md`. Returns the refreshed list
 * (Codex is asked to rescan, so the new skill is already present).
 */
export async function createSkill(input: NewSkillInput): Promise<IntegrationsList> {
  return commands.createSkill(input.name, input.description, input.body ?? null, null);
}

/**
 * Create an MCP server, or save edits to an existing one.
 *
 * `previousName` is what tells the two apart — pass the name the server is
 * currently stored under to edit it, which is also how a rename is expressed.
 * Transport follows the populated fields: `command` means stdio, `url` means
 * streamable HTTP. `envKeys` is the full desired set of env variable names:
 * values left blank keep the stored secret (which the UI never receives) and
 * names missing from the set are dropped.
 */
/** An MCP server to add, or to edit when `previousName` names the existing entry. */
export interface McpServerInput {
  previousName?: string | null;
  name: string;
  command?: string | null;
  args?: string[];
  env?: Record<string, string>;
  envKeys?: string[] | null;
  url?: string | null;
  bearerTokenEnvVar?: string | null;
}

export async function saveMcpServer(input: McpServerInput): Promise<IntegrationsList> {
  return commands.saveMcpServer({
    previousName: input.previousName ?? null,
    name: input.name,
    command: input.command ?? null,
    args: input.args ?? [],
    env: input.env ?? {},
    envKeys: input.envKeys ?? null,
    url: input.url ?? null,
    bearerTokenEnvVar: input.bearerTokenEnvVar ?? null,
  });
}
