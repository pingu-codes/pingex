import { cleanBackgroundTerminals, listBackgroundTerminals, terminateBackgroundTerminal } from "$lib/services/api";
import type { CodexEvent } from "$lib/services/codexEvents.svelte";
import { threadBelongsToHome } from "$lib/services/homeRouting";
import { reviewTransition, threadIdOf, turnEnd } from "$lib/services/turnLifecycle";

/**
 * Commands Codex is running (or ran) across all threads.
 *
 * A long-running task keeps working after the user navigates away, and the
 * thread that owns it may not even be loaded — so the store mirrors the output
 * stream itself rather than reading through a ThreadDetail. Every thread's
 * events flow through `dispatch`, whichever view is mounted, which is what
 * makes cross-thread display possible.
 *
 * Most commands end with their step. A background terminal does not: Codex
 * keeps it alive past the turn (and past Stop) until it exits or is stopped,
 * and only then reports its `item/completed`. Those are told apart at turn end
 * by their `processId` and confirmed against `thread/backgroundTerminals/list`.
 */
export interface RunningProcess {
  /** `${threadId}:${itemId}` — item ids are only unique within a thread. */
  key: string;
  threadId: string;
  turnId: string;
  itemId: string;
  command: string;
  cwd: string;
  status: "running" | "completed" | "failed" | "interrupted";
  startedAt: number;
  finishedAt: number | null;
  /** Mirrored aggregated output, with Codex's stdin interleaved. */
  output: string;
  exitCode: number | null;
  /** Codex's PTY session id: what stopping it takes. */
  processId: string | null;
  /** Still running after its turn ended — a dev server, a watcher. */
  background: boolean;
}

export const processes = $state<{ list: RunningProcess[] }>({ list: [] });

/** Ticks once a second, but only while something is running. */
export const processClock = $state<{ now: number }>({ now: 0 });
let ticker: ReturnType<typeof setInterval> | null = null;

/** How long something has been running, as `1m 04s`. */
export function elapsedLabel(since: number, now: number): string {
  const seconds = Math.max(0, Math.floor(((now || Date.now()) - since) / 1000));
  if (seconds < 60) return `${seconds}s`;
  return `${Math.floor(seconds / 60)}m ${String(seconds % 60).padStart(2, "0")}s`;
}

/** A finished process's tail is worth keeping; an unbounded mirror is not. */
const OUTPUT_CAP = 200_000;
/** Finished entries retained per session before the oldest are dropped. */
const FINISHED_CAP = 50;

function syncTicker(): void {
  const working = processes.list.some((process) => process.status === "running");
  if (working && ticker === null) {
    processClock.now = Date.now();
    ticker = setInterval(() => {
      processClock.now = Date.now();
    }, 1000);
  } else if (!working && ticker !== null) {
    clearInterval(ticker);
    ticker = null;
  }
}

function processKey(threadId: string, itemId: string): string {
  return `${threadId}:${itemId}`;
}

export function processByKey(key: string): RunningProcess | null {
  return processes.list.find((process) => process.key === key) ?? null;
}

function processesFor(threadId: string | null | undefined): RunningProcess[] {
  return threadId ? processes.list.filter((process) => process.threadId === threadId) : [];
}

export function runningProcessCount(threadId: string | null | undefined): number {
  return processesFor(threadId).filter((process) => process.status === "running").length;
}

function appendOutput(process: RunningProcess, text: string): void {
  process.output += text;
  if (process.output.length > OUTPUT_CAP) {
    process.output = `…\n${process.output.slice(process.output.length - OUTPUT_CAP)}`;
  }
}

function finish(process: RunningProcess, status: RunningProcess["status"], exitCode: number | null = null): void {
  process.status = status;
  process.exitCode = exitCode ?? process.exitCode;
  process.finishedAt = process.finishedAt ?? Date.now();
}

/** Drop the oldest finished entries once the session has accumulated plenty. */
function trimFinished(): void {
  const finished = processes.list.filter((process) => process.status !== "running");
  if (finished.length <= FINISHED_CAP) return;
  const drop = new Set(finished.slice(0, finished.length - FINISHED_CAP).map((process) => process.key));
  processes.list = processes.list.filter((process) => !drop.has(process.key));
}

