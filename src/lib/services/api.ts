// Typed wrappers over the generated commands. The `commands` proxy in
// `homeRouting` routes each call to the right Home, or, in the browser
// preview, to the fakes in `preview/commands.ts`; the wrappers here only add
// defaults, unwrap wire shapes and name results.

import { convertFileSrc } from "@tauri-apps/api/core";
import type { ClaudeStatus, HarnessKind } from "$lib/bindings";
import { addProfileProject, commands } from "$lib/services/homeRouting";
import { isTauri } from "$lib/services/tauri";
import type {
  AccountRateLimits,
  AgentRun,
  AgentSettings,
  Attachment,
  BootstrapData,
  ChangesSummary,
  CodexServerInfo,
  CommitResult,
  ConfigSetting,
  ContextComposition,
  CreateWorkspaceInput,
  FileDiff,
  FileHit,
  FilePreview,
  GitBranch,
  GitCommit,
  GitContext,
  GitRepoInfo,
  GitStatus,
  HomeOverview,
  Host,
  IntegrationsList,
  LaunchState,
  McpServerStatus,
  MemoryHome,
  Model,
  PairingInfo,
  PendingComment,
  PrDetail,
  PrFile,
  PrFreshness,
  ProjectSource,
  PromptPart,
  ProviderStatus,
  PrSummary,
  QueuedSubmission,
  RemoteConnection,
  ReviewDraft,
  ReviewTarget,
  RuntimeSettings,
  SidebarItemRef,
  SkillSummary,
  SubagentDetail,
  SubagentPolicy,
  SyncResult,
  ThreadDetail,
  ThreadGoal,
  ThreadItem,
  ThreadSearchFilter,
  ThreadSearchPage,
  ThreadsPage,
  ThreadUsage,
  Turn,
  TurnOptions,
  UsageBreakdown,
  UsageScope,
  UserInputPart,
  WireMessage,
  WorkspaceSearchResults,
  WorktreeBranchRequest,
  WorktreeEntry,
  WorktreeHandoffPreflight,
} from "$lib/types";
import { errorMessage } from "$lib/utils/errors";

export type { ClaudeStatus, HarnessKind } from "$lib/bindings";
export { isTauri } from "$lib/services/tauri";

export async function bootstrap(): Promise<BootstrapData> {
  return commands.bootstrap();
}

export async function saveProject(path: string, host: Host | null = null): Promise<BootstrapData> {
  return addProfileProject(path, host);
}

/** Adopt an existing linked worktree as a worktree project. */
export async function addWorktreeProject(path: string): Promise<BootstrapData> {
  return commands.addWorktreeProject(path);
}

/** Create a durable virtual project whose members are real directories or
 * isolated Git worktrees. The backend owns path validation and materializes
 * its writable hub under the active CODEX_HOME. */
export async function createWorkspace(input: CreateWorkspaceInput): Promise<BootstrapData> {
  return commands.createWorkspace(input);
}

export async function updateWorkspace(workspaceId: string, input: CreateWorkspaceInput): Promise<BootstrapData> {
  return commands.updateWorkspace({ ...input, workspaceId });
}

export async function moveThreadToWorkspace(threadId: string, workspaceId: string): Promise<BootstrapData> {
  return commands.moveThreadToWorkspace(threadId, workspaceId);
}

// --- Thread sections (`threadSection/*`, Codex ≥0.149) -----------------------

export async function createThreadSection(name: string, color: string | null): Promise<BootstrapData> {
  return commands.createThreadSection(name, color);
}

export async function updateThreadSection(
  sectionId: string,
  name: string,
  color: string | null,
): Promise<BootstrapData> {
  return commands.updateThreadSection(sectionId, name, color);
}

export async function deleteThreadSection(sectionId: string): Promise<BootstrapData> {
  return commands.deleteThreadSection(sectionId);
}

/** Move a thread into `sectionId`, or out of any section with `null`. */
export async function moveThreadToSection(threadId: string, sectionId: string | null): Promise<BootstrapData> {
  return commands.moveThreadToSection(threadId, sectionId);
}

export async function renameProject(path: string, name: string): Promise<BootstrapData> {
  return commands.renameProject(path, name);
}

export async function renameThread(threadId: string, name: string): Promise<BootstrapData> {
  return commands.renameThread(threadId, name);
}

/** Ask the backend to generate this thread's sidebar title. `seed` is the user's
 *  opening message; omit it to have the backend read the thread instead. Resolves
 *  to `null` when naming was skipped or did not produce a usable title. */
