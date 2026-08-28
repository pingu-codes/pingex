import type { CodexEvent } from "$lib/services/codexEvents.svelte";
import { reviewTransition, threadIdOf, turnEnd } from "$lib/services/turnLifecycle";

/**
 * Commands Codex is running (or ran) across all threads.
 *
 * A long-running task keeps working after the user navigates away, and the
 * thread that owns it may not even be loaded — so the store mirrors the output
 * stream itself rather than reading through a ThreadDetail. Every thread's
 * events flow through `dispatch`, whichever view is mounted, which is what
 * makes cross-thread display possible.
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
}

export const processes = $state<{ list: RunningProcess[] }>({ list: [] });

/** Ticks once a second, but only while something is running. */
export const processClock = $state<{ now: number }>({ now: 0 });
let ticker: ReturnType<typeof setInterval> | null = null;

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

export function processesFor(threadId: string | null | undefined): RunningProcess[] {
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

  if (!reviewExited && (method === "item/started" || method === "item/completed" || method === "item/updated")) {
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
      });
      process = processes.list.at(-1)!;
    }
    process.command = item.command || process.command;
    process.cwd = item.cwd || process.cwd;
    // The completed payload does not repeat streamed output; keep the longer copy.
    if ((item.aggregatedOutput ?? "").length > process.output.length) {
      process.output = item.aggregatedOutput!;
    }
    if (method === "item/completed" || (item.status && item.status !== "inProgress")) {
      const failed = item.status === "failed" || (item.exitCode != null && item.exitCode !== 0);
      finish(process, failed ? "failed" : "completed", item.exitCode ?? null);
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
      });
      process = processes.list.at(-1)!;
      syncTicker();
    }
    appendOutput(process, method === "item/commandExecution/outputDelta" ? params.delta : params.stdin);
    return;
  }

  // A turn ending — however it ends — takes its commands with it. Codex runs
  // commands inside the turn; nothing survives it.
  const end = turnEnd(event);
  if (end) {
    const failed = end.outcome === "failed";
    for (const process of processes.list) {
      if (process.threadId === threadId && process.status === "running") {
        finish(process, failed ? "failed" : "completed");
      }
    }
    trimFinished();
    syncTicker();
  }
}

/** Test helper: drop everything between cases. */
export function resetProcesses(): void {
  processes.list = [];
  syncTicker();
}