/** Fold one Codex event into the registry. Called from `dispatch` for every event. */
export function applyProcessEvent(event: CodexEvent): void {
  if (event.method === "disconnected") {
    // Nothing can report these commands finishing any more.
    for (const process of processes.list) {
      if (event.homeKey && !threadBelongsToHome(process.threadId, event.homeKey)) continue;
      if (process.status === "running") finish(process, "interrupted");
    }
    syncTicker();
    return;
  }
  const threadId = threadIdOf(event);
  if (!threadId) return;
  const { method, params } = event;

  // A review ends by leaving review mode, never by `turn/completed`.
  const reviewExited = reviewTransition(event) === "exited";

  if (!reviewExited && (method === "item/started" || method === "item/completed")) {
    const item = params.item;
    if (item?.type !== "commandExecution" || !item.id) return;
    const key = processKey(threadId, item.id);
    let process = processByKey(key);
    if (!process) {
      processes.list.push({
        key,
        threadId,
        turnId: params.turnId ?? "",
        itemId: item.id,
        command: item.command ?? "",
        cwd: item.cwd ?? "",
        status: "running",
        startedAt: Date.now(),
        finishedAt: null,
        output: item.aggregatedOutput ?? "",
        exitCode: null,
        processId: item.processId ?? null,
        background: false,
      });
      process = processes.list.at(-1)!;
    }
    process.command = item.command || process.command;
    process.cwd = item.cwd || process.cwd;
    process.processId = item.processId ?? process.processId;
    // The completed payload does not repeat streamed output; keep the longer copy.
    if ((item.aggregatedOutput ?? "").length > process.output.length) {
      process.output = item.aggregatedOutput!;
    }
    if (method === "item/completed" || (item.status && item.status !== "inProgress")) {
      const failed = item.status === "failed" || (item.exitCode != null && item.exitCode !== 0);
      // A process the user stopped exits non-zero; it still reads as stopped.
      const status = process.status === "interrupted" ? "interrupted" : failed ? "failed" : "completed";
      finish(process, status, item.exitCode ?? null);
      trimFinished();
    }
    syncTicker();
    return;
  }

  if (method === "item/commandExecution/outputDelta" || method === "item/commandExecution/terminalInteraction") {
    const itemId = params.itemId;
    if (typeof itemId !== "string" || !itemId) return;
    const key = processKey(threadId, itemId);
    let process = processByKey(key);
    if (!process) {
      // The delta beat `item/started`; register with what we know.
      processes.list.push({
        key,
        threadId,
        turnId: params.turnId ?? "",
        itemId,
        command: "",
        cwd: "",
        status: "running",
        startedAt: Date.now(),
        finishedAt: null,
        output: "",
        exitCode: null,
        processId: null,
        background: false,
      });
      process = processes.list.at(-1)!;
      syncTicker();
    }
    appendOutput(process, method === "item/commandExecution/outputDelta" ? params.delta : params.stdin);
    return;
  }

  // A turn ending — however it ends — takes its ordinary commands with it. One
  // with a PTY session may live on as a background terminal; Codex's list says
  // which, and the rest finish then.
  const end = turnEnd(event);
  if (end) {
    const failed = end.outcome === "failed";
    let survivors = false;
    for (const process of processes.list) {
      if (process.threadId !== threadId || process.status !== "running") continue;
      if (process.processId) {
        process.background = true;
        survivors = true;
      } else {
        finish(process, failed ? "failed" : "completed");
      }
    }
    trimFinished();
    syncTicker();
    if (survivors) void reconcileBackground(threadId);
  }
}

const reconciling = new Map<string, Promise<void>>();

/**
 * Bring a thread's background terminals in line with what Codex reports:
 * background entries it no longer lists have ended, and listed terminals this
 * window never saw start (they began before a reload) are added. A failed
 * read changes nothing.
 */
export function reconcileBackground(threadId: string): Promise<void> {
  const running = reconciling.get(threadId);
  if (running) return running;
  const work = listBackgroundTerminals(threadId)
    .then((terminals) => {
      const live = new Map(terminals.map((terminal) => [terminal.itemId, terminal]));
      for (const process of processesFor(threadId)) {
        if (process.status === "running" && process.background && !live.has(process.itemId)) {
          finish(process, "completed");
        }
      }
      for (const terminal of terminals) {
        const known = processByKey(processKey(threadId, terminal.itemId));
        if (known) {
          known.processId = terminal.processId;
          continue;
        }
        processes.list.push({
          key: processKey(threadId, terminal.itemId),
          threadId,
          turnId: "",
          itemId: terminal.itemId,
          command: terminal.command,
          cwd: terminal.cwd,
          status: "running",
          // When it really started is not reported; this is when it was found.
          startedAt: Date.now(),
          finishedAt: null,
          output: "",
          exitCode: null,
          processId: terminal.processId,
          background: true,
        });
      }
      trimFinished();
      syncTicker();
    })
    .catch(() => {})
    .finally(() => reconciling.delete(threadId));
  reconciling.set(threadId, work);
  return work;
}

/** Stop one command that has a PTY session. Resolves whether Codex stopped it;
 *  `false` means it had already gone, and the list settles the entry. */
export async function terminateProcess(process: RunningProcess): Promise<boolean> {
  if (!process.processId) return false;
  const stopped = await terminateBackgroundTerminal(process.threadId, process.processId);
  if (stopped && process.status === "running") finish(process, "interrupted");
  else if (!stopped) await reconcileBackground(process.threadId);
  syncTicker();
  return stopped;
}

/** Stop every background terminal of a thread. */
export async function stopAllBackground(threadId: string): Promise<void> {
  await cleanBackgroundTerminals(threadId);
  for (const process of processesFor(threadId)) {
    if (process.status === "running" && process.background) finish(process, "interrupted");
  }
  syncTicker();
  await reconcileBackground(threadId);
}

/** Background terminals still running in a thread. */
export function backgroundProcessCount(threadId: string | null | undefined): number {
  return processesFor(threadId).filter((process) => process.status === "running" && process.background).length;
}

/** Test helper: drop everything between cases. */
export function resetProcesses(): void {
  processes.list = [];
  reconciling.clear();
  syncTicker();
}
