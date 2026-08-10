import type { AgentRun } from "$lib/types";

/**
 * Agents the app has spawned, keyed by the thread that spawned them.
 *
 * Seeded from `listAgentRuns` when a thread opens and kept current by the
 * `codex:agentRun` event, which the Rust supervisor emits on every state
 * change. Global rather than per-view for the same reason `activeTurns` is:
 * agents keep running after the user navigates away, and the sidebar wants
 * their status regardless of which thread is open.
 */
export const agentRuns = $state<{ byThread: Record<string, AgentRun[]> }>({ byThread: {} });

/** What the supervisor sends on `codex:agentRun`. */
export interface AgentRunEvent {
  runId: string;
  parentThreadId: string;
  /** Joins this run to its `dynamicToolCall` row in the parent's transcript. */
  callId: string | null;
  childThreadId: string | null;
  name: string;
  status: string;
  result: string | null;
  error: string | null;
}

const TERMINAL = new Set(["done", "failed", "killed", "orphaned"]);

export function isRunning(run: AgentRun): boolean {
  return !TERMINAL.has(run.status);
}

/**
 * Reconcile a thread's runs with what the database has, as after a
 * `listAgentRuns`.
 *
 * Not a plain replace. The stored row is written asynchronously as a run
 * progresses, so a read can catch it before it has learned the child thread id
 * or the final status, while the events that told us those already landed here.
 * Overwriting with the row would drop a running agent out of the side menu the
 * moment the user switched threads — which is exactly what a refresh must not
 * do. Fields the row has nothing for keep whatever the events established, and
 * a live run the read has not caught up with is kept.
 */
export function setAgentRuns(threadId: string, runs: AgentRun[]): void {
  const known = agentRuns.byThread[threadId] ?? [];
  const byId = new Map(known.map((run) => [run.runId, run]));
  const merged = runs.map((stored) => {
    const live = byId.get(stored.runId);
    if (!live) return stored;
    byId.delete(stored.runId);
    return {
      ...stored,
      callId: stored.callId ?? live.callId,
      childThreadId: stored.childThreadId ?? live.childThreadId,
      result: stored.result || live.result,
      error: stored.error ?? live.error,
      // A terminal state the events reported wins over a row that still says
      // running; the reverse (a stale live "running") never happens, since
      // every state change comes through the same event.
      status: isRunning(stored) && !isRunning(live) ? live.status : stored.status,
      finishedAt: stored.finishedAt ?? live.finishedAt,
    };
  });
  // Whatever is left never made it into the read: either it was written after
  // the query ran, or its row was lost. Keep it while it is still working.
  agentRuns.byThread[threadId] = [...merged, ...[...byId.values()].filter(isRunning)];
  for (const run of agentRuns.byThread[threadId]) claimOrphanedActivity(run);
}

export function runsFor(threadId: string | null | undefined): AgentRun[] {
  return (threadId && agentRuns.byThread[threadId]) || [];
}

/** How many of a thread's agents are still working. */
export function runningCount(threadId: string | null | undefined): number {
  return runsFor(threadId).filter(isRunning).length;
}

/**
 * Fold one event into the store. An event for a run we have not seen creates
 * it: the event can beat the `listAgentRuns` that would have introduced it,
 * and a card that appears late reads as a bug.
 */
