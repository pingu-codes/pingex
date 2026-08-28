/**
 * The rules for when a thread's turn starts and ends, as read off the event
 * stream. Every store that tracks "is this thread working" — the sidebar's
 * active set, the transcript, the process registry — asks these instead of
 * matching method strings itself, so a rule change lands in one place.
 */
import type { CodexEvent } from "$lib/types";

/** The thread an event is addressed to, when it names one. */
export function threadIdOf(event: CodexEvent): string | null {
  const params = event.params as { threadId?: unknown } | null;
  return typeof params?.threadId === "string" && params.threadId ? params.threadId : null;
}

export type ReviewTransition = "entered" | "exited" | null;

/**
 * Entering or leaving review mode. Codex signals both with items rather than
 * turn events, and a review never gets a `turn/completed` of its own.
 */
export function reviewTransition(event: CodexEvent): ReviewTransition {
  if (event.method !== "item/started" && event.method !== "item/completed") return null;
  const type = event.params.item?.type;
  if (type === "enteredReviewMode") return "entered";
  if (event.method === "item/completed" && type === "exitedReviewMode") return "exited";
  return null;
}

/**
 * An error Codex is about to retry. The turn stays running through it, so it
 * must be shown as a notice, never as the turn's end.
 */
export function isRetryableError(event: CodexEvent): boolean {
  return event.method === "error" && Boolean(event.params.willRetry);
}

export interface TurnEnd {
  threadId: string;
  outcome: "completed" | "failed" | "reviewExited";
}

/**
 * Whether this event ends the thread's turn, and how. Three things do:
 * `turn/completed`; an `error` Codex is not going to retry (one it will retry
 * leaves the turn running); and leaving review mode.
 */
export function turnEnd(event: CodexEvent): TurnEnd | null {
  const threadId = threadIdOf(event);
  if (!threadId) return null;
  if (event.method === "turn/completed") return { threadId, outcome: "completed" };
  if (event.method === "error" && !isRetryableError(event)) return { threadId, outcome: "failed" };
  if (reviewTransition(event) === "exited") return { threadId, outcome: "reviewExited" };
  return null;
}
