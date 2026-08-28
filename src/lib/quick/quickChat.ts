import type { CodexEvent } from "$lib/services/codexEvents.svelte";
import { threadIdOf } from "$lib/services/turnLifecycle";

/**
 * Non-printing keys that only make sense as a whole-name accelerator token.
 * Everything else with a single-character `key` is uppercased as-is.
 */
const NAMED_KEYS: Record<string, string> = {
  " ": "Space",
  Spacebar: "Space",
  ArrowUp: "Up",
  ArrowDown: "Down",
  ArrowLeft: "Left",
  ArrowRight: "Right",
  Enter: "Enter",
  Return: "Enter",
  Escape: "Escape",
  Tab: "Tab",
  Backspace: "Backspace",
  Delete: "Delete",
};

const MODIFIER_KEYS = new Set(["Control", "Shift", "Alt", "Meta", "OS", "AltGraph"]);

function normalizeKey(event: KeyboardEvent): string | null {
  const { key } = event;
  if (MODIFIER_KEYS.has(key)) return null;
  if (key in NAMED_KEYS) return NAMED_KEYS[key];
  // Function keys (F1–F24) and other multi-char named keys pass through.
  if (/^F\d{1,2}$/.test(key)) return key;
  if (key.length === 1) return key.toUpperCase();
  return null;
}

/**
 * Build a Tauri accelerator string (e.g. `CmdOrCtrl+Shift+K`) from a keydown.
 * Returns `null` while only modifiers are held so a recorder can wait for the
 * final key. `metaKey` maps to `CmdOrCtrl` so the same binding works on macOS
 * (Cmd) and elsewhere (Ctrl).
 */
export function acceleratorFromEvent(event: KeyboardEvent): string | null {
  const key = normalizeKey(event);
  if (!key) return null;
  const parts: string[] = [];
  if (event.metaKey) parts.push("CmdOrCtrl");
  if (event.ctrlKey && !event.metaKey) parts.push("Control");
  if (event.altKey) parts.push("Alt");
  if (event.shiftKey) parts.push("Shift");
  parts.push(key);
  return parts.join("+");
}

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