export async function autoNameThread(threadId: string, seed?: string): Promise<BootstrapData | null> {
  return commands.autoNameThread(threadId, seed ?? null);
}

export async function setProjectPinned(path: string, pinned: boolean): Promise<BootstrapData> {
  return commands.setProjectPinned(path, pinned);
}

export async function setProjectArchived(path: string, archived: boolean): Promise<BootstrapData> {
  return commands.setProjectArchived(path, archived);
}

/** Persist the sidebar expansion state without reloading the project tree. */
export async function setProjectExpanded(path: string, expanded: boolean): Promise<void> {
  await commands.setProjectExpanded(path, expanded);
}

export async function setThreadPinned(threadId: string, pinned: boolean): Promise<BootstrapData> {
  return commands.setThreadPinned(threadId, pinned);
}

/** Fold threads out of the sidebar ("Hide thread"), or bring them back. */
export async function setThreadsHidden(threadIds: string[], hidden: boolean): Promise<BootstrapData> {
  return commands.setThreadsHidden(threadIds, hidden);
}

/** Forget the user's drag ordering in one sidebar scope. */
export async function resetSidebarOrder(scope: string): Promise<BootstrapData> {
  return commands.resetSidebarOrder(scope);
}

/** The session-focus button: hide and collapse in one round trip. */
export async function applySessionFocus(
  hide: string[],
  collapseProjects: string[],
  collapseFolders: string[],
): Promise<BootstrapData> {
  return commands.applySessionFocus(hide, collapseProjects, collapseFolders);
}

