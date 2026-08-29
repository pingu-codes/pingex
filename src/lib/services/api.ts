import { commands } from "$lib/bindings";
import { deleteFromLayout, emptyLayout, nextOrdinal, placeInLayout } from "$lib/layout/sidebarTree";
import { previewStageBytes, previewStageFile, previewStageFromPath } from "$lib/services/preview/attachments";
import {
  nextPreviewId,
  previewAddSource,
  previewAgentRuns,
  previewAgentSettings,
  previewArchived,
  previewBranches,
  previewCommits,
  previewConfigSettings,
  previewConnections,
  previewCreateSkill,
  previewData,
  previewDeleteSkill,
  previewFiles,
  previewGitRepoInfo,
  previewGitStatus,
  previewHomeOverview,
  previewIntegrations,
  previewLaunchState,
  previewListSources,
  previewMcpOauthLogin,
  previewMcpServerStatus,
  previewModels,
  previewPrDetail,
  previewProviderStatus,
  previewPrs,
  previewQueue,
  previewQuickShortcut,
  previewRateLimits,
  previewReadSkill,
  previewReindexSource,
  previewRemoveSource,
  previewRuntimeSettings,
  previewSaveInstructions,
  previewSaveMcpServer,
  previewSearchThreads,
  previewSearchWorkspace,
  previewSort,
  previewThread,
  previewThreadsPage,
  previewThreadUsage,
  previewWireLog,
  previewWorktrees,
} from "$lib/services/preview/fixtures";
import { previewCompact, previewInterrupt, previewStreamTurn } from "$lib/services/preview/stream";
import { isTauri } from "$lib/services/tauri";
import type {
  AccountRateLimits,
  AgentRun,
  AgentSettings,
  ArchivedThread,
  Attachment,
  BinaryStatus,
  BootstrapData,
  ChangesSummary,
  CodexServerInfo,
  ConfigSetting,
  CreateWorkspaceInput,
  FileDiff,
  FileHit,
  GitBranch,
  GitCommit,
  GitRepoInfo,
  GitStatus,
  HomeOverview,
  IntegrationsList,
  LaunchState,
  McpServerStatus,
  Model,
  PairingInfo,
  PendingComment,
  PrDetail,
  PrFile,
  PrFreshness,
  ProjectSource,
  ProviderStatus,
  PrSummary,
  QueuedSubmission,
  RemoteConnection,
  ReviewDraft,
  ReviewTarget,
  RuntimeSettings,
  SidebarItemRef,
  SidebarLayout,
  SkillSummary,
  SubagentDetail,
  SubagentPolicy,
  ThreadDetail,
  ThreadGoal,
  ThreadItem,
  ThreadSearchFilter,
  ThreadSearchPage,
  ThreadSummary,
  ThreadsPage,
  ThreadUsage,
  Turn,
  TurnOptions,
  UserInputPart,
  WireMessage,
  WorkspaceSearchResults,
  WorktreeBranchRequest,
  WorktreeEntry,
  WorktreeHandoffPreflight,
} from "$lib/types";

export { isTauri } from "$lib/services/tauri";

export async function bootstrap(): Promise<BootstrapData> {
  if (!isTauri()) return previewData;
  return commands.bootstrap();
}

export async function saveProject(path: string): Promise<BootstrapData> {
  return commands.addProject(path);
}

/** Create a durable virtual project whose members are real directories or
 * isolated Git worktrees. The backend owns path validation and materializes
 * its writable hub under the active CODEX_HOME. */
export async function createWorkspace(input: CreateWorkspaceInput): Promise<BootstrapData> {
  if (!isTauri()) return previewSort();
  return commands.createWorkspace(input);
}

export async function updateWorkspace(workspaceId: string, input: CreateWorkspaceInput): Promise<BootstrapData> {
  if (!isTauri()) return previewSort();
  return commands.updateWorkspace({ ...input, workspaceId });
}

export async function moveThreadToWorkspace(threadId: string, workspaceId: string): Promise<BootstrapData> {
  if (!isTauri()) return previewSort();
  return commands.moveThreadToWorkspace(threadId, workspaceId);
}

// --- Thread sections (`threadSection/*`, Codex ≥0.149) -----------------------

export async function createThreadSection(name: string, color: string | null): Promise<BootstrapData> {
  if (!isTauri()) {
    previewData.sections = [...(previewData.sections ?? []), { id: `section-${Date.now()}`, name, icon: null, color }];
    return previewSort();
  }
  return commands.createThreadSection(name, color);
}

export async function updateThreadSection(
  sectionId: string,
  name: string,
  color: string | null,
): Promise<BootstrapData> {
  if (!isTauri()) {
    const section = previewData.sections?.find((section) => section.id === sectionId);
    if (section) Object.assign(section, { name, color });
    return previewSort();
  }
  return commands.updateThreadSection(sectionId, name, color);
}

export async function deleteThreadSection(sectionId: string): Promise<BootstrapData> {
  if (!isTauri()) {
    previewData.sections = (previewData.sections ?? []).filter((section) => section.id !== sectionId);
    for (const project of previewData.projects) {
      for (const thread of project.threads) if (thread.sectionId === sectionId) thread.sectionId = null;
    }
    return previewSort();
  }
  return commands.deleteThreadSection(sectionId);
}

/** Move a thread into `sectionId`, or out of any section with `null`. */
export async function moveThreadToSection(threadId: string, sectionId: string | null): Promise<BootstrapData> {
  if (!isTauri()) {
    for (const project of previewData.projects) {
      const thread = project.threads.find((thread) => thread.id === threadId);
      if (thread) thread.sectionId = sectionId;
    }
    return previewSort();
  }
  return commands.moveThreadToSection(threadId, sectionId);
}

export async function renameProject(path: string, name: string): Promise<BootstrapData> {
  if (!isTauri()) {
    const project = previewData.projects.find((project) => project.path === path);
    if (project && name.trim()) project.name = name.trim();
    return previewSort();
  }
  return commands.renameProject(path, name);
}

export async function renameThread(threadId: string, name: string): Promise<BootstrapData> {
  if (!isTauri()) {
    for (const project of previewData.projects) {
      const thread = project.threads.find((thread) => thread.id === threadId);
      if (thread && name.trim()) thread.title = name.trim();
    }
    return previewSort();
  }
  return commands.renameThread(threadId, name);
}

/** Ask the backend to generate this thread's sidebar title. `seed` is the user's
 *  opening message; omit it to have the backend read the thread instead. Resolves
 *  to `null` when naming was skipped or did not produce a usable title. */
export async function autoNameThread(threadId: string, seed?: string): Promise<BootstrapData | null> {
  if (!isTauri()) return null;
  return commands.autoNameThread(threadId, seed ?? null);
}

export async function setProjectPinned(path: string, pinned: boolean): Promise<BootstrapData> {
  if (!isTauri()) {
    const project = previewData.projects.find((project) => project.path === path);
    if (project) project.pinned = pinned;
    return previewSort();
  }
  return commands.setProjectPinned(path, pinned);
}

