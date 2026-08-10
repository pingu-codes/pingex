import type { CodexEvent } from "$lib/services/codexEvents.svelte";
import { mergeFileChanges } from "$lib/thread/fileChanges";
import type { ThreadDetail, ThreadItem, Turn } from "$lib/types";

export function ensureTurn(turns: Turn[], turnId: string): Turn {
  const turn = turns.find((candidate) => candidate.id === turnId);
  if (turn) return turn;
  // Adopt the optimistic turn pushed on send if the real id just arrived.
  const local = turns.find((candidate) => candidate.id.startsWith("local-"));
  if (local) {
    local.id = turnId;
    return local;
  }
  turns.push({ id: turnId, status: "inProgress", items: [] });
  return turns[turns.length - 1];
}

/**
 * The review Codex is running right now, if any: a turn that entered review
 * mode and has not left it again.
 */
function openReviewTurn(turns: Turn[]): Turn | undefined {
  return turns.find(
    (turn) =>
      turn.status === "inProgress" &&
      turn.items.some((item) => item.type === "enteredReviewMode") &&
      !turn.items.some((item) => item.type === "exitedReviewMode"),
  );
}

/**
 * End every turn still marked as running.
 *
 * Used where Codex stops streaming without ever completing the turn it was
 * streaming for. Left alone those turns keep the transcript showing the typing
 * indicator, and the composer offering Stop, for work that finished long ago.
 */
export function finalizeRunningTurns(turns: Turn[], status: string) {
  for (const turn of turns) {
    if (turn.status === "inProgress") turn.status = status;
  }
}

export function ensureItem(turns: Turn[], turnId: string, itemId: string, type: string): ThreadItem {
  const turn = ensureTurn(turns, turnId);
  let item = turn.items.find((candidate) => candidate.id === itemId);
  if (!item) {
    turn.items.push({ type, id: itemId });
    item = turn.items[turn.items.length - 1];
  }
  return item;
}

/**
 * Carries over the text an item accumulated from its deltas. Codex's
 * `item/completed` payload does not repeat what it streamed — a completed
 * `reasoning` item always arrives with `summary: []` — so replacing the item
 * wholesale would erase the only copy of the text the app ever gets.
 *
 * File changes are unioned rather than preferred one way or the other: a
 * completed `fileChange` may report only some of the files its patch touched,
 * and anything dropped here is gone from the thread's file list for good.
 */
function mergeItem(existing: ThreadItem, incoming: ThreadItem): ThreadItem {
  const merged = { ...incoming };
  if ((existing.changes ?? []).length > 0) {
    merged.changes = mergeFileChanges(existing.changes, merged.changes);
  }
  if (!(merged.summary ?? []).some(Boolean) && (existing.summary ?? []).some(Boolean)) {
    merged.summary = existing.summary;
  }
  if (!(merged.content ?? []).some(Boolean) && (existing.content ?? []).some(Boolean)) {
    merged.content = existing.content;
  }
  if (!merged.aggregatedOutput && existing.aggregatedOutput) {
    merged.aggregatedOutput = existing.aggregatedOutput;
  }
  if (!merged.text && existing.text) merged.text = existing.text;
  return merged;
}

export function upsertItem(turns: Turn[], turnId: string, incoming: ThreadItem): void {
  const turn = ensureTurn(turns, turnId);
  if (incoming.type === "userMessage") {
    const localIndex = turn.items.findIndex(
      (candidate) => candidate.type === "userMessage" && candidate.id.startsWith("local-"),
    );
    if (localIndex >= 0) {
      turn.items[localIndex] = incoming;
      return;
    }
  }
  const index = turn.items.findIndex((candidate) => candidate.id === incoming.id);
  if (index >= 0) turn.items[index] = mergeItem(turn.items[index], incoming);
  else turn.items.push(incoming);
}

/** Shown while Codex deliberately stalls the stream to check a response. */
export const BUFFERING_NOTICE = "Checking the response before showing it…";

export interface ApplyOutcome {
  /** Set when the stream reported an error message to surface. */
  streamError?: string;
  /**
   * Something worth telling the user that did not end the turn — a warning, a
   * deprecation notice, an error Codex is about to retry. Kept apart from
   * `streamError` so a turn that recovers does not look like it failed.
   */
  notice?: string;
  /** Safety buffering stopped; the buffering notice no longer applies. */
  bufferingEnded?: boolean;
  /** A turn finished; the caller should invalidate the thread cache. */
  turnCompleted?: boolean;
  /** A collab tool call landed; the caller should refresh subagents. */
  collabToolCall?: boolean;
  /** The event targeted this thread (caller should keep the view scrolled). */
  changed: boolean;
}

