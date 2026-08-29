import { eventMatchesHome } from "$lib/app/launch.svelte";
import { events, type HarnessRequestEnvelope } from "$lib/bindings";
import { applyRateLimitUpdate } from "$lib/services/accountUsage.svelte";
import { applyAgentActivity, applyAgentRunEvent } from "$lib/services/agentRuns.svelte";
import { recordUserInputRequest } from "$lib/services/api";
import { applyProcessEvent } from "$lib/services/processes.svelte";
import { isTauri } from "$lib/services/tauri";
import { reviewTransition, threadIdOf, turnEnd } from "$lib/services/turnLifecycle";
import type {
  CodexEvent,
  CodexServerRequestEvent,
  FileUpdateChange,
  McpElicitationSchema,
  RequestPermissionProfile,
  ThreadTokenUsage,
  TurnPlan,
  UserInputQuestion,
} from "$lib/types";

export type { CodexEvent, CodexServerRequestEvent, UserInputQuestion } from "$lib/types";

export interface Approval {
  requestId: number;
  kind: "command" | "fileChange" | "permissions";
  /** On a `command` approval: run a command, or send input to one already
   *  running (Codex ≥0.150; older builds only ever ask to run). */
  approvalKind?: "command" | "writeStdin";
  /** The approval's own id on Codex ≥0.150; informational, the reply is
   *  keyed by `requestId`. */
  approvalId?: string | null;
  threadId: string;
  turnId: string;
  itemId: string;
  command?: string | null;
  cwd?: string | null;
  reason?: string | null;
  changes?: FileUpdateChange[] | null;
  /** What Codex is asking to be allowed, on a `permissions` approval. */
  permissions?: RequestPermissionProfile | null;
  /** Set on a request from another harness; the card then draws `options`
   *  instead of the Codex decision buttons. */
  harness?: "claude";
  options?: { optionId: string; name: string; kind: string }[];
  title?: string;
  description?: string | null;
  /** Focus Decline rather than Allow (the harness flagged the request). */
  defaultToReject?: boolean;
}

/**
 * An MCP server asking the user for input mid-tool-call. Three shapes: `form`
 * carries a schema this app can draw fields from, `openai/form` carries an
 * opaque schema it cannot, and `url` sends the user out to a web page. All
 * three are answered the same way — accept, decline or cancel.
 */
export interface Elicitation {
  requestId: number;
  threadId: string;
  turnId: string | null;
  serverName: string;
  mode: string;
  message: string;
  requestedSchema?: McpElicitationSchema | null;
  url?: string | null;
  /** Opaque `_meta` from the request (e.g. a `suggestion_id` on newer
   *  Codex builds), echoed back on the response so the server can correlate. */
  meta?: unknown;
}

export interface UserInputRequest {
  /** Null for a question whose request died with an earlier session. */
  requestId: number | null;
  threadId: string;
  turnId: string;
  itemId: string;
  questions: UserInputQuestion[];
}

export const approvals = $state<{ list: Approval[] }>({ list: [] });
export const userInputRequests = $state<{ list: UserInputRequest[] }>({ list: [] });
export const elicitations = $state<{ list: Elicitation[] }>({ list: [] });

/** Thread ids with a turn currently in progress, across all threads (not just the open one). */
export const activeTurns = $state<{ list: string[] }>({ list: [] });

/**
 * Bumped whenever Codex reports MCP state changed — a server finished starting,
 * or an OAuth login completed. Views watch `nonce` and refetch;
 * `lastLoginServer` lets the one that started a sign-in clear its pending state.
 */
export const mcpStatus = $state<{ nonce: number; lastLoginServer: string | null }>({
  nonce: 0,
  lastLoginServer: null,
});

/** Bumped on `skills/changed`: the skill set on disk moved under Codex, so
 *  pickers re-run `skills/list` rather than serving a stale list. */
export const skillsStatus = $state<{ nonce: number }>({ nonce: 0 });

/** Threads whose turn is paused while Codex re-authenticates with the model
 *  provider (unstable Codex). Set on `authRecoveryStarted`, gone on
 *  `authRecoveryCompleted` or whatever else ends the turn. */
export const authRecovery = $state<{ byThread: Record<string, { provider: string | null; message: string | null }> }>({
  byThread: {},
});

/**
 * Thread ids holding a question that was never answered. Unlike the stores
 * above this one is persisted, so it survives an app restart and is the only
 * way to find a thread Codex stranded mid-question.
 */