export async function setProjectArchived(path: string, archived: boolean): Promise<BootstrapData> {
  if (!isTauri()) {
    const project = previewData.projects.find((project) => project.path === path);
    if (project) project.archived = archived;
    return previewSort();
  }
  return commands.setProjectArchived(path, archived);
}

/** Persist the sidebar expansion state without reloading the project tree. */
export async function setProjectExpanded(path: string, expanded: boolean): Promise<void> {
  if (!isTauri()) {
    const project = previewData.projects.find((project) => project.path === path);
    if (project) project.expanded = expanded;
    return;
  }
  await commands.setProjectExpanded(path, expanded);
}

export async function setThreadPinned(threadId: string, pinned: boolean): Promise<BootstrapData> {
  if (!isTauri()) {
    for (const project of previewData.projects) {
      const thread = project.threads.find((thread) => thread.id === threadId);
      if (thread) thread.pinned = pinned;
    }
    return previewSort();
  }
  return commands.setThreadPinned(threadId, pinned);
}

export async function revealInFinder(path: string): Promise<void> {
  if (!isTauri()) return;
  await commands.revealInFinder(path);
}

/** Open a URL in the user's default browser instead of the app webview. */
export async function openExternalUrl(url: string): Promise<void> {
  if (!isTauri()) {
    window.open(url, "_blank", "noopener,noreferrer");
    return;
  }
  await commands.openExternalUrl(url);
}

export async function readThread(threadId: string): Promise<ThreadDetail> {
  if (!isTauri()) return { ...previewThread, id: threadId };
  return commands.readThread(threadId) as Promise<ThreadDetail>;
}

export async function listSubagents(threadId: string): Promise<SubagentDetail[]> {
  if (!isTauri()) {
    const byId = new Map(previewData.subagents.map((thread) => [thread.id, thread]));
    const isDescendant = (thread: ThreadSummary) => {
      let parent = thread.parentThreadId ?? null;
      while (parent) {
        if (parent === threadId) return true;
        parent = byId.get(parent)?.parentThreadId ?? null;
      }
      return false;
    };
    return previewData.subagents.filter(isDescendant).map((thread, index) => ({
      id: thread.id,
      parentThreadId: thread.parentThreadId ?? threadId,
      title: thread.title,
      cwd: thread.cwd,
      status: thread.status,
      agentNickname: thread.agentNickname ?? null,
      agentRole: thread.agentRole ?? null,
      model: index === 0 ? "gpt-5.6-terra" : "gpt-5.6-sol",
      reasoningEffort: index === 0 ? "high" : "xhigh",
    }));
  }
  return commands.listSubagents(threadId) as Promise<SubagentDetail[]>;
}

export async function updateSubagentPolicy(
  threadId: string,
  modelPolicy: SubagentPolicy | null,
  reasoningEffortPolicy: SubagentPolicy | null,
): Promise<void> {
  if (!isTauri()) return;
  await commands.updateSubagentPolicy(threadId, modelPolicy, reasoningEffortPolicy);
}

export interface StartedThread {
  id: string;
  cwd?: string;
  /** Which harness the thread runs on; absent means Codex. */
  harness?: string | null;
}

export type HarnessKind = "codex" | "claude";

export async function startThread(
  cwd: string,
  workspaceId?: string | null,
  appSubagents?: boolean | null,
  harness?: HarnessKind | null,
): Promise<StartedThread> {
  if (!isTauri()) return { id: `preview-${nextPreviewId()}`, cwd, harness: harness ?? null };
  return commands.startThread(
    cwd,
    workspaceId ?? null,
    appSubagents ?? null,
    harness ?? null,
  ) as Promise<StartedThread>;
}

/** The models a harness offers. Codex answers `model/list`; Claude has a fixed alias list. */
export async function listHarnessModels(harness: HarnessKind): Promise<Model[]> {
  if (!isTauri()) return previewModels;
  const response = await (commands.listHarnessModels(harness) as Promise<{ data?: Model[] }>);
  return response.data ?? [];
}

export interface ClaudeStatus {
  available: boolean;
  path: string | null;
  version: string | null;
  configDir: string;
  protocolFloor: string;
  message: string | null;
}

/** Whether a usable `claude` binary is installed. */
export async function readClaudeStatus(): Promise<ClaudeStatus> {
  if (!isTauri()) {
    return {
      available: true,
      path: "/usr/local/bin/claude",
      version: "2.1.251",
      configDir: "~/.claude",
      protocolFloor: "1.0.59",
      message: null,
    };
  }
  return commands.readClaudeStatus();
}

export async function startTurn(threadId: string, input: UserInputPart[], options?: TurnOptions): Promise<Turn> {
  if (!isTauri()) {
    const turnId = `preview-turn-${nextPreviewId()}`;
    const text = input.find((part) => part.type === "text")?.text ?? "";
    previewStreamTurn(threadId, turnId, text);
    return { id: turnId, status: "inProgress", items: [] };
  }
  return commands.startTurn(threadId, input, options ?? null) as Promise<Turn>;
}

export async function interruptTurn(threadId: string, turnId: string): Promise<void> {
  if (!isTauri()) {
    previewInterrupt(threadId, turnId);
    return;
  }
  await commands.interruptTurn(threadId, turnId);
}

export async function respondApproval(
  requestId: number,
  /** A Codex decision word, or an option id from another harness's request. */
  decision: string,
): Promise<void> {
  if (!isTauri()) return;
  await commands.respondApproval(requestId, decision);
}

/**
 * Answer a server request whose result is not a bare `{decision}` — a
 * permission grant, an MCP elicitation. Each has its own response shape, so the
 * caller passes the whole object rather than growing a command per method.
 */
export async function respondServerRequest(requestId: number, result: unknown): Promise<void> {
  if (!isTauri()) return;
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
  if (!isTauri()) return;
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
  if (!isTauri()) return;
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
  if (!isTauri()) return [];
  return await commands.threadsWithUnansweredQuestions();
}

/** Threads with a turn running right now on the backend's Codex child. */
export async function threadsWithActiveTurns(): Promise<string[]> {
  if (!isTauri()) return [];
  return await commands.threadsWithActiveTurns();
}

/** Summarise the thread so far and drop the raw history from the model's context. */
export async function compactThread(threadId: string): Promise<void> {
  if (!isTauri()) {
    previewCompact(threadId);
    return;
  }
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
  if (!isTauri()) {
    return { id: `preview-turn-${nextPreviewId()}`, status: "inProgress", items: [] };
  }
  return commands.startReview(threadId, target) as Promise<Turn>;
}

/** Set or update the goal for a long-running task (`/goal <objective>`). */
export async function setThreadGoal(threadId: string, objective: string): Promise<ThreadGoal> {
  if (!isTauri()) {
    return { threadId, objective, status: "active", tokenBudget: null, tokensUsed: 0, timeUsedSeconds: 0 };
  }
  return commands.threadGoalSet(threadId, objective, null) as Promise<ThreadGoal>;
}

