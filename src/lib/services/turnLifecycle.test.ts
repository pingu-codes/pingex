import { describe, expect, it } from "vitest";
import { isRetryableError, reviewTransition, threadIdOf, turnEnd } from "$lib/services/turnLifecycle";
import { fakeEvent } from "$lib/testing/codexEvents";

describe("threadIdOf", () => {
  it("reads a non-empty string thread id and nothing else", () => {
    expect(threadIdOf(fakeEvent({ method: "turn/started", params: { threadId: "t" } }))).toBe("t");
    expect(threadIdOf(fakeEvent({ method: "turn/started", params: { threadId: "" } }))).toBeNull();
    expect(threadIdOf(fakeEvent({ method: "turn/started", params: { threadId: 3 } }))).toBeNull();
    expect(threadIdOf(fakeEvent({ method: "account/rateLimits/updated", params: {} }))).toBeNull();
    expect(threadIdOf(fakeEvent({ method: "disconnected", params: null }))).toBeNull();
  });
});

describe("reviewTransition", () => {
  it("sees review mode entered on either item event and exited only on completion", () => {
    const entered = { threadId: "t", item: { type: "enteredReviewMode", id: "i" } };
    const exited = { threadId: "t", item: { type: "exitedReviewMode", id: "i" } };
    expect(reviewTransition(fakeEvent({ method: "item/started", params: entered }))).toBe("entered");
    expect(reviewTransition(fakeEvent({ method: "item/completed", params: entered }))).toBe("entered");
    expect(reviewTransition(fakeEvent({ method: "item/started", params: exited }))).toBeNull();
    expect(reviewTransition(fakeEvent({ method: "item/completed", params: exited }))).toBe("exited");
    expect(
      reviewTransition(
        fakeEvent({ method: "item/completed", params: { threadId: "t", item: { type: "agentMessage" } } }),
      ),
    ).toBeNull();
    expect(reviewTransition(fakeEvent({ method: "turn/completed", params: { threadId: "t" } }))).toBeNull();
  });
});

describe("turnEnd", () => {
  it("ends on turn/completed, a final error, or leaving review mode", () => {
    expect(turnEnd(fakeEvent({ method: "turn/completed", params: { threadId: "t", turn: { id: "u" } } }))).toEqual({
      threadId: "t",
      outcome: "completed",
    });
    expect(turnEnd(fakeEvent({ method: "error", params: { threadId: "t", willRetry: false } }))).toEqual({
      threadId: "t",
      outcome: "failed",
    });
    expect(turnEnd(fakeEvent({ method: "error", params: { threadId: "t" } }))).toEqual({
      threadId: "t",
      outcome: "failed",
    });
    expect(
      turnEnd(
        fakeEvent({ method: "item/completed", params: { threadId: "t", item: { type: "exitedReviewMode", id: "i" } } }),
      ),
    ).toEqual({ threadId: "t", outcome: "reviewExited" });
  });

  it("keeps the turn running through an error Codex will retry", () => {
    const retry = fakeEvent({ method: "error", params: { threadId: "t", willRetry: true } });
    expect(isRetryableError(retry)).toBe(true);
    expect(turnEnd(retry)).toBeNull();
  });

  it("does not end anything without a thread to end it for", () => {
    expect(turnEnd(fakeEvent({ method: "turn/completed", params: { turn: { id: "u" } } }))).toBeNull();
    expect(turnEnd(fakeEvent({ method: "turn/started", params: { threadId: "t" } }))).toBeNull();
    expect(turnEnd(fakeEvent({ method: "disconnected", params: null }))).toBeNull();
  });
});
