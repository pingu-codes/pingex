import { describe, expect, it } from "vitest";
import { acceleratorFromEvent, applyQuickEvent, emptyQuickResponse } from "$lib/quick/quickChat";

function keydown(init: KeyboardEventInit): KeyboardEvent {
  return new KeyboardEvent("keydown", init);
}

describe("acceleratorFromEvent", () => {
  it("maps meta to CmdOrCtrl and uppercases letters", () => {
    expect(acceleratorFromEvent(keydown({ key: "k", metaKey: true }))).toBe("CmdOrCtrl+K");
  });

  it("orders modifiers and names the space key", () => {
    expect(acceleratorFromEvent(keydown({ key: " ", metaKey: true, shiftKey: true }))).toBe("CmdOrCtrl+Shift+Space");
  });

  it("uses Control only when meta is absent", () => {
    expect(acceleratorFromEvent(keydown({ key: "j", ctrlKey: true, altKey: true }))).toBe("Control+Alt+J");
  });

  it("passes through function and arrow keys", () => {
    expect(acceleratorFromEvent(keydown({ key: "F5", altKey: true }))).toBe("Alt+F5");
    expect(acceleratorFromEvent(keydown({ key: "ArrowUp", metaKey: true }))).toBe("CmdOrCtrl+Up");
  });

  it("returns null while only modifiers are held", () => {
    expect(acceleratorFromEvent(keydown({ key: "Meta", metaKey: true }))).toBeNull();
    expect(acceleratorFromEvent(keydown({ key: "Shift", shiftKey: true }))).toBeNull();
  });
});

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
