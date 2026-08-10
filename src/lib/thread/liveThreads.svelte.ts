/**
 * Transcripts of threads that are mid-turn but whose view is not mounted.
 *
 * Switching threads remounts `ThreadView`, so without this the working state of
 * the thread being left behind is thrown away: its stream events go nowhere and
 * coming back re-reads a transcript that predates the running turn (the local
 * detail cache is only keyed by the summary's `updated_at`, which does not move
 * until the turn ends). The retained document keeps receiving events, so
 * returning to a working thread shows exactly what it was doing.
 *
 * The mounted view still applies events to its own thread; this store covers
 * every other retained one, and hands the document back on remount.
 */
import { invalidateThreadCache } from "$lib/services/api";
import { activeTurns, type CodexEvent, setThreadHandler } from "$lib/services/codexEvents.svelte";
import { applyThreadEvent } from "$lib/thread/threadStream";
import type { SubagentPolicy, ThreadDetail, TurnOptions, UserInputPart } from "$lib/types";

export interface QueuedMessage {
  input: UserInputPart[];
  options?: TurnOptions;
}

/** The part of a thread view's state that outlives its component. */
export interface LiveThread {
  detail: ThreadDetail;
  queued: QueuedMessage[];
  compacting: boolean;
  streamError: string | null;
  subagentModelPolicy: SubagentPolicy | null;
  subagentReasoningEffortPolicy: SubagentPolicy | null;
}

/** Session state that a view writes back when it unmounts. */
export type LiveSession = Omit<LiveThread, "detail">;

const live = $state<Record<string, LiveThread>>({});

/** Thread whose view is mounted — that view applies its own events. */
let openThreadId: string | null = null;
let listening = false;

function listen() {
  if (listening) return;
  listening = true;
  setThreadHandler(onEvent);
}

/** A document is only worth keeping while there is work attached to it. */
function working(id: string, entry: LiveThread): boolean {
  return (
    activeTurns.list.includes(id) ||
    entry.detail.turns.some((turn) => turn.status === "inProgress") ||
    entry.queued.length > 0 ||
    entry.streamError !== null
  );
}

function onEvent(event: CodexEvent) {
  if (event.method === "disconnected") {
    // Nothing retained can make progress any more, and a reconnected session is
    // the honest source for what actually survived — forget the documents the
    // open view is not holding on to.
    for (const id of Object.keys(live)) {
      if (id !== openThreadId) delete live[id];
    }
    return;
  }
  const id = event.params?.threadId;
  if (typeof id !== "string" || id === openThreadId) return;
  const entry = live[id];
  if (!entry) return;
  if (event.method === "thread/compacted") entry.compacting = false;
  if (event.method === "thread/settings/updated") {
    entry.subagentModelPolicy = event.params.threadSettings?.subagentModelPolicy ?? null;
    entry.subagentReasoningEffortPolicy = event.params.threadSettings?.subagentReasoningEffortPolicy ?? null;
  }
  const outcome = applyThreadEvent(entry.detail, event);
  if (outcome.streamError) entry.streamError = outcome.streamError;
  if (outcome.turnCompleted) {
    entry.compacting = false;
    // The detail cache still holds the transcript from before this turn; drop it
    // so a later read of this thread does not serve it back.
    invalidateThreadCache(id).catch(() => {});
    if (!working(id, entry)) delete live[id];
  }
}

/**
 * Claim the thread a view is mounting on, returning the retained document when
 * the thread was left mid-work. The caller owns event handling for it from here.
 */
export function adoptLive(threadId: string): LiveThread | null {
  listen();
  openThreadId = threadId;
  return live[threadId] ?? null;
}

/** Register a freshly read transcript as the open thread's live document. */
export function trackLive(threadId: string, detail: ThreadDetail): LiveThread {
  listen();
  openThreadId = threadId;
  live[threadId] = {
    detail,
    queued: [],
    compacting: false,
    streamError: null,
    subagentModelPolicy: detail.subagentModelPolicy ?? null,
    subagentReasoningEffortPolicy: detail.subagentReasoningEffortPolicy ?? null,
  };
  return live[threadId];
}

/**
 * The view is going away. Its session state is written back, and the document
 * is kept only while the thread still has work in flight — an idle thread is
 * cheaper (and safer) to read back from Codex next time.
 */
export function releaseLive(threadId: string, session: LiveSession): void {
  if (openThreadId === threadId) openThreadId = null;
  const entry = live[threadId];
  if (!entry) return;
  Object.assign(entry, session);
  if (!working(threadId, entry)) delete live[threadId];
}

/** Test seam: forget every retained document and re-subscribe from scratch. */
export function resetLiveThreads(): void {
  for (const id of Object.keys(live)) delete live[id];
  openThreadId = null;
  listening = false;
}