/** Pause or resume the thread's goal without changing its objective. */
export async function setThreadGoalStatus(threadId: string, status: "active" | "paused"): Promise<ThreadGoal> {
  if (!isTauri()) {
    return { threadId, objective: "", status, tokenBudget: null, tokensUsed: 0, timeUsedSeconds: 0 };
  }
  return commands.threadGoalSet(threadId, null, status) as Promise<ThreadGoal>;
}

/** The thread's goal, or null when none is set. */
export async function getThreadGoal(threadId: string): Promise<ThreadGoal | null> {
  if (!isTauri()) return null;
  return commands.threadGoalGet(threadId) as Promise<ThreadGoal | null>;
}

/** Drop the thread's goal (`/goal clear`). */
export async function clearThreadGoal(threadId: string): Promise<void> {
  if (!isTauri()) return;
  await commands.threadGoalClear(threadId);
}

export async function invalidateThreadCache(threadId: string): Promise<void> {
  if (!isTauri()) return;
  await commands.invalidateThreadCache(threadId);
}

export async function archiveThread(threadId: string): Promise<BootstrapData> {
  if (!isTauri()) {
    for (const project of previewData.projects) {
      project.threads = project.threads.filter((thread) => thread.id !== threadId);
    }
    return previewSort();
  }
  return commands.archiveThread(threadId);
}

export async function deleteThread(threadId: string): Promise<BootstrapData> {
  if (!isTauri()) {
    for (const project of previewData.projects) {
      project.threads = project.threads.filter((thread) => thread.id !== threadId);
    }
    return previewSort();
  }
  return commands.deleteThread(threadId);
}

export async function removeProject(path: string): Promise<BootstrapData> {
  if (!isTauri()) {
    previewData.projects = previewData.projects.filter((project) => project.path !== path);
    return previewSort();
  }
  return commands.removeProject(path);
}

/** Create a sidebar folder. `scope` is "" for the top level or a project path. */
export async function createSidebarFolder(
  scope: string,
  parentId: string | null,
  name: string,
): Promise<BootstrapData> {
  if (!isTauri()) {
    const layout = previewLayout();
    layout.folders.push({
      id: `folder-${Date.now()}-${layout.folders.length}`,
      scope,
      parentId,
      name,
      expanded: true,
      ordinal: nextOrdinal(layout, scope, parentId),
    });
    return previewSort();
  }
  return commands.createSidebarFolder(scope, parentId, name);
}

export async function renameSidebarFolder(id: string, name: string): Promise<BootstrapData> {
  if (!isTauri()) {
    const folder = previewLayout().folders.find((folder) => folder.id === id);
    if (folder) folder.name = name;
    return previewSort();
  }
  return commands.renameSidebarFolder(id, name);
}

/** Remove a folder; whatever it held moves up to its parent. */
export async function deleteSidebarFolder(id: string): Promise<BootstrapData> {
  if (!isTauri()) {
    previewData.sidebarLayout = deleteFromLayout(previewLayout(), id);
    return previewSort();
  }
  return commands.deleteSidebarFolder(id);
}

/** Persist a folder's open/closed state without reloading the tree. */
export async function setSidebarFolderExpanded(id: string, expanded: boolean): Promise<void> {
  if (!isTauri()) {
    const folder = previewLayout().folders.find((folder) => folder.id === id);
    if (folder) folder.expanded = expanded;
    return;
  }
  await commands.setSidebarFolderExpanded(id, expanded);
}

/** Put `item` under `parentId` with `siblings` as the parent's full new order. */
export async function placeSidebarItem(
  scope: string,
  item: SidebarItemRef,
  parentId: string | null,
  siblings: SidebarItemRef[],
): Promise<BootstrapData> {
  if (!isTauri()) {
    previewData.sidebarLayout = placeInLayout(previewLayout(), scope, item, parentId, siblings);
    return previewSort();
  }
  return commands.placeSidebarItem(scope, item, parentId, siblings);
}

function previewLayout(): SidebarLayout {
  previewData.sidebarLayout ??= emptyLayout();
  return previewData.sidebarLayout;
}

export async function listArchivedThreads(): Promise<ArchivedThread[]> {
  if (!isTauri()) return previewArchived;
  const response = await (commands.listArchivedThreads() as Promise<{ data?: any[] }>);
  return (response.data ?? []).map((thread) => ({
    id: thread.id ?? "",
    title: (thread.name || thread.preview || "Untitled thread").split("\n")[0].slice(0, 80),
    cwd: thread.cwd ?? "",
    updatedAt: thread.updatedAt ?? 0,
  }));
}

export async function unarchiveThread(threadId: string): Promise<BootstrapData> {
  if (!isTauri()) {
    const index = previewArchived.findIndex((thread) => thread.id === threadId);
    if (index >= 0) previewArchived.splice(index, 1);
    return previewSort();
  }
  return commands.unarchiveThread(threadId);
}

export async function listModels(): Promise<Model[]> {
  if (!isTauri()) return previewModels;
  const response = await (commands.listModels() as Promise<{ data?: Model[] }>);
  return response.data ?? [];
}

export async function readAccountRateLimits(): Promise<AccountRateLimits> {
  if (!isTauri()) return previewRateLimits;
  return commands.readAccountRateLimits() as Promise<AccountRateLimits>;
}

export async function forkThread(
  threadId: string,
  beforeTurnId?: string,
  lastTurnId?: string,
  cwd?: string,
): Promise<StartedThread> {
  if (!isTauri()) return { id: `preview-fork-${nextPreviewId()}`, cwd };
  return commands.forkThread(threadId, beforeTurnId ?? null, lastTurnId ?? null, cwd ?? null) as Promise<StartedThread>;
}

/** Truncate `threadId` by its last `numTurns` turns, keeping the thread id. */
export async function rollbackThread(threadId: string, numTurns: number): Promise<StartedThread> {
  if (!isTauri()) return { id: threadId };
  return commands.rollbackThread(threadId, numTurns) as Promise<StartedThread>;
}

export interface RevertedThread {
  thread: ThreadDetail;
  turnsBackwardsCursor?: string | null;
  itemsBackwardsCursor?: string | null;
}

/** Replace `threadId`'s durable history with the prefix before `beforeTurnId`.
 *  `keptTurnIds` are the surviving turns, used to prune the local journal
 *  (revert's response carries no turns, unlike rollback). */
export async function revertThread(
  threadId: string,
  beforeTurnId: string,
  keptTurnIds: string[],
): Promise<RevertedThread> {
  if (!isTauri()) return { thread: { ...previewThread, id: threadId, turns: [] } };
  return commands.revertThread(threadId, beforeTurnId, keptTurnIds) as Promise<RevertedThread>;
}

