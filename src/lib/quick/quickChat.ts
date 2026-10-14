import type { CodexEvent } from "$lib/services/codexEvents.svelte";
import { threadIdOf } from "$lib/services/turnLifecycle";

/** Streaming state for the quick window's single short response. */
export interface QuickResponse {
  threadId: string | null;
  text: string;
  streaming: boolean;
  error: string | null;
}

export function emptyQuickResponse(threadId: string | null = null): QuickResponse {
  return { threadId, text: "", streaming: Boolean(threadId), error: null };
}

/**
 * Reduce a Codex event into the quick response. Only events for the active
 * thread mutate state; the reducer is pure so it can be unit-tested without a
 * live session. Reuses the same event shapes as `applyThreadEvent`.
 */
export function applyQuickEvent(state: QuickResponse, event: CodexEvent): QuickResponse {
  if (!state.threadId || threadIdOf(event) !== state.threadId) return state;
  const { method, params } = event;
  switch (method) {
    case "turn/started":
      return { ...state, streaming: true, error: null };
    case "item/agentMessage/delta":
      return { ...state, text: state.text + (params.delta ?? ""), streaming: true };
    case "item/started":
    case "item/completed":
      if (params.item?.type === "agentMessage" && typeof params.item.text === "string") {
        return { ...state, text: params.item.text, streaming: method !== "item/completed" ? true : state.streaming };
      }
      return state;
    case "turn/completed":
      return { ...state, streaming: false };
    case "error":
      return { ...state, streaming: false, error: params.error?.message ?? "Codex reported an error." };
    default:
      return state;
  }
}
