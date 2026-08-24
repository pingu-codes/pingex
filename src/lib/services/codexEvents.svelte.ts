import { listen } from "@tauri-apps/api/event";
import { eventMatchesHome } from "$lib/app/launch.svelte";
import { applyRateLimitUpdate } from "$lib/services/accountUsage.svelte";
import { type AgentRunEvent, applyAgentActivity, applyAgentRunEvent } from "$lib/services/agentRuns.svelte";
import { recordUserInputRequest } from "$lib/services/api";
import { applyProcessEvent } from "$lib/services/processes.svelte";
import { isTauri } from "$lib/services/tauri";
import type {
  FileUpdateChange,
  McpElicitationSchema,
  RequestPermissionProfile,
  ThreadTokenUsage,
  TurnPlan,
} from "$lib/types";

export interface CodexEvent {
  method: string;
  params: any;
}

export interface Approval {
  requestId: number;
  kind: "command" | "fileChange" | "permissions";
  threadId: string;
  turnId: string;
  itemId: string;
  command?: string;
  cwd?: string;
  reason?: string;
  changes?: FileUpdateChange[];
  /** What Codex is asking to be allowed, on a `permissions` approval. */
  permissions?: RequestPermissionProfile;
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
  requestedSchema?: McpElicitationSchema;
  url?: string;
  /** Opaque `_meta` from the request (e.g. a `suggestion_id` on newer
   *  Codex builds), echoed back on the response so the server can correlate. */
  meta?: unknown;
}

export interface UserInputQuestion {
  id: string;
  header: string;
  question: string;
  isOther?: boolean;
  isSecret?: boolean;
  options?: { label: string; description?: string }[] | null;
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
  if (
    (event.method === "item/started" || event.method === "item/completed") &&
    event.params?.item?.type === "enteredReviewMode" &&
    event.params?.threadId
  ) {
    reviewThreads.add(event.params.threadId);
  }
  if (event.method === "turn/started" && !reviewThreads.has(event.params?.threadId)) {
    setTurnActive(event.params?.threadId, true);
  }
  // An error Codex says it will retry leaves the turn running, so the thread
  // must stay marked active or the sidebar stops showing it as working.
  if (event.method === "turn/completed" || (event.method === "error" && !event.params?.willRetry)) {
    setTurnActive(event.params?.threadId, false);
    // A review that dies on an error never reaches `exitedReviewMode`.
    reviewThreads.delete(event.params?.threadId);
  }
  // A review never sends `turn/completed`, so leaving review mode is the only
  // notice that the thread has stopped working. Without this the sidebar keeps
  // showing it as busy, and re-opening it never repairs the stale turn.
  if (event.method === "item/completed" && event.params?.item?.type === "exitedReviewMode") {
    setTurnActive(event.params?.threadId, false);
    reviewThreads.delete(event.params?.threadId);
  }
  if (event.method === "disconnected") {
    activeTurns.list = [];
    turnPlans.byThread = {};
    reviewThreads.clear();
  }
  if (event.method === "turn/plan/updated" && event.params?.threadId) {
    turnPlans.byThread[event.params.threadId] = {
      turnId: event.params.turnId,
      explanation: event.params.explanation ?? null,
      steps: event.params.plan ?? [],
    };
  }
  // The plan belongs to the turn that built it. Keyed by turn id so a stale
  // notification arriving after the next turn started cannot wipe its plan.
  if (event.method === "turn/completed" && event.params?.threadId) {
    const plan = turnPlans.byThread[event.params.threadId];
    if (plan && (!event.params.turn?.id || plan.turnId === event.params.turn.id)) {
      delete turnPlans.byThread[event.params.threadId];
    }
  }
  // Codex resolved a request behind our back — it timed out, another client
  // answered it, or the turn was interrupted. Without this the card sits there
  // forever waiting for an answer nobody is listening for any more.
  if (event.method === "serverRequest/resolved" && typeof event.params?.requestId === "number") {
    removeApproval(event.params.requestId);
    removeUserInputRequest(event.params.requestId);
    removeElicitation(event.params.requestId);
  }
  if (event.method === "thread/tokenUsage/updated" && event.params?.threadId && event.params.tokenUsage) {
    threadTokenUsage[event.params.threadId] = event.params.tokenUsage;
  }
  // Rolling rate-limit updates are sparse; the store merges them into the last
  // full snapshot rather than replacing it.
  if (event.method === "account/rateLimits/updated" && event.params?.rateLimits) {
    applyRateLimitUpdate(event.params.rateLimits);
  }
  // MCP servers start asynchronously and OAuth completes out of band (in the
  // user's browser), so the Integrations view has to be told to re-read rather
  // than polling. Both notifications just bump a nonce; whoever is watching
  // decides whether a refetch is worth it.
  if (event.method === "mcpServer/startupStatus/updated" || event.method === "mcpServer/oauthLogin/completed") {
    mcpStatus.nonce += 1;
    if (event.method === "mcpServer/oauthLogin/completed") {
      mcpStatus.lastLoginServer = event.params?.name ?? event.params?.serverName ?? null;
    }
  }
  for (const handler of [...threadHandlers]) handler(event);
}