export async function readThreadUsage(threadId: string): Promise<ThreadUsage | null> {
  if (!isTauri()) return previewThreadUsage(threadId);
  const response = await (commands.readThreadUsage(threadId) as Promise<{ threadUsage?: ThreadUsage | null }>);
  return response.threadUsage ?? null;
}

/** Prefix the Rust side puts on a queue error when this Codex has no usable
 *  server queue — too old for `thread/queue/*`, missing the experimental
 *  capability, or running without a queue database. Kept in step with
 *  `Feature::QUEUE` in `src-tauri/src/codex/compat.rs`. */
export const QUEUE_UNSUPPORTED = "codex-queue-unsupported";

/** Whether a rejected queue call means "this Codex cannot queue at all", as
 *  opposed to an ordinary failure of a queue that does work. */
export function isQueueUnsupported(cause: unknown): boolean {
  const message = cause instanceof Error ? cause.message : String(cause);
  return message.startsWith(QUEUE_UNSUPPORTED);
}

/** Prefix the Rust side puts on a `thread/revert` error when this Codex
 *  predates the API (0.146.0 and earlier), as opposed to a revert that failed.
 *  Kept in step with `Feature::REVERT` in `src-tauri/src/codex/compat.rs`. */
export const REVERT_UNSUPPORTED = "codex-revert-unsupported";

export function isRevertUnsupported(cause: unknown): boolean {
  const message = cause instanceof Error ? cause.message : String(cause);
  return message.startsWith(REVERT_UNSUPPORTED);
}

/** Prefix the Rust side puts on a `turn/settings/update` error when this
 *  Codex predates the API (0.150.1 and earlier). Kept in step with
 *  `Feature::TURN_SETTINGS` in `src-tauri/src/codex/compat.rs`. */
export const TURN_SETTINGS_UNSUPPORTED = "codex-turn-settings-unsupported";

export function isTurnSettingsUnsupported(cause: unknown): boolean {
  const message = cause instanceof Error ? cause.message : String(cause);
  return message.startsWith(TURN_SETTINGS_UNSUPPORTED);
}

export type TurnSettingsUpdateStatus = "applied" | "targetUnavailable";

/** Change the model and/or effort of the turn running right now. Rejects
 *  with a `TURN_SETTINGS_UNSUPPORTED` error on a Codex without the API. */
export async function updateTurnSettings(
  threadId: string,
  turnId: string,
  settings: { model?: string | null; effort?: string | null },
): Promise<TurnSettingsUpdateStatus> {
  if (!isTauri()) return "applied";
  const response = (await commands.updateTurnSettings(
    threadId,
    turnId,
    settings.model ?? null,
    settings.effort ?? null,
  )) as { status?: string } | null;
  return response?.status === "applied" ? "applied" : "targetUnavailable";
}

