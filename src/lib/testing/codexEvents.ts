/**
 * Loosely typed Codex events for tests. The reducers tolerate a partial
 * payload — a `turn/completed` with only an id, an item with only a type —
 * while `CodexEvent` says what Codex really sends. Tests write the partial
 * form and go through here rather than casting at every call site.
 */
import type { CodexEvent, CodexServerRequestEvent } from "$lib/types";

export interface FakeEvent {
  method: string;
  params?: unknown;
}

export function fakeEvent(event: FakeEvent): CodexEvent {
  return event as CodexEvent;
}

export function fakeServerRequest(payload: FakeEvent & { requestId: number }): CodexServerRequestEvent {
  return payload as CodexServerRequestEvent;
}