export const unansweredQuestions = $state<{ list: string[] }>({ list: [] });

export function markUnanswered(threadId: string) {
  if (!unansweredQuestions.list.includes(threadId)) unansweredQuestions.list.push(threadId);
}

export function clearUnanswered(threadId: string) {
  unansweredQuestions.list = unansweredQuestions.list.filter((id) => id !== threadId);
}

/** Seeds the persisted set at startup. */
export function setUnansweredQuestions(threadIds: string[]) {
  unansweredQuestions.list = [...threadIds];
}

/**
 * Seeds the set from the backend at startup. The backend outlives a webview
 * reload, and a turn that started before the reload never re-announces itself,
 * so this is the only way the new webview learns it is still running. Merged
 * rather than replaced: a `turn/started` can land before the seed arrives.
 */
export function seedActiveTurns(threadIds: string[]) {
  for (const threadId of threadIds) setTurnActive(threadId, true);
}

function setTurnActive(threadId: string | undefined, active: boolean) {
  if (!threadId) return;
  if (active) {
    if (!activeTurns.list.includes(threadId)) activeTurns.list.push(threadId);
  } else {
    activeTurns.list = activeTurns.list.filter((id) => id !== threadId);
  }
}

/**
 * Latest context usage per thread. Codex only replays `thread/tokenUsage/updated`
 * the first time a thread resumes in a session, so re-opening one later has to
 * fall back to what we already saw.
 */
export const threadTokenUsage: Record<string, ThreadTokenUsage> = {};

/**
 * The todo list Codex is working through, per thread. It belongs to a single
 * turn — Codex rebuilds it from scratch each time — so it is dropped when that
 * turn ends rather than lingering over the next one.
 */
export const turnPlans = $state<{ byThread: Record<string, TurnPlan> }>({ byThread: {} });

const threadHandlers = new Set<(event: CodexEvent) => void>();

/**
 * Register a thread event handler (the open ThreadView, a side-question
 * panel, ...). Handlers filter by threadId themselves. Returns an
 * unregister fn.
 */
export function setThreadHandler(handler: (event: CodexEvent) => void): () => void {
  threadHandlers.add(handler);
  return () => {
    threadHandlers.delete(handler);
  };
}

export function removeApproval(requestId: number) {
  approvals.list = approvals.list.filter((approval) => approval.requestId !== requestId);
}

export function removeUserInputRequest(requestId: number) {
  userInputRequests.list = userInputRequests.list.filter((request) => request.requestId !== requestId);
}

export function removeElicitation(requestId: number) {
  elicitations.list = elicitations.list.filter((elicitation) => elicitation.requestId !== requestId);
}

/**
 * Threads currently inside a review. Mid-review Codex announces a bookkeeping
 * `turn/started` the transcript deliberately ignores; this set lets the active
 * flag ignore it too. The real review turn's `turn/started` arrives before
 * `enteredReviewMode` does, so it still activates normally.
 */
const reviewThreads = new Set<string>();