export function applyAgentRunEvent(event: AgentRunEvent): void {
  const { parentThreadId } = event;
  if (!parentThreadId) return;
  const existing = agentRuns.byThread[parentThreadId] ?? [];
  const index = existing.findIndex((run) => run.runId === event.runId);
  if (index === -1) {
    agentRuns.byThread[parentThreadId] = [
      ...existing,
      {
        runId: event.runId,
        parentThreadId,
        parentTurnId: "",
        callId: event.callId,
        childThreadId: event.childThreadId,
        name: event.name,
        prompt: "",
        cwd: "",
        model: null,
        reasoningEffort: null,
        status: event.status,
        result: event.result,
        error: event.error,
        createdAt: Date.now(),
        finishedAt: null,
      },
    ];
    claimOrphanedActivity(agentRuns.byThread[parentThreadId].at(-1)!);
    return;
  }
  if (TERMINAL.has(event.status)) clearActivity(event.runId);
  agentRuns.byThread[parentThreadId] = existing.map((run, at) =>
    at === index
      ? {
          ...run,
          callId: event.callId ?? run.callId,
          childThreadId: event.childThreadId ?? run.childThreadId,
          name: event.name || run.name,
          status: event.status,
          // A run only goes back to running when a follow-up turn starts, and
          // the last agent message *is* the result — so the previous turn's
          // answer is no longer one. Once it has finished, an event carrying
          // nothing must not erase what an earlier one delivered.
          result: TERMINAL.has(event.status) ? event.result || run.result : event.result || null,
          error: event.error ?? run.error,
          finishedAt: TERMINAL.has(event.status) ? (run.finishedAt ?? Date.now()) : run.finishedAt,
        }
      : run,
  );
  const run = agentRuns.byThread[parentThreadId][index];
  if (run) claimOrphanedActivity(run);
}

/**
 * What an agent is doing right now, for the card in the parent's transcript.
 *
 * `codex:agentRun` only fires on a state change, so between "running" and
 * "done" — which can be many minutes — the card has nothing to say and reads as
 * stuck. The child's own item stream is already on `codex:event`; it is simply
 * addressed to a thread no view is showing. This folds that stream down to one
 * line per run.
 *
 * Deliberately not part of `AgentRun`: that mirrors a database row, and none of
 * this is persisted. An agent's activity is only meaningful while it is live.
 */
export interface AgentActivity {
  /** What it is doing, e.g. `$ rg -n foo src/`. */
  label: string;
  /** When the current turn started, for the elapsed counter. */
  since: number;
}

export const agentActivity = $state<{ byRun: Record<string, AgentActivity> }>({ byRun: {} });

/**
 * A second hand for the elapsed counters, shared so a thread full of agent
 * cards keeps one timer rather than one each — and stopped entirely when no
 * agent is working, which is almost always.
 */
export const agentClock = $state<{ now: number }>({ now: 0 });
let ticker: ReturnType<typeof setInterval> | null = null;

function syncTicker(): void {
  const working = Object.keys(agentActivity.byRun).length > 0;
  if (working && ticker === null) {
    agentClock.now = Date.now();
    ticker = setInterval(() => {
      agentClock.now = Date.now();
    }, 1000);
  } else if (!working && ticker !== null) {
    clearInterval(ticker);
    ticker = null;
  }
}

/** How long an agent has been on its current turn, as `1m 04s`. */
export function elapsedLabel(since: number, now: number = agentClock.now): string {
  const seconds = Math.max(0, Math.floor(((now || Date.now()) - since) / 1000));
  if (seconds < 60) return `${seconds}s`;
  return `${Math.floor(seconds / 60)}m ${String(seconds % 60).padStart(2, "0")}s`;
}

/**
 * Activity seen before we knew which run the thread belonged to.
 *
 * A spawn sets the child thread id and starts the turn in the same breath, so
 * the child's first notifications can beat the `codex:agentRun` that carries
 * the id here. Holding them means the first thing an agent does still shows.
 */
const orphanedActivity = new Map<string, AgentActivity>();

/** The one-line summary an event contributes, or null to leave the last one. */
export function activityLabel(event: { method: string; params: any }): string | null {
  const { method, params } = event;
  if (method === "turn/started") return "starting…";
  if (method === "item/agentMessage/delta") return "writing…";
  if (method.startsWith("item/reasoning/summary")) return "thinking…";
  if (method !== "item/started" && method !== "item/completed") return null;
  const item = params?.item;
  switch (item?.type) {
    case "commandExecution":
      return item.command ? `$ ${item.command}` : "running a command";
    case "fileChange": {
      const path = item.changes?.[0]?.path;
      const more = (item.changes?.length ?? 0) - 1;
      if (!path) return "editing files";
      return more > 0 ? `editing ${path} +${more} more` : `editing ${path}`;
    }
    case "webSearch":
      return item.query ? `searching “${item.query}”` : "searching the web";
    case "mcpToolCall":
    case "dynamicToolCall":
      return item.tool ? `${item.tool}` : "calling a tool";
    case "reasoning":
      return "thinking…";
    case "agentMessage":
      return "writing…";
    default:
      return null;
  }
}