export async function revealInFinder(path: string): Promise<void> {
  await commands.revealInFinder(path);
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

/** Open a URL in the user's default browser instead of the app webview. */
export async function openExternalUrl(url: string): Promise<void> {
  await commands.openExternalUrl(url);
}

export async function readThread(threadId: string): Promise<ThreadDetail> {
  return commands.readThread(threadId) as Promise<ThreadDetail>;
}

export async function listSubagents(threadId: string): Promise<SubagentDetail[]> {
  return commands.listSubagents(threadId) as Promise<SubagentDetail[]>;
}

export async function updateSubagentPolicy(
  threadId: string,
  modelPolicy: SubagentPolicy | null,
  reasoningEffortPolicy: SubagentPolicy | null,
): Promise<void> {
  await commands.updateSubagentPolicy(threadId, modelPolicy, reasoningEffortPolicy);
}

interface StartedThread {
  speedTier?: string | null;
  id: string;
  cwd?: string;
  /** Which harness the thread runs on; absent means Codex. */
  harness?: string | null;
}

export async function startThread(
  cwd: string,
  workspaceId?: string | null,
  appSubagents?: boolean | null,
  harness?: HarnessKind | null,
): Promise<StartedThread> {
  return commands.startThread(
    cwd,
    workspaceId ?? null,
    appSubagents ?? null,
    harness ?? null,
  ) as Promise<StartedThread>;
}

/** The models a harness offers. Codex answers `model/list`; Claude has a fixed alias list. */
export async function listHarnessModels(harness: HarnessKind): Promise<Model[]> {
  return normalizeModels((await (commands.listHarnessModels(harness) as Promise<{ data?: ModelWire[] }>)).data);
}

/** Whether one registered Home's `claude` binary is installed and logged in. */
export async function readHomeClaudeStatus(id: string): Promise<ClaudeStatus> {
  return commands.readHomeClaudeStatus(id);
}

export async function startTurn(threadId: string, input: UserInputPart[], options?: TurnOptions): Promise<Turn> {
  return commands.startTurn(threadId, input, options ?? null) as Promise<Turn>;
}

export async function interruptTurn(threadId: string, turnId: string): Promise<void> {
  await commands.interruptTurn(threadId, turnId);
}

export async function respondApproval(
  requestId: number,
  /** A Codex decision word, or an option id from another harness's request. */
  decision: string,
): Promise<void> {
  await commands.respondApproval(requestId, decision);
}

/**
 * Answer a server request whose result is not a bare `{decision}` — a
 * permission grant, an MCP elicitation. Each has its own response shape, so the
 * caller passes the whole object rather than growing a command per method.
 */
export async function respondServerRequest(requestId: number, result: unknown): Promise<void> {
  await commands.respondServerRequest(requestId, result);
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

/** Threads holding a question that was never answered. */
export async function threadsWithUnansweredQuestions(): Promise<string[]> {
  return await commands.threadsWithUnansweredQuestions();
}

/** Threads with a turn running right now on the backend's Codex child. */
export async function threadsWithActiveTurns(): Promise<string[]> {
  return await commands.threadsWithActiveTurns();
}

/** Summarise the thread so far and drop the raw history from the model's context. */
export async function compactThread(threadId: string): Promise<void> {
  await commands.compactThread(threadId);
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

/** Drop the thread's goal (`/goal clear`). */
export async function clearThreadGoal(threadId: string): Promise<void> {
  await commands.threadGoalClear(threadId);
}

export async function invalidateThreadCache(threadId: string): Promise<void> {
  await commands.invalidateThreadCache(threadId);
}

export async function archiveThread(threadId: string): Promise<BootstrapData> {
  return commands.archiveThread(threadId);
}

export async function deleteThread(threadId: string): Promise<BootstrapData> {
  return commands.deleteThread(threadId);
}

export async function removeProject(path: string): Promise<BootstrapData> {
  return commands.removeProject(path);
}

/** Create a sidebar folder. `scope` is "" for the top level or a project path. */
export async function createSidebarFolder(
  scope: string,
  parentId: string | null,
  name: string,
): Promise<BootstrapData> {
  return commands.createSidebarFolder(scope, parentId, name);
}

export async function renameSidebarFolder(id: string, name: string): Promise<BootstrapData> {
  return commands.renameSidebarFolder(id, name);
}

/** Remove a folder; whatever it held moves up to its parent. */
export async function deleteSidebarFolder(id: string): Promise<BootstrapData> {
  return commands.deleteSidebarFolder(id);
}

/** Persist a folder's open/closed state without reloading the tree. */
export async function setSidebarFolderExpanded(id: string, expanded: boolean): Promise<void> {
  await commands.setSidebarFolderExpanded(id, expanded);
}

/** Put `item` under `parentId` with `siblings` as the parent's full new order. */
export async function placeSidebarItem(
  scope: string,
  item: SidebarItemRef,
  parentId: string | null,
  siblings: SidebarItemRef[],
): Promise<BootstrapData> {
  return commands.placeSidebarItem(scope, item, parentId, siblings);
}

export async function unarchiveThread(threadId: string): Promise<BootstrapData> {
  return commands.unarchiveThread(threadId);
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

/** Token usage summed over a thread, a project, or the whole Home, with the
 *  ledger's category attribution. `since` is a unix time in seconds. */
export async function readUsageBreakdown(scope: UsageScope, since: number | null = null): Promise<UsageBreakdown> {
  return commands.readUsageBreakdown(scope, since);
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

/** The exact composition of a live thread's context, as the harness reports
 *  it. Rejects with a `CONTEXT_BREAKDOWN_UNSUPPORTED` error when it cannot;
 *  callers fall back to the estimate in `readUsageBreakdown`. */
export async function readContextBreakdown(threadId: string): Promise<ContextComposition> {
  return commands.readContextBreakdown(threadId);
}

/** Prefix the Rust side puts on `read_prompt_parts_text` when the thread has
 *  no Codex rollout to read text from — a Claude thread, or a Codex thread
 *  with nothing on disk yet. Kept in step with `PROMPT_TEXT_UNSUPPORTED` in
 *  `src-tauri/src/usage/commands.rs`. */
export const PROMPT_TEXT_UNSUPPORTED = "harness-unsupported:prompt_parts_text";

export const isPromptPartsTextUnsupported = (cause: unknown) => isUnsupported(cause, PROMPT_TEXT_UNSUPPORTED);

/** The full text of a Codex thread's system-prompt parts, fetched on demand
 *  — never bundled into `readContextBreakdown`/`readUsageBreakdown`, which
 *  are polled on every token update. Rejects with `PROMPT_TEXT_UNSUPPORTED`
 *  for a Claude thread or one with no rollout on disk. */
export async function readPromptPartsText(threadId: string): Promise<PromptPart[]> {
  return commands.readPromptPartsText(threadId);
}

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

export async function queueReorder(threadId: string, queuedSubmissionIds: string[]): Promise<void> {
  await commands.queueReorder(threadId, queuedSubmissionIds);
}

export async function sideQuestionMode(parentThreadId: string): Promise<"fork" | "native"> {
  return commands.sideQuestionMode(parentThreadId);
}

export async function addNativeSideQuestion(parentThreadId: string, sideThreadId: string, title: string) {
  return commands.addNativeSideQuestion(parentThreadId, sideThreadId, title);
}

export async function readNativeSideQuestion(parentThreadId: string, sideThreadId: string) {
  return commands.readNativeSideQuestion(parentThreadId, sideThreadId);
}

export async function prepareNativeSideQuestion(parentThreadId: string, sideThreadId: string, question: string) {
  return commands.prepareNativeSideQuestion(parentThreadId, sideThreadId, question);
}

export async function answerNativeSideQuestion(parentThreadId: string, sideThreadId: string, requestId: string) {
  return commands.answerNativeSideQuestion(parentThreadId, sideThreadId, requestId);
}

export async function cancelNativeSideQuestion(parentThreadId: string, sideThreadId: string, requestId: string) {
  return commands.cancelNativeSideQuestion(parentThreadId, sideThreadId, requestId);
}

export async function addSideQuestion(
  parentThreadId: string,
  sideThreadId: string,
  title: string,
  inheritedTurns: number,
): Promise<BootstrapData> {
  return commands.addSideQuestion(parentThreadId, sideThreadId, title, inheritedTurns);
}

/** Record a fork as a version of the message whose turn it replaced. */
export async function addThreadBranch(
  parentThreadId: string,
  threadId: string,
  replacedTurnId: string,
  inheritedTurns: number,
): Promise<BootstrapData> {
  return commands.addThreadBranch(parentThreadId, threadId, replacedTurnId, inheritedTurns);
}

/** Remember which turn in a branch carries the edited message. */
export async function setThreadBranchEditTurn(threadId: string, editTurnId: string): Promise<void> {
  await commands.setThreadBranchEditTurn(threadId, editTurnId);
}

export async function removeSideQuestion(sideThreadId: string): Promise<BootstrapData> {
  return commands.removeSideQuestion(sideThreadId);
}

export async function searchProjectFiles(root: string, query: string, limit = 20): Promise<FileHit[]> {
  return commands.searchProjectFiles(root, query, limit);
}

// --- Project instructions, sources, and workspace search ---
// Instructions and source indexing are Rust-owned; the renderer never walks the
// filesystem.

export async function saveProjectInstructions(projectPath: string, instructions: string): Promise<void> {
  await commands.saveProjectInstructions(projectPath, instructions);
}

export async function listProjectSources(projectPath: string): Promise<ProjectSource[]> {
  return commands.listProjectSources(projectPath);
}

export async function addProjectSource(
  projectPath: string,
  sourcePath: string,
  kind: "folder" | "file",
): Promise<ProjectSource[]> {
  return commands.addProjectSource(projectPath, sourcePath, kind);
}

export async function removeProjectSource(id: string, projectPath: string): Promise<ProjectSource[]> {
  return commands.removeProjectSource(id, projectPath);
}

export async function reindexSource(id: string): Promise<void> {
  await commands.reindexSource(id);
}

export async function searchWorkspace(
  projectPath: string,
  query: string,
  cursor?: string | null,
  generation = 0,
): Promise<WorkspaceSearchResults> {
  return commands.searchWorkspace(projectPath, query, cursor ?? null, generation);
}

// --- Attachments (staging) ---
// Native staging validates size/type in Rust and copies into a bounded
// `<codex_home>/staging/` directory; only safe references reach `turn/start`.

export async function stageAttachment(sourcePath: string): Promise<Attachment> {
  return commands.stageAttachment(sourcePath);
}

export async function stageClipboardImage(filename: string, mime: string, bytes: number[]): Promise<Attachment> {
  return commands.stageClipboardImage(filename, mime, bytes);
}

/** Browser/Playwright-only: stage from a `File` object (no native path). */
export async function stageBrowserFile(file: File): Promise<Attachment> {
  if (!import.meta.env.DEV) throw new Error("Browser staging is a preview-only path");
  return (await import("$lib/services/preview/attachments")).previewStageFile(file);
}

export async function removeStagedAttachment(id: string): Promise<void> {
  await commands.removeStaged(id);
}

export async function openInZed(path: string): Promise<void> {
  await commands.openInZed(path);
}

// Per-project message drafts. Each project's draft lives in its own nested
// folder under the app's data directory. `content` is the serialized composer
// parts (JSON).

export async function saveDraft(project: string, content: string): Promise<void> {
  await commands.saveDraft(project, content);
}

export async function loadDraft(project: string): Promise<string | null> {
  return commands.loadDraft(project);
}

export async function deleteDraft(project: string): Promise<void> {
  await commands.deleteDraft(project);
}

export async function listProjectFiles(root: string): Promise<string[]> {
  return commands.listProjectFiles(root);
}

export async function readRuntimeSettings(): Promise<RuntimeSettings> {
  return commands.readRuntimeSettings();
}

/** What the running `codex app-server` reported about itself at `initialize`. */
export async function readCodexServerInfo(): Promise<CodexServerInfo> {
  return (await (commands.readCodexServerInfo() as Promise<CodexServerInfo | null>)) ?? {};
}

export async function updateRuntimeSettings(
  codexHome: string | null,
  codexBinary: string | null,
  claudeBinary: string | null = null,
  claudeConfigDir: string | null = null,
  codexHost: Host | null = null,
  claudeHost: Host | null = null,
): Promise<RuntimeSettings> {
  return commands.updateRuntimeSettings(codexHome, codexBinary, claudeBinary, claudeConfigDir, codexHost, claudeHost);
}

/** The WSL distributions installed here; empty anywhere but Windows. */
export async function listWslDistros(): Promise<string[]> {
  return commands.listWslDistros();
}

export async function readLaunchState(): Promise<LaunchState> {
  return commands.readLaunchState();
}

/**
 * Bind this window to a home. `host` says where it lives; `null` lets the
 * backend infer it from the path (a `\\wsl.localhost\..` pick is a WSL home).
 */
export async function selectCodexHome(path: string, host: Host | null = null): Promise<LaunchState> {
  return commands.selectCodexHome(path, host);
}

/**
 * Open another app window, bound to `path` when given (e.g. a second account's
 * Codex home) or showing the launch picker when not. Returns the new window's
 * label.
 */
export async function openHomeWindow(path?: string, host: Host | null = null): Promise<string | null> {
  return commands.openHomeWindow(path ?? null, host);
}

/** Persist and immediately apply a Codex CLI path (null clears the override). */
export async function setCodexBinary(path: string | null): Promise<LaunchState> {
  return commands.setCodexBinary(path);
}

export async function removeRecentHome(path: string, host: Host | null = null): Promise<LaunchState> {
  return commands.removeRecentHome(path, host);
}

export async function readConfigSettings(): Promise<ConfigSetting[]> {
  return commands.readConfigSettings();
}

/** Set (value) or unset (unset:true, inherit default) a whitelisted config.toml key. */
export async function writeConfigSetting(key: string, value: string | null, unset = false): Promise<ConfigSetting[]> {
  return commands.writeConfigSetting(key, value, unset);
}

export async function readHomeOverview(): Promise<HomeOverview> {
  return commands.readHomeOverview();
}

// --- Native Git service ---
// The frontend never shells out to Git; every call goes through these wrappers,
// which the Rust `git` module backs with the installed `git` executable.

export async function gitRepoInfo(dir: string): Promise<GitRepoInfo> {
  return commands.gitRepoInfo(dir);
}

export async function gitStatus(dir: string): Promise<GitStatus> {
  return commands.gitStatus(dir);
}

export async function gitContext(dir: string): Promise<GitContext> {
  return commands.gitContext(dir);
}

export async function gitStage(dir: string, paths: string[]): Promise<void> {
  await commands.gitStage(dir, paths);
}

export async function gitUnstage(dir: string, paths: string[]): Promise<void> {
  await commands.gitUnstage(dir, paths);
}

export async function gitDiscard(dir: string, paths: string[], untrackedPaths: string[]): Promise<void> {
  await commands.gitDiscard(dir, paths, untrackedPaths);
}

export async function gitCommit(dir: string, message: string): Promise<CommitResult> {
  return commands.gitCommit(dir, message);
}

export async function gitFetch(dir: string): Promise<SyncResult> {
  return commands.gitFetch(dir);
}

export async function gitPull(dir: string): Promise<SyncResult> {
  return commands.gitPull(dir);
}

export async function gitPush(dir: string, setUpstream: boolean): Promise<SyncResult> {
  return commands.gitPush(dir, setUpstream);
}

export async function gitCheckoutBranch(dir: string, name: string, force: boolean): Promise<void> {
  await commands.gitCheckoutBranch(dir, name, force);
}

export async function gitCreateBranch(
  dir: string,
  name: string,
  base: string | null,
  checkout: boolean,
): Promise<void> {
  await commands.gitCreateBranch(dir, name, base, checkout);
}

export async function gitStagedFileDiff(dir: string, path: string, maxBytes?: number): Promise<FileDiff> {
  return commands.gitStagedFileDiff(dir, path, maxBytes ?? null);
}

export async function gitWorktrees(repoDir: string): Promise<WorktreeEntry[]> {
  return commands.gitWorktrees(repoDir);
}

export async function gitChangesSummary(dir: string): Promise<ChangesSummary> {
  return commands.gitChangesSummary(dir);
}

export async function gitFileDiff(
  dir: string,
  base: string,
  path: string,
  untracked: boolean,
  maxBytes?: number,
): Promise<FileDiff> {
  return commands.gitFileDiff(dir, base, path, untracked, maxBytes ?? null);
}

export async function gitWorktreeHandoffPreflight(
  worktreePath: string,
  targetDir: string,
): Promise<WorktreeHandoffPreflight> {
  return commands.gitWorktreeHandoffPreflight(worktreePath, targetDir);
}

/** Check the temp worktree's branch out in `targetDir` and remove the worktree. Returns the branch. */
export async function gitWorktreeHandoff(
  worktreePath: string,
  targetDir: string,
  commitUncommitted: boolean,
  branchName?: string | null,
): Promise<string> {
  return commands.gitWorktreeHandoff(worktreePath, targetDir, commitUncommitted, branchName ?? null);
}

export async function gitRecentCommits(dir: string, limit = 20): Promise<GitCommit[]> {
  return commands.gitRecentCommits(dir, limit);
}

export async function gitBranches(dir: string, limit = 200): Promise<GitBranch[]> {
  return commands.gitBranches(dir, limit);
}

export async function gitWorktreeAdd(repoDir: string, path: string, branch: WorktreeBranchRequest): Promise<void> {
  await commands.gitWorktreeAdd(repoDir, { path, branch });
}

export async function gitWorktreeRemove(repoDir: string, path: string, force: boolean): Promise<void> {
  await commands.gitWorktreeRemove(repoDir, path, force);
}

export async function gitWorktreePrune(repoDir: string): Promise<void> {
  await commands.gitWorktreePrune(repoDir);
}

export async function gitWorktreeLock(repoDir: string, path: string, reason?: string): Promise<void> {
  await commands.gitWorktreeLock(repoDir, path, reason ?? null);
}

export async function gitWorktreeUnlock(repoDir: string, path: string): Promise<void> {
  await commands.gitWorktreeUnlock(repoDir, path);
}

// --- CLI / desktop handoff ---
// The reproducible command and shareable link are built in Rust (proper shell
// quoting / URL encoding); the frontend only requests and copies them.

export async function handoffCommand(threadId: string, cwd: string): Promise<string> {
  return commands.handoffCommand(threadId, cwd);
}

export async function handoffThreadLink(threadId: string, cwd: string, label?: string): Promise<string> {
  return commands.handoffThreadLink(threadId, cwd, label ?? null);
}

/** Copy arbitrary text to the system clipboard. */
export const copyText = handoffCopy;

export async function handoffCopy(text: string): Promise<void> {
  await commands.handoffCopy(text);
}

export async function handoffLaunchTerminal(threadId: string, cwd: string): Promise<void> {
  await commands.handoffLaunchTerminal(threadId, cwd);
}

export async function remotePairingStart(): Promise<PairingInfo> {
  return commands.remotePairingStart() as Promise<PairingInfo>;
}

export async function remotePairingStatus(pairingCode: string): Promise<{ claimed: boolean }> {
  return commands.remotePairingStatus(pairingCode) as Promise<{ claimed: boolean }>;
}

// --- Pull-request review (provider-neutral; GitHub adapter via `gh`) ---
// The frontend never shells out to `gh`; every call goes through these wrappers,
// which the Rust `review` module backs.

export async function reviewProviderStatus(repoDir: string): Promise<ProviderStatus> {
  return commands.reviewProviderStatus(repoDir);
}

export async function reviewListPrs(repoDir: string): Promise<PrSummary[]> {
  return commands.reviewListPrs(repoDir);
}

export async function reviewPrDetail(repoDir: string, number: number): Promise<PrDetail> {
  return commands.reviewPrDetail(repoDir, number);
}

export async function reviewCheckFresh(
  repoDir: string,
  number: number,
  knownHead: string,
  knownUpdatedAt: string,
): Promise<PrFreshness> {
  return commands.reviewCheckFresh(repoDir, number, knownHead, knownUpdatedAt);
}

export async function reviewLocalDiff(repoDir: string, base: string, head?: string): Promise<PrFile[]> {
  return commands.reviewLocalDiff(repoDir, base, head ?? null);
}

export async function reviewSubmit(
  repoDir: string,
  number: number,
  event: string,
  body: string,
  comments: PendingComment[],
): Promise<void> {
  await commands.reviewSubmit(repoDir, number, event, body, comments);
}

export async function reviewReply(repoDir: string, number: number, commentId: number, body: string): Promise<void> {
  await commands.reviewReply(repoDir, number, commentId, body);
}

export async function reviewResolveThread(repoDir: string, threadId: string): Promise<void> {
  await commands.reviewResolveThread(repoDir, threadId);
}

export async function reviewSaveDraft(
  provider: string,
  repo: string,
  prNumber: number,
  headSha: string,
  payload: string,
): Promise<void> {
  await commands.reviewSaveDraft(provider, repo, prNumber, headSha, payload);
}

export async function reviewLoadDraft(provider: string, repo: string, prNumber: number): Promise<ReviewDraft | null> {
  return commands.reviewLoadDraft(provider, repo, prNumber);
}

export async function reviewDeleteDraft(provider: string, repo: string, prNumber: number): Promise<void> {
  await commands.reviewDeleteDraft(provider, repo, prNumber);
}

// --- Remote connections management ---

export async function listConnections(): Promise<RemoteConnection[]> {
  return commands.listConnections();
}

export async function refreshConnections(): Promise<RemoteConnection[]> {
  return commands.refreshConnections();
}

export async function renameConnection(clientId: string, name: string): Promise<void> {
  await commands.renameConnection(clientId, name);
}

/** Safe: forgets the local record without revoking the credential. */
export async function disconnectConnection(clientId: string): Promise<void> {
  await commands.disconnectConnection(clientId);
}

/** Destructive: revokes the credential on the relay and drops the local record. */
export async function revokeConnection(clientId: string): Promise<void> {
  await commands.revokeConnection(clientId);
}

// --- Quick chat and global hotkeys ---------------------------------------
// The default accelerator (documented in settings.rs) that toggles the quick
// composer when nothing is persisted yet.
export const DEFAULT_QUICK_SHORTCUT = "CmdOrCtrl+Shift+Space";

/** The global shortcut currently bound to the quick-chat window. */
export async function getQuickShortcut(): Promise<string> {
  return commands.getQuickShortcut();
}

/**
 * Persist and re-register the quick-chat shortcut. Rejects (with the OS
 * conflict message) when the accelerator is invalid or already taken.
 */
export async function setQuickShortcut(accelerator: string): Promise<string> {
  return commands.setQuickShortcut(accelerator);
}

/** Hand a quick-window thread back to the main window and navigate to it. */
export async function quickOpenFullThread(threadId: string): Promise<void> {
  await commands.quickOpenFullThread(threadId);
}

// --- App-owned subagents ---

/**
 * Every agent a thread has spawned, oldest first. Read from the database so
 * runs from earlier app launches are included; live changes arrive separately
 * on the `codex:agentRun` event.
 */
export async function listAgentRuns(threadId: string): Promise<AgentRun[]> {
  return commands.listAgentRuns(threadId);
}

/** Stop a running agent and kill its process. */
export async function killAgentRun(runId: string): Promise<void> {
  await commands.killAgentRun(runId);
}

export async function readAgentSettings(): Promise<AgentSettings> {
  return commands.readAgentSettings() as Promise<AgentSettings>;
}

export async function writeAgentSettings(settings: AgentSettings): Promise<AgentSettings> {
  return commands.writeAgentSettings(settings) as Promise<AgentSettings>;
}

// --- Integrations (MCP servers, skills, plugins) ---

/**
 * Config-declared servers plus Codex's skill list. `cwds` scopes the skill
 * lookup — pass the active project directory to pick up project skills.
 */
export async function listIntegrations(cwds: string[] = [], forceReload = false): Promise<IntegrationsList> {
  return commands.listIntegrations(cwds, forceReload);
}

export async function setIntegrationEnabled(
  kind: "mcp" | "plugin",
  id: string,
  enabled: boolean | null,
  projectPath: string | null,
): Promise<IntegrationsList> {
  return commands.setIntegrationEnabled(kind, id, enabled, projectPath);
}

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

/**
 * Start an OAuth login for one server. Resolves once Codex has the flow under
 * way (it opens the browser) — success arrives later as an
 * `mcpServer/oauthLogin/completed` notification. Streamable-HTTP servers only.
 */
export async function mcpOauthLogin(name: string): Promise<void> {
  await commands.mcpOauthLogin(name);
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

export async function setSkillEnabled(name: string, enabled: boolean, path: string | null = null): Promise<void> {
  await commands.setSkillEnabled(name, enabled, path);
}

/** Raw `SKILL.md` text for a skill; `path` is the directory or the file. */
export async function readSkill(path: string): Promise<string> {
  return commands.readSkill(path);
}

/**
 * Read a file for the preview modal. `path` must already be stripped of any
 * `:line` suffix (see `parseFileRef`). The everyday failures — a binary, a
 * deleted path, a directory — come back as `problem`, not as a rejection.
 */
export async function readFilePreview(path: string, maxBytes?: number): Promise<FilePreview> {
  return commands.readFilePreview(path, maxBytes ?? null);
}

/**
 * Scaffold `<codex_home>/skills/<name>/SKILL.md`. Returns the refreshed list
 * (Codex is asked to rescan, so the new skill is already present).
 */
export async function createSkill(input: {
  name: string;
  description: string;
  body?: string | null;
}): Promise<IntegrationsList> {
  return commands.createSkill(input.name, input.description, input.body ?? null, null);
}

/** Delete a user-scope skill directory. Refused for anything outside `~/.codex/skills`. */
export async function deleteSkill(path: string): Promise<IntegrationsList> {
  return commands.deleteSkill(path, null);
}

/** The memories of every Home in the Profile. `projects` name Claude's
 *  per-project stores. */
export async function listMemories(projects: string[]): Promise<MemoryHome[]> {
  return commands.listMemories(projects);
}

export async function readMemory(homeId: string, path: string): Promise<string> {
  return commands.readMemory(homeId, path);
}

export async function writeMemory(
  homeId: string,
  path: string,
  contents: string,
  projects: string[],
): Promise<MemoryHome[]> {
  return commands.writeMemory(homeId, path, contents, projects);
}

/** Delete a memory; a Claude memory's `MEMORY.md` line goes with it. */
export async function deleteMemory(homeId: string, path: string, projects: string[]): Promise<MemoryHome[]> {
  return commands.deleteMemory(homeId, path, projects);
}

export async function setMemoryToggle(
  homeId: string,
  key: string,
  value: boolean,
  projects: string[],
): Promise<MemoryHome[]> {
  return commands.setMemoryToggle(homeId, key, value, projects);
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
export async function saveMcpServer(input: {
  previousName?: string | null;
  name: string;
  command?: string | null;
  args?: string[];
  env?: Record<string, string>;
  envKeys?: string[] | null;
  url?: string | null;
  bearerTokenEnvVar?: string | null;
}): Promise<IntegrationsList> {
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

export async function removeMcpServer(name: string): Promise<IntegrationsList> {
  return commands.removeMcpServer(name);
}

export async function setMcpEnabled(name: string, enabled: boolean): Promise<IntegrationsList> {
  return commands.setMcpEnabled(name, enabled);
}

// --- History search and pagination (feature 11) ---

/**
 * Search the local thread index. `generation` is echoed back unchanged so the
 * caller can discard responses that a later keystroke has already superseded.
 */
export async function searchThreads(
  query: string,
  cursor: string | null,
  filter: ThreadSearchFilter,
  generation: number,
): Promise<ThreadSearchPage> {
  return commands.searchThreads(query, cursor, filter, generation);
}

/** Page through the app-server thread listing, forwarding its opaque cursor. */
export async function listThreadsPage(
  cursor: string | null,
  pageSize: number,
  archived: boolean,
  projectPath: string | null,
): Promise<ThreadsPage> {
  return commands.listThreadsPage(cursor, pageSize, archived, projectPath);
}

// --- Message log (Advanced settings) ---

/**
 * Start or stop recording the JSON-RPC traffic with the app-server. Recording
 * is off by default; stopping also discards whatever was captured.
 */
export async function setWireLogging(enabled: boolean): Promise<void> {
  await commands.setWireLogging(enabled);
}

/** The buffered messages, oldest first. */
export async function readWireLog(): Promise<WireMessage[]> {
  return commands.readWireLog();
}

export async function clearWireLog(): Promise<void> {
  await commands.clearWireLog();
}