function dispatch(event: CodexEvent) {
  // A subagent's notifications arrive here under its own thread id, so every
  // mounted view filters them out. Before that happens, fold them into the
  // agent's activity line — otherwise the only sign of a working agent in the
  // thread that spawned it is a status word that does not move for minutes.
  applyAgentActivity(event);
  applyProcessEvent(event);
  const threadId = threadIdOf(event);
  if (threadId && reviewTransition(event) === "entered") reviewThreads.add(threadId);
  if (event.method === "turn/started" && threadId && !reviewThreads.has(threadId)) {
    setTurnActive(threadId, true);
  }
  // Whatever ends the turn — including a review leaving review mode, and
  // excluding an error Codex will retry — stops the thread showing as working.
  const end = turnEnd(event);
  if (end) {
    setTurnActive(end.threadId, false);
    // A review that dies on an error never reaches `exitedReviewMode`.
    reviewThreads.delete(end.threadId);
    delete authRecovery.byThread[end.threadId];
  }
  switch (event.method) {
    case "disconnected":
      activeTurns.list = [];
      turnPlans.byThread = {};
      authRecovery.byThread = {};
      reviewThreads.clear();
      break;
    // Codex dropped the thread from memory (another client closed it, or it
    // was evicted): nothing can still be running in it.
    case "thread/closed":
      setTurnActive(event.params.threadId, false);
      delete turnPlans.byThread[event.params.threadId];
      delete authRecovery.byThread[event.params.threadId];
      break;
    case "modelProvider/authRecoveryStarted":
      authRecovery.byThread[event.params.threadId] = {
        provider: event.params.provider ?? null,
        message: event.params.message ?? null,
      };
      break;
    case "modelProvider/authRecoveryCompleted":
      delete authRecovery.byThread[event.params.threadId];
      break;
    case "skills/changed":
      skillsStatus.nonce += 1;
      break;
    case "turn/plan/updated":
      turnPlans.byThread[event.params.threadId] = {
        turnId: event.params.turnId,
        explanation: event.params.explanation ?? null,
        steps: event.params.plan ?? [],
      };
      break;
    // The plan belongs to the turn that built it. Keyed by turn id so a stale
    // notification arriving after the next turn started cannot wipe its plan.
    case "turn/completed": {
      const plan = turnPlans.byThread[event.params.threadId];
      if (plan && (!event.params.turn?.id || plan.turnId === event.params.turn.id)) {
        delete turnPlans.byThread[event.params.threadId];
      }
      break;
    }
    // Codex resolved a request behind our back — it timed out, another client
    // answered it, or the turn was interrupted. Without this the card sits there
    // forever waiting for an answer nobody is listening for any more.
    case "serverRequest/resolved":
      if (typeof event.params.requestId === "number") {
        removeApproval(event.params.requestId);
        removeUserInputRequest(event.params.requestId);
        removeElicitation(event.params.requestId);
      }
      break;
    case "thread/tokenUsage/updated":
      if (event.params.tokenUsage) threadTokenUsage[event.params.threadId] = event.params.tokenUsage;
      break;
    // Rolling rate-limit updates are sparse; the store merges them into the last
    // full snapshot rather than replacing it.
    case "account/rateLimits/updated":
      if (event.params.rateLimits) applyRateLimitUpdate(event.params.rateLimits);
      break;
    // MCP servers start asynchronously and OAuth completes out of band (in the
    // user's browser), so the Integrations view has to be told to re-read rather
    // than polling. Both notifications just bump a nonce; whoever is watching
    // decides whether a refetch is worth it.
    case "mcpServer/startupStatus/updated":
      mcpStatus.nonce += 1;
      break;
    case "mcpServer/oauthLogin/completed":
      mcpStatus.nonce += 1;
      mcpStatus.lastLoginServer = event.params.name ?? event.params.serverName ?? null;
      break;
  }
  for (const handler of [...threadHandlers]) handler(event);
}

function onServerRequest(payload: CodexServerRequestEvent) {
  const { requestId } = payload;
  switch (payload.method) {
    case "item/commandExecution/requestApproval": {
      const params = payload.params;
      approvals.list.push({
        requestId,
        kind: "command",
        // Absent before 0.150, when every approval was for running a command.
        approvalKind: params.kind === "writeStdin" ? "writeStdin" : "command",
        approvalId: params.approvalId ?? null,
        threadId: params.threadId ?? "",
        turnId: params.turnId ?? "",
        itemId: params.itemId ?? "",
        command: params.command,
        cwd: params.cwd,
        reason: params.reason,
      });
      break;
    }
    case "item/fileChange/requestApproval": {
      const params = payload.params;
      approvals.list.push({
        requestId,
        kind: "fileChange",
        threadId: params.threadId ?? "",
        turnId: params.turnId ?? "",
        itemId: params.itemId ?? "",
        reason: params.reason,
        changes: params.changes,
      });
      break;
    }
    case "item/permissions/requestApproval": {
      const params = payload.params;
      approvals.list.push({
        requestId,
        kind: "permissions",
        threadId: params.threadId ?? "",
        turnId: params.turnId ?? "",
        itemId: params.itemId ?? "",
        cwd: params.cwd,
        reason: params.reason,
        permissions: params.permissions ?? {},
      });
      break;
    }
    case "mcpServer/elicitation/request": {
      const params = payload.params;
      elicitations.list.push({
        requestId,
        threadId: params.threadId ?? "",
        turnId: params.turnId ?? null,
        serverName: params.serverName ?? "",
        mode: params.mode ?? "form",
        message: params.message ?? "",
        requestedSchema: params.requestedSchema,
        url: params.url,
        meta: params._meta ?? null,
      });
      break;
    }
    case "item/tool/requestUserInput": {
      const params = payload.params;
      const request = {
        requestId,
        threadId: params.threadId ?? "",
        turnId: params.turnId ?? "",
        itemId: params.itemId ?? "",
        questions: params.questions ?? [],
      };
      userInputRequests.list.push(request);
      // The request itself dies with the app-server, so persist the question now:
      // it is the only copy that survives the app exiting unanswered. The
      // backend stamps afterItemId with the id of the item that preceded this
      // question in the real stream order, so it can be spliced back into the
      // right spot (rather than appended to the end) after a restart.
      if (request.threadId && request.turnId && request.itemId) {
        markUnanswered(request.threadId);
        void recordUserInputRequest({
          threadId: request.threadId,
          turnId: request.turnId,
          itemId: request.itemId,
          afterItemId: params.afterItemId ?? undefined,
          item: {
            type: "userInputAnswered",
            id: request.itemId,
            questions: request.questions,
            answers: {},
            unanswered: true,
          },
        }).catch(() => {});
      }
      break;
    }
    default:
      // The app's own `pingex_*` agent tools never arrive here, and neither do
      // the ones Rust answers by itself (`currentTime/read`, ...): a response
      // held by the webview would not survive a reload. Anything still reaching
      // this branch is a method Codex has added since — and because an
      // unanswered request stalls its turn, it is worth saying so out loud
      // rather than letting the thread quietly hang.
      console.warn(`Unhandled Codex server request: ${payload.params.method}`, payload.params.params);
  }
}

