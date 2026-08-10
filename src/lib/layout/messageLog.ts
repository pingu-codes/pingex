/** Pure helpers behind the Advanced → message log viewer, kept out of the
 * component so the filtering and formatting are unit-testable. */

import type { WireMessage } from "$lib/types";

export type DirectionFilter = "all" | "out" | "in";

export interface MessageFilter {
  /** Free text matched against the method, kind, id and payload. */
  query?: string;
  direction?: DirectionFilter;
  /** When set, only messages belonging to this thread (plus messages with no
   * thread of their own, which are session-wide) are kept. */
  threadId?: string | null;
}

/** How many messages the viewer keeps in memory, mirroring the backend cap. */
export const MESSAGE_LOG_LIMIT = 500;

/** A short label for the message: the method if it has one, else the id it
 * answers. Responses carry no method, so `#12` is all there is to show. */
export function describeMessage(message: WireMessage): string {
  if (message.method) return message.method;
  if (message.id !== null) return `#${message.id}`;
  return message.kind;
}

/** Pretty-printed payload for the expanded row. */
export function formatPayload(payload: unknown): string {
  try {
    return JSON.stringify(payload, null, 2) ?? "null";
  } catch {
    return String(payload);
  }
}

/** `HH:MM:SS.mmm` in local time — the only part of the stamp that helps when
 * reading a burst of messages from one turn. */
export function formatTime(at: number): string {
  const date = new Date(at);
  const pad = (value: number, width = 2) => String(value).padStart(width, "0");
  return `${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}.${pad(date.getMilliseconds(), 3)}`;
}

function matchesQuery(message: WireMessage, query: string): boolean {
  const haystack = [
    message.method ?? "",
    message.kind,
    message.direction,
    message.id === null ? "" : `#${message.id}`,
    message.threadId ?? "",
    formatPayload(message.payload),
  ]
    .join("\n")
    .toLowerCase();
  return haystack.includes(query);
}

export function filterMessages(messages: WireMessage[], filter: MessageFilter): WireMessage[] {
  const query = (filter.query ?? "").trim().toLowerCase();
  const direction = filter.direction ?? "all";
  const threadId = filter.threadId ?? null;
  return messages.filter((message) => {
    if (direction !== "all" && message.direction !== direction) return false;
    // A message with no thread of its own (a response, a session-wide
    // notification) stays visible: hiding it would break request/response pairs.
    if (threadId && message.threadId && message.threadId !== threadId) return false;
    if (query && !matchesQuery(message, query)) return false;
    return true;
  });
}

/** Append a message, dropping the oldest once the buffer is full. */
export function appendMessage(messages: WireMessage[], message: WireMessage): WireMessage[] {
  const next = [...messages, message];
  return next.length > MESSAGE_LOG_LIMIT ? next.slice(next.length - MESSAGE_LOG_LIMIT) : next;
}

/** The whole log as text, for the copy-to-clipboard button. */
export function messagesToText(messages: WireMessage[]): string {
  return messages
    .map(
      (message) =>
        `${formatTime(message.at)} ${message.direction === "out" ? "→" : "←"} ${message.kind} ${describeMessage(message)}\n${formatPayload(message.payload)}`,
    )
    .join("\n\n");
}