/**
 * Applies a streaming Codex event to the thread's turns in place. The thread
 * may be a $state proxy; mutations flow through Svelte reactivity.
 */
export function applyThreadEvent(thread: ThreadDetail, { method, params }: CodexEvent): ApplyOutcome {
  const outcome: ApplyOutcome = { changed: true };
  switch (method) {
    case "turn/started":
      // Mid-review, Codex announces a turn of its own under an id no item ever
      // references and no `turn/completed` ever names — bookkeeping for the
      // seeded review message, not new work. Adopting it would leave a second,
      // permanently empty turn running for the rest of the thread's life.
      if (openReviewTurn(thread.turns) && !thread.turns.some((turn) => turn.id === params.turn.id)) {
        break;
      }
      ensureTurn(thread.turns, params.turn.id).status = "inProgress";
      break;
    case "turn/completed": {
      // Codex can complete a turn under an id it never streamed items for. It
      // resolves that the same way on its own side: an unmatched completion
      // applies to whatever is currently running.
      const matched = thread.turns.find((candidate) => candidate.id === params.turn.id);
      const turn = matched ?? thread.turns.find((candidate) => candidate.status === "inProgress");
      if (turn) {
        turn.status = params.turn.status;
        turn.error = params.turn.error ?? null;
        turn.durationMs = params.turn.durationMs ?? turn.durationMs;
        turn.startedAt = params.turn.startedAt ?? turn.startedAt;
        turn.completedAt = params.turn.completedAt ?? turn.completedAt;
        // Only if Codex reports them; otherwise the locally recorded pair stands.
        turn.model = params.turn.model ?? turn.model;
        turn.reasoningEffort = params.turn.reasoningEffort ?? turn.reasoningEffort;
      }
      // An unmatched completion means Codex considers the whole thread idle —
      // the session store clears it outright — so no other turn may stay
      // running here either, or the composer sits busy forever.
      if (!matched) finalizeRunningTurns(thread.turns, params.turn.status);
      outcome.turnCompleted = true;
      break;
    }
    case "item/started":
    case "item/completed":
      upsertItem(thread.turns, params.turnId, params.item);
      if (params.item?.type === "collabAgentToolCall") outcome.collabToolCall = true;
      // Leaving review mode is the only end a review gets: Codex sends no
      // `turn/completed` for one. Only the review's own turn ends here — a
      // queued message that raced in must not be marked completed with it.
      if (method === "item/completed" && params.item?.type === "exitedReviewMode") {
        const reviewTurn = thread.turns.find((turn) => turn.items.some((item) => item.id === params.item.id));
        if (reviewTurn) {
          if (reviewTurn.status === "inProgress") reviewTurn.status = "completed";
        } else {
          finalizeRunningTurns(thread.turns, "completed");
        }
        outcome.turnCompleted = true;
      }
      break;
    case "item/agentMessage/delta": {
      const item = ensureItem(thread.turns, params.turnId, params.itemId, "agentMessage");
      item.text = (item.text ?? "") + params.delta;
      // `item/completed` replaces the item wholesale, which clears this again.
      item.streaming = true;
      break;
    }
    // Plans stream exactly like agent messages; without this the whole plan
    // appears at once when the item completes.
    case "item/plan/delta": {
      const item = ensureItem(thread.turns, params.turnId, params.itemId, "plan");
      item.text = (item.text ?? "") + params.delta;
      item.streaming = true;
      break;
    }
    case "item/reasoning/summaryPartAdded": {
      const item = ensureItem(thread.turns, params.turnId, params.itemId, "reasoning");
      item.summary = item.summary ?? [];
      while (item.summary.length <= params.summaryIndex) item.summary.push("");
      break;
    }
    case "item/reasoning/summaryTextDelta": {
      const item = ensureItem(thread.turns, params.turnId, params.itemId, "reasoning");
      item.summary = item.summary ?? [];
      while (item.summary.length <= params.summaryIndex) item.summary.push("");
      item.summary[params.summaryIndex] += params.delta;
      break;
    }
    // The model's unsummarised reasoning, indexed separately from the summary
    // it also streams. Only shown on request — it is long and repetitive — but
    // it has to be collected as it arrives, because the completed item does not
    // repeat it any more than it repeats the summary.
    case "item/reasoning/textDelta": {
      const item = ensureItem(thread.turns, params.turnId, params.itemId, "reasoning");
      // `content` is `UserInputPart[]` on a user message and `string[]` here;
      // on a reasoning item only the latter ever occurs.
      const content = (item.content ?? []) as string[];
      while (content.length <= params.contentIndex) content.push("");
      content[params.contentIndex] += params.delta;
      item.content = content;
      break;
    }
    case "item/commandExecution/outputDelta": {
      const item = ensureItem(thread.turns, params.turnId, params.itemId, "commandExecution");
      item.aggregatedOutput = (item.aggregatedOutput ?? "") + params.delta;
      break;
    }
    // What Codex typed into an interactive command. The command's own output
    // echoes nothing back, so without this the transcript shows the prompt and
    // then the result with no sign of the answer in between.
    case "item/commandExecution/terminalInteraction": {
      const item = ensureItem(thread.turns, params.turnId, params.itemId, "commandExecution");
      item.aggregatedOutput = (item.aggregatedOutput ?? "") + params.stdin;
      break;
    }
    // A long-running MCP tool reporting where it has got to. Replaces rather
    // than accumulates: each message supersedes the last.
    case "item/mcpToolCall/progress": {
      const item = ensureItem(thread.turns, params.turnId, params.itemId, "mcpToolCall");
      item.progress = params.message;
      break;
    }
    case "item/fileChange/patchUpdated": {
      const item = ensureItem(thread.turns, params.turnId, params.itemId, "fileChange");
      item.changes = mergeFileChanges(item.changes, params.changes);
      break;
    }
    // Codex's own risk assessment of an action before it runs. Attached to the
    // item it judged, so a command that gets blocked can say why instead of
    // just failing. `targetItemId` is absent for network-policy reviews, which
    // belong to no single item — those are dropped rather than mis-attributed.
    case "item/autoApprovalReview/completed": {
      if (!params.targetItemId) break;
      const item = thread.turns
        .find((turn) => turn.id === params.turnId)
        ?.items.find((candidate) => candidate.id === params.targetItemId);
      if (item) item.guardianReview = params.review;
      break;
    }
    // Codex swapped the model out mid-turn. The user picked the other one, so
    // this is worth saying out loud.
    case "model/rerouted":
      outcome.notice = `Switched from ${params.fromModel} to ${params.toModel}.`;
      break;
    // A deliberate stall while the response is checked. Only announced when
    // Codex asks for it, since it is otherwise invisible and looks like a hang
    // — and withdrawn again when the stall ends, since it describes a state
    // rather than an event.
    case "model/safetyBuffering/updated":
      if (params.showBufferingUi) outcome.notice = BUFFERING_NOTICE;
      else outcome.bufferingEnded = true;
      break;
    // Hooks are the user's own code. A successful run is not worth mentioning;
    // one that failed or blocked the turn very much is, because nothing else in
    // the transcript would show it.
    case "hook/completed": {
      const status = params.run?.status;
      if (status === "failed" || status === "blocked" || status === "stopped") {
        const detail = params.run?.statusMessage ?? hookOutput(params.run);
        outcome.notice = `Hook ${params.run?.eventName ?? ""} ${status}${detail ? `: ${detail}` : ""}`.trim();
      }
      break;
    }
    case "error":
      // An error Codex is about to retry is not the end of the turn, so it
      // must not be presented as one.
      if (params.willRetry) {
        outcome.notice = `${params.error?.message ?? "Codex reported an error."} Retrying…`;
      } else {
        outcome.streamError = params.error?.message ?? "Codex reported an error.";
        // The turn is over even if no `turn/completed` follows, and the session
        // store already stops counting the thread as working — without this the
        // transcript alone would go on claiming it still is. A `turn/completed`
        // that does arrive overwrites this with Codex's own verdict.
        finalizeRunningTurns(thread.turns, "failed");
      }
      break;
    // Advisories Codex expects the client to show. None of them stop the turn,
    // and all four carry the text in the same place.
    case "warning":
    case "guardianWarning":
    case "deprecationNotice":
    case "configWarning":
      outcome.notice = noticeText(params);
      break;
  }
  return outcome;
}

/** The first thing a failing hook actually said, when it set no status message. */
function hookOutput(run: { entries?: { kind?: string; text?: string }[] } | undefined): string {
  const entries = run?.entries ?? [];
  return entries.find((entry) => entry.kind === "error" || entry.kind === "stop")?.text ?? "";
}

/**
 * The user-facing line out of a warning-shaped notification. Codex spells the
 * field differently per notification (`message`, or `summary` + `details` on a
 * deprecation), so all the spellings are tried rather than guessing per method.
 */
function noticeText(params: {
  message?: string;
  summary?: string;
  warning?: string;
  details?: string;
  additionalDetails?: string;
}): string {
  const text = params?.message ?? params?.summary ?? params?.warning;
  const detail = params?.details ?? params?.additionalDetails;
  if (!text) return "Codex sent a notice.";
  return detail ? `${text} — ${detail}` : text;
}
