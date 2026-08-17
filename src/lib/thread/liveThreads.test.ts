import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ThreadDetail } from "$lib/types";

type Handler = (event: { method: string; params: any }) => void;

const mocks = vi.hoisted(() => ({
  invalidateThreadCache: vi.fn().mockResolvedValue(undefined),
  queueList: vi.fn().mockResolvedValue([]),
  activeTurns: { list: [] as string[] },
  handlers: [] as Handler[],
}));

vi.mock("$lib/services/api", () => ({
  invalidateThreadCache: mocks.invalidateThreadCache,
  queueList: mocks.queueList,
}));

vi.mock("$lib/services/codexEvents.svelte", () => ({
  activeTurns: mocks.activeTurns,
  setThreadHandler: (handler: Handler) => {
    mocks.handlers.push(handler);
    return () => {};
  },
}));

import { adoptLive, releaseLive, resetLiveThreads, trackLive } from "$lib/thread/liveThreads.svelte";

const emit = (method: string, params: any) => {
  for (const handler of mocks.handlers) handler({ method, params });
};

function detail(threadId: string): ThreadDetail {
  return { id: threadId, preview: "", cwd: "/repo", turns: [] };
}

const idle = {
  queued: [],
  queuedOptions: new Map(),
  compacting: false,
  streamError: null,
  subagentModelPolicy: null,
  subagentReasoningEffortPolicy: null,
};

/** Open a thread, start a turn on it (as the mounted view would), then leave. */
function leaveWorking(threadId: string) {
  const entry = trackLive(threadId, detail(threadId));
  entry.detail.turns.push({ id: `${threadId}-turn`, status: "inProgress", items: [] });
  mocks.activeTurns.list.push(threadId);
  releaseLive(threadId, idle);
  return entry;
}

beforeEach(() => {
  resetLiveThreads();
  mocks.activeTurns.list = [];
  mocks.invalidateThreadCache.mockClear();
  mocks.queueList.mockClear();
  mocks.queueList.mockResolvedValue([]);
});

describe("liveThreads", () => {
  it("keeps applying events to a working thread that was navigated away from", () => {
    leaveWorking("thread-a");
    trackLive("thread-b", detail("thread-b"));

    emit("item/agentMessage/delta", { threadId: "thread-a", turnId: "thread-a-turn", itemId: "m1", delta: "Hel" });
    emit("item/agentMessage/delta", { threadId: "thread-a", turnId: "thread-a-turn", itemId: "m1", delta: "lo" });

    const held = adoptLive("thread-a");
    expect(held?.detail.turns[0].status).toBe("inProgress");
    expect(held?.detail.turns[0].items[0].text).toBe("Hello");
  });

  it("leaves the open thread to its own view so deltas are not applied twice", () => {
    const entry = trackLive("thread-a", detail("thread-a"));
    entry.detail.turns.push({ id: "turn-1", status: "inProgress", items: [] });
    emit("item/agentMessage/delta", { threadId: "thread-a", turnId: "turn-1", itemId: "m1", delta: "hi" });
    expect(entry.detail.turns[0].items).toHaveLength(0);
  });

  it("drops an idle thread on release but retains one with work in flight", () => {
    trackLive("thread-idle", detail("thread-idle"));
    releaseLive("thread-idle", idle);
    expect(adoptLive("thread-idle")).toBeNull();

    leaveWorking("thread-busy");
    expect(adoptLive("thread-busy")).not.toBeNull();
  });

  it("retains a released thread that still has queued messages", () => {
    trackLive("thread-a", detail("thread-a"));
    releaseLive("thread-a", {
      ...idle,
      queued: [{ id: "q1", input: [{ type: "text", text: "next" }], clientUserMessageId: "c1" }],
    });
    expect(adoptLive("thread-a")?.queued).toHaveLength(1);
  });

  it("re-lists a retained thread's queue when the server says it changed", async () => {
    mocks.queueList.mockResolvedValue([
      { id: "q1", input: [{ type: "text", text: "next" }], clientUserMessageId: "c1" },
    ]);
    leaveWorking("thread-a");
    trackLive("thread-b", detail("thread-b"));

    emit("thread/queue/changed", { threadId: "thread-a" });
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(mocks.queueList).toHaveBeenCalledWith("thread-a");
    expect(adoptLive("thread-a")?.queued).toHaveLength(1);
  });

  it("does not let a re-list swallow a queue this window is holding alone", async () => {
    // On a Codex without the server queue the entry only exists here, so a
    // listing that does not mention it must merge, not replace.
    mocks.queueList.mockResolvedValue([
      { id: "q1", input: [{ type: "text", text: "server" }], clientUserMessageId: "c1" },
    ]);
    trackLive("thread-a", detail("thread-a"));
    mocks.activeTurns.list.push("thread-a");
    releaseLive("thread-a", {
      ...idle,
      queued: [{ id: "local-c2", input: [{ type: "text", text: "mine" }], clientUserMessageId: "c2" }],
    });
    trackLive("thread-b", detail("thread-b"));

    emit("thread/queue/changed", { threadId: "thread-a" });
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(adoptLive("thread-a")?.queued.map((entry) => entry.id)).toEqual(["q1", "local-c2"]);
  });

  it("invalidates the stale detail cache when a background turn ends", async () => {
    leaveWorking("thread-a");
    trackLive("thread-b", detail("thread-b"));

    // The event store clears the active turn before handlers see the event.
    mocks.activeTurns.list = mocks.activeTurns.list.filter((id) => id !== "thread-a");
    emit("turn/completed", { threadId: "thread-a", turn: { id: "thread-a-turn", status: "completed" } });

    expect(mocks.invalidateThreadCache).toHaveBeenCalledWith("thread-a");
    // Finished and unwatched: the next open reads it back fresh.
    expect(adoptLive("thread-a")).toBeNull();
  });

  it("forgets background documents when the session disconnects", () => {
    leaveWorking("thread-a");
    trackLive("thread-b", detail("thread-b"));

    emit("disconnected", null);

    expect(adoptLive("thread-a")).toBeNull();
  });
});