/**
 * A request from a non-Codex harness. Permissions become approvals on the
 * card their tool kind suggests; questions go where Codex's questions go,
 * persisted the same way so an unanswered one survives a reload.
 */
export function onHarnessRequest(payload: HarnessRequestEnvelope) {
  const { requestId, threadId, turnId, itemId, request } = payload;
  if (request.type === "user_input") {
    const questions = (request.questions ?? []) as UserInputQuestion[];
    userInputRequests.list.push({ requestId, threadId, turnId, itemId, questions });
    markUnanswered(threadId);
    void recordUserInputRequest({
      threadId,
      turnId,
      itemId,
      item: { type: "userInputAnswered", id: itemId, questions, answers: {}, unanswered: true },
    }).catch(() => {});
    return;
  }
  const kind: Approval["kind"] =
    request.kind === "execute" && request.command != null
      ? "command"
      : request.kind === "edit" && Array.isArray(request.changes) && request.changes.length > 0
        ? "fileChange"
        : "permissions";
  approvals.list.push({
    requestId,
    kind,
    harness: "claude",
    threadId,
    turnId,
    itemId,
    command: request.command ?? null,
    cwd: request.cwd ?? null,
    reason: request.reason ?? null,
    changes: kind === "fileChange" ? (request.changes as FileUpdateChange[]) : null,
    permissions: null,
    options: request.options,
    title: request.title,
    description: request.description ?? null,
    defaultToReject: request.defaultToReject,
  });
}

let started = false;

export async function startCodexListeners(): Promise<void> {
  if (started || !isTauri()) return;
  started = true;
  // Backend events are broadcast to every window and tagged with the home
  // they belong to; drop what is meant for a window on another account.
  //
  // The generated payload leaves structured fields (`item`, `turn`, ...) as
  // `unknown`; this is the one place they are asserted to be the hand-written
  // shapes in `$lib/types` the reducers read.
  await events.codexEvent.listen(({ payload }) => {
    if (!eventMatchesHome(payload.codexHome)) return;
    dispatch(payload as CodexEvent);
  });
  await events.codexServerRequest.listen(({ payload }) => {
    if (!eventMatchesHome(payload.codexHome)) return;
    onServerRequest(payload as CodexServerRequestEvent);
  });
  await events.harnessRequest.listen(({ payload }) => {
    if (!eventMatchesHome(payload.codexHome)) return;
    onHarnessRequest(payload);
  });
  await events.codexAgentRun.listen(({ payload }) => {
    if (!eventMatchesHome(payload.codexHome)) return;
    applyAgentRunEvent(payload);
  });
  await events.codexDisconnected.listen(({ payload }) => {
    if (!eventMatchesHome(payload?.codexHome)) return;
    approvals.list = [];
    userInputRequests.list = [];
    elicitations.list = [];
    dispatch({ method: "disconnected", params: null });
  });
}

/** Drives the same dispatch path from browser-preview fakes. */
export function previewEmit(event: CodexEvent): void {
  dispatch(event);
}

export function previewEmitServerRequest(payload: CodexServerRequestEvent): void {
  onServerRequest(payload);
}