function runByChildThreadId(threadId: string): AgentRun | null {
  for (const runs of Object.values(agentRuns.byThread)) {
    const match = runs.find((run) => run.childThreadId === threadId);
    if (match) return match;
  }
  return null;
}

function setActivity(runId: string, label: string, restart: boolean): void {
  const since = restart ? Date.now() : (agentActivity.byRun[runId]?.since ?? Date.now());
  agentActivity.byRun[runId] = { label, since };
  syncTicker();
}

function clearActivity(runId: string): void {
  delete agentActivity.byRun[runId];
  syncTicker();
}

/**
 * Fold one of a child's notifications into its run's activity line. Events for
 * threads that are not a known agent — every ordinary thread — fall straight
 * through.
 */
export function applyAgentActivity(event: { method: string; params: any }): void {
  const threadId = event.params?.threadId;
  if (typeof threadId !== "string" || !threadId) return;
  const run = runByChildThreadId(threadId);
  if (event.method === "turn/completed" || event.method === "error") {
    // The run's own status takes over from here; a stale "thinking…" beside a
    // finished agent is worse than no line at all.
    if (run) clearActivity(run.runId);
    orphanedActivity.delete(threadId);
    return;
  }
  const label = activityLabel(event);
  if (label === null) return;
  const restart = event.method === "turn/started";
  if (!run) {
    const held = orphanedActivity.get(threadId);
    orphanedActivity.set(threadId, {
      label,
      since: restart || !held ? Date.now() : held.since,
    });
    return;
  }
  setActivity(run.runId, label, restart);
}

/** Hand a run whatever its thread did before we knew the two were connected. */
function claimOrphanedActivity(run: AgentRun): void {
  if (!run.childThreadId) return;
  const held = orphanedActivity.get(run.childThreadId);
  if (!held) return;
  orphanedActivity.delete(run.childThreadId);
  agentActivity.byRun[run.runId] ??= held;
  syncTicker();
}

export function activityFor(run: AgentRun): AgentActivity | null {
  return agentActivity.byRun[run.runId] ?? null;
}

/**
 * Find the run behind a `dynamicToolCall` item. The transcript item carries the
 * `callId` the spawn was answered under (the item's own id), which is the only
 * link between what the parent shows and what the supervisor is running.
 */
export function runByCallId(callId: string | null | undefined): AgentRun | null {
  if (!callId) return null;
  for (const runs of Object.values(agentRuns.byThread)) {
    const match = runs.find((run) => run.callId === callId);
    if (match) return match;
  }
  return null;
}

/**
 * The run a transcript row refers to.
 *
 * Prefers the call id, then falls back to the agent's name. The fallback earns
 * its keep because the id link is fragile on a re-read: `thread/read` drops
 * `dynamicToolCall` items altogether and renumbers everything it does return,
 * so the row only keeps its real id while the local journal is merged back in.
 * A card that has lost its run renders inert — no status, no way into the
 * agent's thread — which is worse than occasionally picking the wrong one of
 * two identically-named agents.
 */
export function runForToolCall(item: { id?: string; arguments?: Record<string, unknown> }): AgentRun | null {
  const byId = runByCallId(item.id);
  if (byId) return byId;
  const name = item.arguments?.name;
  if (typeof name !== "string" || !name.trim()) return null;
  for (const runs of Object.values(agentRuns.byThread)) {
    const match = runs.find((run) => run.name === name.trim());
    if (match) return match;
  }
  return null;
}

/** Test helper: drop everything between cases. */
export function resetAgentRuns(): void {
  agentRuns.byThread = {};
  agentActivity.byRun = {};
  orphanedActivity.clear();
  syncTicker();
}
