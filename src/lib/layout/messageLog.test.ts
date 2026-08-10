import { describe, expect, it } from "vitest";
import {
  appendMessage,
  describeMessage,
  filterMessages,
  formatPayload,
  MESSAGE_LOG_LIMIT,
  messagesToText,
} from "$lib/layout/messageLog";
import type { WireMessage } from "$lib/types";

function message(overrides: Partial<WireMessage> = {}): WireMessage {
  return {
    seq: 0,
    at: 1_753_700_000_000,
    direction: "out",
    kind: "request",
    method: "thread/sendMessage",
    id: 1,
    threadId: "thread-1",
    payload: { text: "hello" },
    truncated: false,
    ...overrides,
  };
}

describe("describeMessage", () => {
  it("prefers the method, falls back to the answered id", () => {
    expect(describeMessage(message())).toBe("thread/sendMessage");
    expect(describeMessage(message({ method: null, id: 12, kind: "response" }))).toBe("#12");
    expect(describeMessage(message({ method: null, id: null, kind: "notification" }))).toBe("notification");
  });
});

describe("filterMessages", () => {
  const messages = [
    message({ seq: 0, direction: "out", method: "thread/sendMessage", threadId: "thread-1" }),
    message({ seq: 1, direction: "in", kind: "notification", method: "turn/started", threadId: "thread-2" }),
    message({ seq: 2, direction: "in", kind: "response", method: null, id: 1, threadId: null }),
  ];

  it("returns everything with an empty filter", () => {
    expect(filterMessages(messages, {})).toHaveLength(3);
  });

  it("narrows by direction", () => {
    expect(filterMessages(messages, { direction: "out" }).map((entry) => entry.seq)).toEqual([0]);
  });

  it("keeps thread-less messages when filtering by thread so pairs stay intact", () => {
    // The response (seq 2) carries no threadId but answers the request in thread-1.
    expect(filterMessages(messages, { threadId: "thread-1" }).map((entry) => entry.seq)).toEqual([0, 2]);
  });

  it("matches the query against the payload as well as the method", () => {
    expect(filterMessages(messages, { query: "TURN/STARTED" }).map((entry) => entry.seq)).toEqual([1]);
    expect(filterMessages(messages, { query: "hello" }).map((entry) => entry.seq)).toEqual([0, 1, 2]);
    expect(filterMessages(messages, { query: "nothing-matches" })).toHaveLength(0);
  });
});

describe("appendMessage", () => {
  it("drops the oldest once the buffer is full", () => {
    let messages: WireMessage[] = [];
    for (let seq = 0; seq < MESSAGE_LOG_LIMIT + 3; seq++) {
      messages = appendMessage(messages, message({ seq }));
    }
    expect(messages).toHaveLength(MESSAGE_LOG_LIMIT);
    expect(messages[0].seq).toBe(3);
  });
});

describe("formatPayload", () => {
  it("pretty-prints and survives values JSON cannot handle", () => {
    expect(formatPayload({ a: 1 })).toBe('{\n  "a": 1\n}');
    const circular: Record<string, unknown> = {};
    circular.self = circular;
    expect(formatPayload(circular)).toContain("object");
  });
});

describe("messagesToText", () => {
  it("renders direction arrows and payloads", () => {
    const text = messagesToText([message(), message({ direction: "in", kind: "response", method: null })]);
    expect(text).toContain("→ request thread/sendMessage");
    expect(text).toContain("← response #1");
    expect(text).toContain('"text": "hello"');
  });
});