function onServerRequest(payload: { requestId: number; method: string; params: any }) {
  const { requestId, method, params } = payload;
  if (method === "item/commandExecution/requestApproval") {
    approvals.list.push({
      requestId,
      kind: "command",
      threadId: params?.threadId ?? "",
      turnId: params?.turnId ?? "",
      itemId: params?.itemId ?? "",
      command: params?.command,
      cwd: params?.cwd,
      reason: params?.reason,
    });
  } else if (method === "item/fileChange/requestApproval") {
    approvals.list.push({
      requestId,
      kind: "fileChange",
      threadId: params?.threadId ?? "",
      turnId: params?.turnId ?? "",
      itemId: params?.itemId ?? "",
      reason: params?.reason,
      changes: params?.changes,
    });
  } else if (method === "item/permissions/requestApproval") {
    approvals.list.push({
      requestId,
      kind: "permissions",
      threadId: params?.threadId ?? "",
      turnId: params?.turnId ?? "",
      itemId: params?.itemId ?? "",
      cwd: params?.cwd,
      reason: params?.reason,
      permissions: params?.permissions ?? {},
    });
  } else if (method === "mcpServer/elicitation/request") {
    elicitations.list.push({
      requestId,
      threadId: params?.threadId ?? "",
      turnId: params?.turnId ?? null,
      serverName: params?.serverName ?? "",
      mode: params?.mode ?? "form",
      message: params?.message ?? "",
      requestedSchema: params?.requestedSchema,
      url: params?.url,
      meta: params?._meta ?? null,
    });
  } else if (method === "item/tool/requestUserInput") {
    const request = {
      requestId,
      threadId: params?.threadId ?? "",
      turnId: params?.turnId ?? "",
      itemId: params?.itemId ?? "",
      questions: params?.questions ?? [],
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
        afterItemId: typeof params?.afterItemId === "string" ? params.afterItemId : undefined,
        item: {
          type: "userInputAnswered",
          id: request.itemId,
          questions: request.questions,
          answers: {},
          unanswered: true,
        },
      }).catch(() => {});
    }
  } else {
    // The app's own `pingex_*` agent tools never arrive here, and neither do
    // the ones Rust answers by itself (`currentTime/read`, ...): a response
    // held by the webview would not survive a reload. Anything still reaching
    // this branch is a method Codex has added since — and because an
    // unanswered request stalls its turn, it is worth saying so out loud
    // rather than letting the thread quietly hang.
    console.warn(`Unhandled Codex server request: ${method}`, params);
  }
}

let started = false;

export async function startCodexListeners(): Promise<void> {
  if (started || !isTauri()) return;
  started = true;
  // Backend events are broadcast to every window and tagged with the home
  // they belong to; drop what is meant for a window on another account.
  await listen<CodexEvent & { codexHome?: string }>("codex:event", (event) => {
    if (!eventMatchesHome(event.payload.codexHome)) return;
    dispatch(event.payload);
  });
  await listen<{ requestId: number; method: string; params: any; codexHome?: string }>(
    "codex:serverRequest",
    (event) => {
      if (!eventMatchesHome(event.payload.codexHome)) return;
      onServerRequest(event.payload);
    },
  );
  await listen<AgentRunEvent & { codexHome?: string }>("codex:agentRun", (event) => {
    if (!eventMatchesHome(event.payload.codexHome)) return;
    applyAgentRunEvent(event.payload);
  });
  await listen<{ codexHome?: string } | null>("codex:disconnected", (event) => {
    if (!eventMatchesHome(event.payload?.codexHome)) return;
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

export function previewEmitServerRequest(payload: { requestId: number; method: string; params: any }): void {
  onServerRequest(payload);
}