export async function queueList(threadId: string): Promise<QueuedSubmission[]> {
  if (!isTauri()) return [...previewQueue(threadId)];
  const submissions: QueuedSubmission[] = [];
  let cursor: string | null = null;
  do {
    const page: { data?: QueuedSubmission[]; nextCursor?: string | null } = (await commands.queueList(
      threadId,
      cursor,
    )) as {
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
  if (!isTauri()) {
    const submission: QueuedSubmission = { id: `preview-queued-${nextPreviewId()}`, input, clientUserMessageId };
    previewQueue(threadId).push(submission);
    return submission;
  }
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
  if (!isTauri()) {
    const queue = previewQueue(threadId);
    const entry = queue.find((item) => item.id === queuedSubmissionId);
    if (entry) entry.input = input;
    return entry ?? { id: queuedSubmissionId, input, clientUserMessageId: "" };
  }
  const response = await (commands.queueUpdate(threadId, queuedSubmissionId, input) as Promise<{
    queuedSubmission: QueuedSubmission;
  }>);
  return response.queuedSubmission;
}

export async function queueDelete(threadId: string, queuedSubmissionId: string): Promise<boolean> {
  if (!isTauri()) {
    const queue = previewQueue(threadId);
    const index = queue.findIndex((item) => item.id === queuedSubmissionId);
    if (index >= 0) queue.splice(index, 1);
    return index >= 0;
  }
  const response = await (commands.queueDelete(threadId, queuedSubmissionId) as Promise<{ deleted?: boolean }>);
  return response.deleted ?? false;
}

export async function queueReorder(threadId: string, queuedSubmissionIds: string[]): Promise<void> {
  if (!isTauri()) {
    const queue = previewQueue(threadId);
    queue.sort((a, b) => queuedSubmissionIds.indexOf(a.id) - queuedSubmissionIds.indexOf(b.id));
    return;
  }
  await commands.queueReorder(threadId, queuedSubmissionIds);
}

/** Start the given queued submission (or the head of the queue) as a turn.
 *  Errors if the thread already has an active or pending turn. */
export async function queueStart(threadId: string, queuedSubmissionId?: string): Promise<Turn> {
  if (!isTauri()) {
    const queue = previewQueue(threadId);
    const index = queuedSubmissionId ? queue.findIndex((item) => item.id === queuedSubmissionId) : 0;
    if (index < 0 || !queue.length) throw new Error("queue is empty");
    queue.splice(index, 1);
    return { id: `preview-turn-${nextPreviewId()}`, status: "inProgress", items: [] };
  }
  const response = await (commands.queueStart(threadId, queuedSubmissionId ?? null) as Promise<{ turn: Turn }>);
  return response.turn;
}

export async function addSideQuestion(
  parentThreadId: string,
  sideThreadId: string,
  title: string,
): Promise<BootstrapData> {
  if (!isTauri()) {
    previewData.sideQuestions.unshift({ parentThreadId, sideThreadId, title, createdAt: Date.now() / 1000 });
    return previewSort();
  }
  return commands.addSideQuestion(parentThreadId, sideThreadId, title);
}

export async function removeSideQuestion(sideThreadId: string): Promise<BootstrapData> {
  if (!isTauri()) {
    previewData.sideQuestions = previewData.sideQuestions.filter((entry) => entry.sideThreadId !== sideThreadId);
    return previewSort();
  }
  return commands.removeSideQuestion(sideThreadId);
}

export async function searchProjectFiles(root: string, query: string, limit = 20): Promise<FileHit[]> {
  if (!isTauri()) {
    const lowered = query.toLowerCase();
    return previewFiles.filter((file) => file.path.toLowerCase().includes(lowered)).slice(0, limit);
  }
  return commands.searchProjectFiles(root, query, limit);
}

// --- Project instructions, sources, and workspace search ---
// Instructions and source indexing are Rust-owned; the renderer never walks the
// filesystem. Browser mode serves in-memory fixtures so the detail view and
// search results render for Playwright without Tauri.

export async function saveProjectInstructions(projectPath: string, instructions: string): Promise<void> {
  if (!isTauri()) {
    previewSaveInstructions(projectPath, instructions);
    return;
  }
  await commands.saveProjectInstructions(projectPath, instructions);
}

export async function listProjectSources(projectPath: string): Promise<ProjectSource[]> {
  if (!isTauri()) return previewListSources(projectPath);
  return commands.listProjectSources(projectPath);
}

export async function addProjectSource(
  projectPath: string,
  sourcePath: string,
  kind: "folder" | "file",
): Promise<ProjectSource[]> {
  if (!isTauri()) return previewAddSource(projectPath, sourcePath, kind);
  return commands.addProjectSource(projectPath, sourcePath, kind);
}

export async function removeProjectSource(id: string, projectPath: string): Promise<ProjectSource[]> {
  if (!isTauri()) return previewRemoveSource(id, projectPath);
  return commands.removeProjectSource(id, projectPath);
}

export async function reindexSource(id: string): Promise<void> {
  if (!isTauri()) {
    previewReindexSource(id);
    return;
  }
  await commands.reindexSource(id);
}

export async function searchWorkspace(
  projectPath: string,
  query: string,
  cursor?: string | null,
  generation = 0,
): Promise<WorkspaceSearchResults> {
  if (!isTauri()) return previewSearchWorkspace(projectPath, query, cursor ?? null, generation);
  return commands.searchWorkspace(projectPath, query, cursor ?? null, generation);
}

// --- Attachments (staging) ---
// Native staging validates size/type in Rust and copies into a bounded
// `<codex_home>/staging/` directory; only safe references reach `turn/start`.
// Browser mode fabricates equivalent metadata so Playwright keeps working.

export async function stageAttachment(sourcePath: string): Promise<Attachment> {
  if (!isTauri()) return previewStageFromPath(sourcePath);
  return commands.stageAttachment(sourcePath);
}

export async function stageClipboardImage(filename: string, mime: string, bytes: number[]): Promise<Attachment> {
  if (!isTauri()) return previewStageBytes(filename, mime, bytes);
  return commands.stageClipboardImage(filename, mime, bytes);
}

/** Browser/Playwright-only: stage from a `File` object (no native path). */
export async function stageBrowserFile(file: File): Promise<Attachment> {
  return previewStageFile(file);
}

export async function removeStagedAttachment(id: string): Promise<void> {
  if (!isTauri()) return;
  await commands.removeStaged(id);
}

export async function openInZed(path: string): Promise<void> {
  if (!isTauri()) return;
  await commands.openInZed(path);
}

// Per-project message drafts. Each project's draft lives in its own nested
// folder under the app's data directory; the browser preview falls back to
// localStorage. `content` is the serialized composer parts (JSON).
const previewDraftKey = (project: string) => `pingex-draft:${project}`;

export async function saveDraft(project: string, content: string): Promise<void> {
  if (!isTauri()) {
    localStorage.setItem(previewDraftKey(project), content);
    return;
  }
  await commands.saveDraft(project, content);
}

export async function loadDraft(project: string): Promise<string | null> {
  if (!isTauri()) return localStorage.getItem(previewDraftKey(project));
  return commands.loadDraft(project);
}

export async function deleteDraft(project: string): Promise<void> {
  if (!isTauri()) {
    localStorage.removeItem(previewDraftKey(project));
    return;
  }
  await commands.deleteDraft(project);
}

export async function listProjectFiles(root: string): Promise<string[]> {
  if (!isTauri())
    return previewFiles
      .filter((file) => !file.isDir)
      .map((file) => file.path)
      .sort();
  return commands.listProjectFiles(root);
}

export async function readRuntimeSettings(): Promise<RuntimeSettings> {
  if (!isTauri()) return { ...previewRuntimeSettings };
  return commands.readRuntimeSettings();
}

/** What the running `codex app-server` reported about itself at `initialize`. */
export async function readCodexServerInfo(): Promise<CodexServerInfo> {
  if (!isTauri()) {
    return { userAgent: "pingex/0.151.0 (preview; arm64) (pingex; 0.0.0)", platformOs: "preview" };
  }
  return (await (commands.readCodexServerInfo() as Promise<CodexServerInfo | null>)) ?? {};
}

export async function updateRuntimeSettings(
  codexHome: string | null,
  codexBinary: string | null,
): Promise<RuntimeSettings> {
  if (!isTauri()) {
    previewRuntimeSettings.overrideCodexHome = codexHome;
    previewRuntimeSettings.overrideCodexBinary = codexBinary;
    previewRuntimeSettings.restartRequired = Boolean(codexHome || codexBinary);
    return { ...previewRuntimeSettings };
  }
  return commands.updateRuntimeSettings(codexHome, codexBinary);
}

export async function readLaunchState(): Promise<LaunchState> {
  if (!isTauri()) return structuredClone(previewLaunchState);
  return commands.readLaunchState();
}

export async function selectCodexHome(path: string): Promise<LaunchState> {
  if (!isTauri()) {
    previewLaunchState.codexHome = path;
    previewLaunchState.needsPicker = false;
    previewLaunchState.recentHomes = [
      { path, lastUsed: Math.floor(Date.now() / 1000), exists: true },
      ...previewLaunchState.recentHomes.filter((home) => home.path !== path),
    ];
    return structuredClone(previewLaunchState);
  }
  return commands.selectCodexHome(path);
}

/**
 * Open another app window, bound to `path` when given (e.g. a second account's
 * Codex home) or showing the launch picker when not. Returns the new window's
 * label.
 */
export async function openHomeWindow(path?: string): Promise<string | null> {
  if (!isTauri()) return null;
  return commands.openHomeWindow(path ?? null);
}

/** Probe a Codex CLI path without saving it (blank = the active binary). */
export async function checkCodexBinary(path: string | null): Promise<BinaryStatus> {
  if (!isTauri()) {
    const binary = path?.trim() || previewLaunchState.codexBinaryStatus.binary;
    const found = binary === "codex" || binary.endsWith("/codex");
    return {
      binary,
      resolved: found ? (binary.includes("/") ? binary : "/opt/homebrew/bin/codex") : null,
      found,
      message: found ? null : `No executable Codex CLI at ${binary}.`,
    };
  }
  return commands.checkCodexBinary(path);
}

/** Persist and immediately apply a Codex CLI path (null clears the override). */
export async function setCodexBinary(path: string | null): Promise<LaunchState> {
  if (!isTauri()) {
    const status = await checkCodexBinary(path);
    if (!status.found) throw new Error(status.message ?? "Codex CLI not found");
    previewLaunchState.codexBinary = status.binary;
    previewLaunchState.codexBinaryStatus = status;
    return structuredClone(previewLaunchState);
  }
  return commands.setCodexBinary(path);
}

export async function removeRecentHome(path: string): Promise<LaunchState> {
  if (!isTauri()) {
    previewLaunchState.recentHomes = previewLaunchState.recentHomes.filter((home) => home.path !== path);
    return structuredClone(previewLaunchState);
  }
  return commands.removeRecentHome(path);
}

export async function readConfigSettings(): Promise<ConfigSetting[]> {
  if (!isTauri()) return structuredClone(previewConfigSettings);
  return commands.readConfigSettings();
}

/** Set (value) or unset (unset:true, inherit default) a whitelisted config.toml key. */
export async function writeConfigSetting(key: string, value: string | null, unset = false): Promise<ConfigSetting[]> {
  if (!isTauri()) {
    const setting = previewConfigSettings.find((entry) => entry.key === key);
    if (setting) {
      if (unset) {
        setting.value = setting.default;
        setting.source = "default";
      } else {
        setting.value = value;
        setting.source = "config";
      }
    }
    return structuredClone(previewConfigSettings);
  }
  return commands.writeConfigSetting(key, value, unset);
}

export async function readHomeOverview(): Promise<HomeOverview> {
  if (!isTauri()) return structuredClone(previewHomeOverview);
  return commands.readHomeOverview();
}

// --- Native Git service ---
// The frontend never shells out to Git; every call goes through these wrappers,
// which the Rust `git` module backs with the installed `git` executable.

export async function gitRepoInfo(dir: string): Promise<GitRepoInfo> {
  if (!isTauri()) return previewGitRepoInfo(dir);
  return commands.gitRepoInfo(dir);
}

export async function gitStatus(dir: string): Promise<GitStatus> {
  if (!isTauri()) return { ...previewGitStatus, refreshedAt: Date.now() };
  return commands.gitStatus(dir);
}

export async function gitWorktrees(repoDir: string): Promise<WorktreeEntry[]> {
  if (!isTauri()) return structuredClone(previewWorktrees);
  return commands.gitWorktrees(repoDir);
}

export async function gitChangesSummary(dir: string): Promise<ChangesSummary> {
  if (!isTauri()) {
    return {
      base: "HEAD",
      baseBranch: null,
      files: [
        { path: "src/lib/example.ts", oldPath: null, status: "modified", additions: 12, deletions: 3, binary: false },
        { path: "README.md", oldPath: null, status: "added", additions: 40, deletions: 0, binary: false },
      ],
      truncated: false,
      totalFiles: 2,
      additions: 52,
      deletions: 3,
    };
  }
  return commands.gitChangesSummary(dir);
}

export async function gitFileDiff(
  dir: string,
  base: string,
  path: string,
  untracked: boolean,
  maxBytes?: number,
): Promise<FileDiff> {
  if (!isTauri()) {
    const patch = `diff --git a/${path} b/${path}\n--- a/${path}\n+++ b/${path}\n@@ -1,2 +1,3 @@\n line\n-old\n+new\n+added\n`;
    return { path, patch, truncated: false, binary: false, bytes: patch.length };
  }
  return commands.gitFileDiff(dir, base, path, untracked, maxBytes ?? null);
}

export async function gitWorktreeHandoffPreflight(
  worktreePath: string,
  targetDir: string,
): Promise<WorktreeHandoffPreflight> {
  if (!isTauri()) return { branch: "codex/tmp-preview", worktreeDirty: false, targetDirty: false, blocker: null };
  return commands.gitWorktreeHandoffPreflight(worktreePath, targetDir);
}

/** Check the temp worktree's branch out in `targetDir` and remove the worktree. Returns the branch. */
export async function gitWorktreeHandoff(
  worktreePath: string,
  targetDir: string,
  commitUncommitted: boolean,
  branchName?: string | null,
): Promise<string> {
  if (!isTauri()) return branchName || "codex/tmp-preview";
  return commands.gitWorktreeHandoff(worktreePath, targetDir, commitUncommitted, branchName ?? null);
}

export async function gitRecentCommits(dir: string, limit = 20): Promise<GitCommit[]> {
  if (!isTauri()) return structuredClone(previewCommits);
  return commands.gitRecentCommits(dir, limit);
}

export async function gitBranches(dir: string, limit = 200): Promise<GitBranch[]> {
  if (!isTauri()) return structuredClone(previewBranches);
  return commands.gitBranches(dir, limit);
}

export async function gitWorktreeAdd(repoDir: string, path: string, branch: WorktreeBranchRequest): Promise<void> {
  if (!isTauri()) {
    previewWorktrees.push({
      path,
      head: null,
      branch: branch.name,
      detached: false,
      bare: false,
      locked: false,
      lockReason: null,
      prunable: false,
      prunableReason: null,
      isMain: false,
      isCodexManaged: false,
      missingDir: false,
      branchCheckedOutElsewhere: false,
      upstream: null,
      ahead: 0,
      behind: 0,
      status: { staged: 0, unstaged: 0, untracked: 0, conflicted: 0 },
      state: null,
    });
    return;
  }
  await commands.gitWorktreeAdd(repoDir, { path, branch });
}

export async function gitWorktreeRemove(repoDir: string, path: string, force: boolean): Promise<void> {
  if (!isTauri()) {
    const index = previewWorktrees.findIndex((entry) => entry.path === path);
    if (index >= 0) previewWorktrees.splice(index, 1);
    return;
  }
  await commands.gitWorktreeRemove(repoDir, path, force);
}

export async function gitWorktreePrune(repoDir: string): Promise<void> {
  if (!isTauri()) {
    for (let index = previewWorktrees.length - 1; index >= 0; index -= 1) {
      if (previewWorktrees[index].missingDir) previewWorktrees.splice(index, 1);
    }
    return;
  }
  await commands.gitWorktreePrune(repoDir);
}

export async function gitWorktreeLock(repoDir: string, path: string, reason?: string): Promise<void> {
  if (!isTauri()) {
    const entry = previewWorktrees.find((candidate) => candidate.path === path);
    if (entry) {
      entry.locked = true;
      entry.lockReason = reason?.trim() || null;
    }
    return;
  }
  await commands.gitWorktreeLock(repoDir, path, reason ?? null);
}

export async function gitWorktreeUnlock(repoDir: string, path: string): Promise<void> {
  if (!isTauri()) {
    const entry = previewWorktrees.find((candidate) => candidate.path === path);
    if (entry) {
      entry.locked = false;
      entry.lockReason = null;
    }
    return;
  }
  await commands.gitWorktreeUnlock(repoDir, path);
}

// --- CLI / desktop handoff ---
// The reproducible command and shareable link are built in Rust (proper shell
// quoting / URL encoding); the frontend only requests and copies them.

export async function handoffCommand(threadId: string, cwd: string): Promise<string> {
  if (!isTauri()) return `CODEX_HOME='${previewLaunchState.codexHome}' codex resume '${threadId}' --cd '${cwd}'`;
  return commands.handoffCommand(threadId, cwd);
}

export async function handoffThreadLink(threadId: string, cwd: string, label?: string): Promise<string> {
  if (!isTauri()) {
    const params = new URLSearchParams({ path: cwd, codexHome: previewLaunchState.codexHome });
    if (label) params.set("label", label);
    return `codex://threads/${threadId}?${params.toString()}`;
  }
  return commands.handoffThreadLink(threadId, cwd, label ?? null);
}

/** Copy arbitrary text to the system clipboard. */
export const copyText = handoffCopy;

export async function handoffCopy(text: string): Promise<void> {
  if (!isTauri()) {
    await navigator.clipboard?.writeText(text).catch(() => {});
    return;
  }
  await commands.handoffCopy(text);
}

export async function handoffLaunchTerminal(command: string): Promise<void> {
  if (!isTauri()) return;
  await commands.handoffLaunchTerminal(command);
}

export async function remotePairingStart(): Promise<PairingInfo> {
  if (!isTauri()) {
    return {
      qrSvg:
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10" width="220" height="220"><rect width="10" height="10" fill="#fff"/><path d="M1 1h3v3H1zM6 1h3v3H6zM1 6h3v3H1zM6 6h1v1H6zM8 6h1v1H8zM6 8h1v1H6zM8 8h1v1H8z" fill="#000"/></svg>',
      pairingCode: "preview-pairing-code",
      manualPairingCode: "PREVIEW-1234",
      expiresAt: Date.now() / 1000 + 600,
    };
  }
  return commands.remotePairingStart() as Promise<PairingInfo>;
}

export async function remotePairingStatus(pairingCode: string): Promise<{ claimed: boolean }> {
  if (!isTauri()) return { claimed: false };
  return commands.remotePairingStatus(pairingCode) as Promise<{ claimed: boolean }>;
}

// --- Pull-request review (provider-neutral; GitHub adapter via `gh`) ---
// The frontend never shells out to `gh`; every call goes through these wrappers,
// which the Rust `review` module backs. Browser mode serves preview fixtures so
// the whole three-pane view renders without Tauri.

const previewReviewDrafts = new Map<string, ReviewDraft>();

export async function reviewProviderStatus(repoDir: string): Promise<ProviderStatus> {
  if (!isTauri()) return { ...previewProviderStatus };
  return commands.reviewProviderStatus(repoDir);
}

export async function reviewListPrs(repoDir: string): Promise<PrSummary[]> {
  if (!isTauri()) return structuredClone(previewPrs);
  return commands.reviewListPrs(repoDir);
}

export async function reviewPrDetail(repoDir: string, number: number): Promise<PrDetail> {
  if (!isTauri()) {
    const detail = structuredClone(previewPrDetail);
    const match = previewPrs.find((pr) => pr.number === number);
    if (match) detail.summary = structuredClone(match);
    return detail;
  }
  return commands.reviewPrDetail(repoDir, number);
}

export async function reviewCheckFresh(
  repoDir: string,
  number: number,
  knownHead: string,
  knownUpdatedAt: string,
): Promise<PrFreshness> {
  if (!isTauri()) return { stale: false, remoteHead: knownHead, remoteUpdatedAt: knownUpdatedAt };
  return commands.reviewCheckFresh(repoDir, number, knownHead, knownUpdatedAt);
}

export async function reviewLocalDiff(repoDir: string, base: string, head?: string): Promise<PrFile[]> {
  if (!isTauri()) return structuredClone(previewPrDetail.files);
  return commands.reviewLocalDiff(repoDir, base, head ?? null);
}

export async function reviewSubmit(
  repoDir: string,
  number: number,
  event: string,
  body: string,
  comments: PendingComment[],
): Promise<void> {
  if (!isTauri()) return;
  await commands.reviewSubmit(repoDir, number, event, body, comments);
}

export async function reviewReply(repoDir: string, number: number, commentId: number, body: string): Promise<void> {
  if (!isTauri()) return;
  await commands.reviewReply(repoDir, number, commentId, body);
}

export async function reviewResolveThread(repoDir: string, threadId: string): Promise<void> {
  if (!isTauri()) return;
  await commands.reviewResolveThread(repoDir, threadId);
}

const reviewDraftKey = (provider: string, repo: string, prNumber: number) => `${provider}:${repo}:${prNumber}`;

export async function reviewSaveDraft(
  provider: string,
  repo: string,
  prNumber: number,
  headSha: string,
  payload: string,
): Promise<void> {
  if (!isTauri()) {
    previewReviewDrafts.set(reviewDraftKey(provider, repo, prNumber), {
      headSha,
      payload,
      updatedAt: Math.floor(Date.now() / 1000),
    });
    return;
  }
  await commands.reviewSaveDraft(provider, repo, prNumber, headSha, payload);
}

export async function reviewLoadDraft(provider: string, repo: string, prNumber: number): Promise<ReviewDraft | null> {
  if (!isTauri()) return previewReviewDrafts.get(reviewDraftKey(provider, repo, prNumber)) ?? null;
  return commands.reviewLoadDraft(provider, repo, prNumber);
}

export async function reviewDeleteDraft(provider: string, repo: string, prNumber: number): Promise<void> {
  if (!isTauri()) {
    previewReviewDrafts.delete(reviewDraftKey(provider, repo, prNumber));
    return;
  }
  await commands.reviewDeleteDraft(provider, repo, prNumber);
}

// --- Remote connections management ---

export async function listConnections(): Promise<RemoteConnection[]> {
  if (!isTauri()) return previewConnections.map((connection) => ({ ...connection }));
  return commands.listConnections();
}

export async function refreshConnections(): Promise<RemoteConnection[]> {
  if (!isTauri()) return previewConnections.map((connection) => ({ ...connection }));
  return commands.refreshConnections();
}

export async function renameConnection(clientId: string, name: string): Promise<void> {
  if (!isTauri()) {
    const connection = previewConnections.find((entry) => entry.clientId === clientId);
    if (connection && name.trim()) connection.name = name.trim();
    return;
  }
  await commands.renameConnection(clientId, name);
}

/** Safe: forgets the local record without revoking the credential. */
export async function disconnectConnection(clientId: string): Promise<void> {
  if (!isTauri()) {
    const index = previewConnections.findIndex((entry) => entry.clientId === clientId);
    if (index >= 0) previewConnections.splice(index, 1);
    return;
  }
  await commands.disconnectConnection(clientId);
}

/** Destructive: revokes the credential on the relay and drops the local record. */
export async function revokeConnection(clientId: string): Promise<void> {
  if (!isTauri()) {
    const index = previewConnections.findIndex((entry) => entry.clientId === clientId);
    if (index >= 0) previewConnections.splice(index, 1);
    return;
  }
  await commands.revokeConnection(clientId);
}

// --- Quick chat and global hotkeys ---------------------------------------
// The default accelerator (documented in settings.rs) that toggles the quick
// composer when nothing is persisted yet.
export const DEFAULT_QUICK_SHORTCUT = "CmdOrCtrl+Shift+Space";

/** The global shortcut currently bound to the quick-chat window. */
export async function getQuickShortcut(): Promise<string> {
  if (!isTauri()) return previewQuickShortcut.value;
  return commands.getQuickShortcut();
}

/**
 * Persist and re-register the quick-chat shortcut. Rejects (with the OS
 * conflict message) when the accelerator is invalid or already taken.
 */
export async function setQuickShortcut(accelerator: string): Promise<string> {
  if (!isTauri()) {
    previewQuickShortcut.value = accelerator.trim() || DEFAULT_QUICK_SHORTCUT;
    return previewQuickShortcut.value;
  }
  return commands.setQuickShortcut(accelerator);
}

/** Hand a quick-window thread back to the main window and navigate to it. */
export async function quickOpenFullThread(threadId: string): Promise<void> {
  if (!isTauri()) return;
  await commands.quickOpenFullThread(threadId);
}

// --- App-owned subagents ---

/**
 * Every agent a thread has spawned, oldest first. Read from the database so
 * runs from earlier app launches are included; live changes arrive separately
 * on the `codex:agentRun` event.
 */
export async function listAgentRuns(threadId: string): Promise<AgentRun[]> {
  if (!isTauri()) {
    return previewAgentRuns.filter((run) => run.parentThreadId === threadId);
  }
  return commands.listAgentRuns(threadId);
}

/** Stop a running agent and kill its process. */
export async function killAgentRun(runId: string): Promise<void> {
  if (!isTauri()) return;
  await commands.killAgentRun(runId);
}

/** The agent's own thread, so the user can open its transcript. */
export async function openAgentThread(runId: string): Promise<string | null> {
  if (!isTauri()) {
    return previewAgentRuns.find((run) => run.runId === runId)?.childThreadId ?? null;
  }
  return commands.openAgentThread(runId);
}

export async function readAgentSettings(): Promise<AgentSettings> {
  if (!isTauri()) return structuredClone(previewAgentSettings);
  return commands.readAgentSettings() as Promise<AgentSettings>;
}

export async function writeAgentSettings(settings: AgentSettings): Promise<AgentSettings> {
  if (!isTauri()) {
    Object.assign(previewAgentSettings, settings);
    return structuredClone(previewAgentSettings);
  }
  return commands.writeAgentSettings(settings) as Promise<AgentSettings>;
}

// --- Integrations (MCP servers, skills, plugins) ---

/**
 * Config-declared servers plus Codex's skill list. `cwds` scopes the skill
 * lookup — pass the active project directory to pick up project skills.
 */
export async function listIntegrations(cwds: string[] = []): Promise<IntegrationsList> {
  if (!isTauri()) return structuredClone(previewIntegrations);
  return commands.listIntegrations(cwds);
}

/**
 * Live per-server state from Codex: startup result, the tools each server
 * actually exposes, and auth status. Keyed by server name so callers can join
 * it onto the config-declared list.
 */
export async function listMcpServerStatus(): Promise<Record<string, McpServerStatus>> {
  const response = isTauri()
    ? await (commands.listMcpServerStatus() as Promise<{ data?: McpServerStatus[] }>)
    : previewMcpServerStatus();
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
  if (!isTauri()) {
    previewMcpOauthLogin(name);
    return;
  }
  await commands.mcpOauthLogin(name);
}

/** Make a running Codex re-read `config.toml` without a restart. */
export async function reloadMcpServers(): Promise<void> {
  if (!isTauri()) return;
  await commands.reloadMcpServers();
}

/** Skills Codex would offer for these directories, for the `$` picker. */
export async function listSkillsFor(cwds: string[]): Promise<SkillSummary[]> {
  if (!isTauri()) return structuredClone(previewIntegrations.skills);
  const response = await (commands.listSkillsFor(cwds) as Promise<{ data?: { skills?: SkillSummary[] }[] }>);
  const byName = new Map<string, SkillSummary>();
  for (const group of response.data ?? []) {
    for (const skill of group.skills ?? []) {
      if (!byName.has(skill.name)) byName.set(skill.name, skill);
    }
  }
  return [...byName.values()].sort((a, b) => a.name.localeCompare(b.name));
}

export async function setSkillEnabled(name: string, enabled: boolean): Promise<void> {
  if (!isTauri()) {
    const skill = previewIntegrations.skills.find((entry) => entry.name === name);
    if (skill) skill.enabled = enabled;
    return;
  }
  await commands.setSkillEnabled(name, enabled);
}

/** Raw `SKILL.md` text for a skill; `path` is the directory or the file. */
export async function readSkill(path: string): Promise<string> {
  if (!isTauri()) return previewReadSkill(path);
  return commands.readSkill(path);
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
  if (!isTauri()) return previewCreateSkill(input);
  return commands.createSkill(input.name, input.description, input.body ?? null, null);
}

/** Delete a user-scope skill directory. Refused for anything outside `~/.codex/skills`. */
export async function deleteSkill(path: string): Promise<IntegrationsList> {
  if (!isTauri()) return previewDeleteSkill(path);
  return commands.deleteSkill(path, null);
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
  const args = input.args ?? [];
  const env = input.env ?? {};
  if (!isTauri()) {
    previewSaveMcpServer({ ...input, args, envKeys: input.envKeys ?? Object.keys(env) });
    return structuredClone(previewIntegrations);
  }
  return commands.saveMcpServer({
    previousName: input.previousName ?? null,
    name: input.name,
    command: input.command ?? null,
    args,
    env,
    envKeys: input.envKeys ?? null,
    url: input.url ?? null,
    bearerTokenEnvVar: input.bearerTokenEnvVar ?? null,
  });
}

export async function removeMcpServer(name: string): Promise<IntegrationsList> {
  if (!isTauri()) {
    previewIntegrations.mcpServers = previewIntegrations.mcpServers.filter((server) => server.name !== name);
    return structuredClone(previewIntegrations);
  }
  return commands.removeMcpServer(name);
}

export async function setMcpEnabled(name: string, enabled: boolean): Promise<IntegrationsList> {
  if (!isTauri()) {
    const server = previewIntegrations.mcpServers.find((entry) => entry.name === name);
    if (server) server.enabled = enabled;
    return structuredClone(previewIntegrations);
  }
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
  if (!isTauri()) return previewSearchThreads(query, cursor, filter, generation);
  return commands.searchThreads(query, cursor, filter, generation);
}

/** Page through the app-server thread listing, forwarding its opaque cursor. */
export async function listThreadsPage(
  cursor: string | null,
  pageSize: number,
  archived: boolean,
  projectPath: string | null,
): Promise<ThreadsPage> {
  if (!isTauri()) return previewThreadsPage(cursor, pageSize, archived, projectPath);
  return commands.listThreadsPage(cursor, pageSize, archived, projectPath);
}

// --- Message log (Advanced settings) ---

/**
 * Start or stop recording the JSON-RPC traffic with the app-server. Recording
 * is off by default; stopping also discards whatever was captured.
 */
export async function setWireLogging(enabled: boolean): Promise<void> {
  if (!isTauri()) return;
  await commands.setWireLogging(enabled);
}

/** The buffered messages, oldest first. */
export async function readWireLog(): Promise<WireMessage[]> {
  if (!isTauri()) return [...previewWireLog];
  return commands.readWireLog();
}

export async function clearWireLog(): Promise<void> {
  if (!isTauri()) return;
  await commands.clearWireLog();
}
