import { describe, expect, it } from "vitest";
import { applyQuickEvent as applyTyped, emptyQuickResponse, type QuickResponse } from "$lib/quick/quickChat";
import { type FakeEvent, fakeEvent } from "$lib/testing/codexEvents";

const applyQuickEvent = (state: QuickResponse, event: FakeEvent) => applyTyped(state, fakeEvent(event));

describe("applyQuickEvent", () => {
  const threadId = "thread-1";

  it("ignores events for other threads", () => {
    const state = emptyQuickResponse(threadId);
    const next = applyQuickEvent(state, {
      method: "item/agentMessage/delta",
      params: { threadId: "other", delta: "hi" },
    });
    expect(next).toBe(state);
    expect(next.text).toBe("");
  });

  it("accumulates streamed deltas and completes on turn end", () => {
    let state = emptyQuickResponse(threadId);
    state = applyQuickEvent(state, { method: "turn/started", params: { threadId, turn: { id: "t1" } } });
    state = applyQuickEvent(state, { method: "item/agentMessage/delta", params: { threadId, delta: "Hel" } });
    state = applyQuickEvent(state, { method: "item/agentMessage/delta", params: { threadId, delta: "lo" } });
    expect(state.text).toBe("Hello");
    expect(state.streaming).toBe(true);

    state = applyQuickEvent(state, { method: "turn/completed", params: { threadId, turn: { id: "t1" } } });
    expect(state.streaming).toBe(false);
    expect(state.text).toBe("Hello");
  });

  it("adopts the final text from a completed agent message item", () => {
    let state = emptyQuickResponse(threadId);
    state = applyQuickEvent(state, {
      method: "item/completed",
      params: { threadId, item: { type: "agentMessage", id: "m1", text: "Full answer" } },
    });
    expect(state.text).toBe("Full answer");
  });

  it("captures stream errors", () => {
    const state = applyQuickEvent(emptyQuickResponse(threadId), {
      method: "error",
      params: { threadId, error: { message: "boom" } },
    });
    expect(state.error).toBe("boom");
    expect(state.streaming).toBe(false);
  });
});
