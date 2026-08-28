import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ThreadDetail, TurnOptions } from "$lib/types";

type Handler = (event: { method: string; params: unknown }) => void;

const mocks = vi.hoisted(() => ({
  invalidateThreadCache: vi.fn().mockResolvedValue(undefined),
  readThread: vi.fn(),
  getThreadGoal: vi.fn().mockResolvedValue(null),
  listSubagents: vi.fn().mockResolvedValue([]),
  listAgentRuns: vi.fn().mockResolvedValue([]),
  startTurn: vi.fn(),
  interruptTurn: vi.fn().mockResolvedValue(undefined),
  queueAdd: vi.fn(),
  queueDelete: vi.fn().mockResolvedValue(true),
  queueList: vi.fn().mockResolvedValue([]),
  requestAutoName: vi.fn(),
  activeTurns: { list: [] as string[] },
  threadTokenUsage: {} as Record<string, unknown>,
  handlers: [] as Handler[],
}));

vi.mock("$lib/services/api", () => ({
  invalidateThreadCache: mocks.invalidateThreadCache,
  readThread: mocks.readThread,
  getThreadGoal: mocks.getThreadGoal,
  listSubagents: mocks.listSubagents,
  listAgentRuns: mocks.listAgentRuns,
  startTurn: mocks.startTurn,
  interruptTurn: mocks.interruptTurn,
  isQueueUnsupported: () => true,
  queueAdd: mocks.queueAdd,
  queueDelete: mocks.queueDelete,
  queueList: mocks.queueList,
  queueUpdate: vi.fn(),
  queueReorder: vi.fn(),
}));

vi.mock("$lib/services/codexEvents.svelte", () => ({
  activeTurns: mocks.activeTurns,
  threadTokenUsage: mocks.threadTokenUsage,
  setThreadHandler: (handler: Handler) => {
    mocks.handlers.push(handler);
    return () => {
      mocks.handlers = mocks.handlers.filter((candidate) => candidate !== handler);
    };
  },
}));

vi.mock("$lib/thread/autoName", () => ({ requestAutoName: mocks.requestAutoName }));
vi.mock("$lib/toaster", () => ({ toaster: {}, toastError: vi.fn() }));

import {
  attachSession,
  draftSession,
  openSession,
  peekSession,
  releaseSession,
  resetSessions,
} from "$lib/thread/sessions.svelte";

const emit = (method: string, params: unknown) => {
  for (const handler of [...mocks.handlers]) handler({ method, params });
};
const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

function detail(threadId: string, turns: ThreadDetail["turns"] = []): ThreadDetail {
  return { id: threadId, preview: "", cwd: "/repo", turns };
}

/** Open a thread, let it load, start a turn on it (as a view would), then leave. */
async function leaveWorking(threadId: string) {
  mocks.readThread.mockResolvedValueOnce(detail(threadId));
  const session = openSession(threadId);
  await settle();
  session.thread?.turns.push({ id: `${threadId}-turn`, status: "inProgress", items: [] });
  mocks.activeTurns.list.push(threadId);
  releaseSession(session);
  return session;
}

beforeEach(() => {
  resetSessions();
  mocks.handlers = [];
  mocks.activeTurns.list = [];
  mocks.invalidateThreadCache.mockClear();
  mocks.readThread.mockReset();
  mocks.queueList.mockReset();
  mocks.queueList.mockResolvedValue([]);
  mocks.queueAdd.mockReset();
  mocks.queueAdd.mockRejectedValue(new Error("codex-queue-unsupported"));
  mocks.startTurn.mockReset();
  mocks.startTurn.mockResolvedValue({ id: "turn-real", status: "inProgress" });
  mocks.requestAutoName.mockClear();
  mocks.getThreadGoal.mockReset();
  mocks.getThreadGoal.mockResolvedValue(null);
});

describe("sessions retention", () => {
  it("keeps applying events to a working thread that was navigated away from", async () => {
    const left = await leaveWorking("thread-a");
    emit("item/agentMessage/delta", { threadId: "thread-a", turnId: "thread-a-turn", itemId: "m1", delta: "Hel" });
    emit("item/agentMessage/delta", { threadId: "thread-a", turnId: "thread-a-turn", itemId: "m1", delta: "lo" });

    const back = openSession("thread-a");
    expect(back).toBe(left);
    expect(back.thread?.turns[0].items[0].text).toBe("Hello");
    // The retained transcript is used as-is; nothing is re-read behind it.
    expect(mocks.readThread).toHaveBeenCalledTimes(1);
  });

  it("drops an idle thread on release but retains one with work in flight", async () => {
    mocks.readThread.mockResolvedValueOnce(detail("thread-idle"));
    releaseSession(openSession("thread-idle"));
    await settle();
    expect(peekSession("thread-idle")).toBeNull();

    await leaveWorking("thread-busy");
    expect(peekSession("thread-busy")).not.toBeNull();
  });

  it("retains a released thread that still has queued messages", async () => {
    mocks.readThread.mockResolvedValueOnce(detail("thread-a"));
    const session = openSession("thread-a");
    await settle();
    session.thread?.turns.push({ id: "t", status: "inProgress", items: [] });
    await session.send([{ type: "text", text: "next" }]);
    session.thread?.turns.splice(0, 1);
    releaseSession(session);
    expect(peekSession("thread-a")?.queue.entries).toHaveLength(1);
  });

  it("invalidates the stale detail cache when a background turn ends, then lets go", async () => {
    await leaveWorking("thread-a");
    mocks.invalidateThreadCache.mockClear();
    // The event store clears the active turn before handlers see the event.
    mocks.activeTurns.list = [];
    emit("turn/completed", { threadId: "thread-a", turn: { id: "thread-a-turn", status: "completed" } });

    expect(mocks.invalidateThreadCache).toHaveBeenCalledWith("thread-a");
    // Finished and unwatched: the next open reads it back fresh.
    expect(peekSession("thread-a")).toBeNull();
  });

  it("keeps a finished thread while a view still shows it", async () => {
    mocks.readThread.mockResolvedValueOnce(detail("thread-a", [{ id: "t", status: "inProgress", items: [] }]));
    mocks.activeTurns.list = ["thread-a"];
    const session = openSession("thread-a");
    await settle();
    mocks.activeTurns.list = [];
    emit("turn/completed", { threadId: "thread-a", turn: { id: "t", status: "completed" } });
    expect(peekSession("thread-a")).toBe(session);
  });

  it("forgets background sessions when the stream disconnects, and tells the shown one", async () => {
    await leaveWorking("thread-a");
    mocks.readThread.mockResolvedValueOnce(detail("thread-b", [{ id: "t", status: "inProgress", items: [] }]));
    const shown = openSession("thread-b");
    await settle();

    emit("disconnected", null);

    expect(peekSession("thread-a")).toBeNull();
    expect(shown.streamError).toBe("Lost connection to Codex.");
    expect(shown.thread?.turns[0].status).toBe("interrupted");
  });

  it("drops the stale detail cache before reading a thread that is mid-turn", async () => {
    mocks.activeTurns.list = ["thread-a"];
    mocks.readThread.mockResolvedValueOnce(detail("thread-a"));
    openSession("thread-a");
    await settle();
    expect(mocks.invalidateThreadCache).toHaveBeenCalledWith("thread-a");
  });

  it("shows a turn a dead session left running as interrupted", async () => {
    mocks.readThread.mockResolvedValueOnce(detail("thread-a", [{ id: "t", status: "inProgress", items: [] }]));
    const session = openSession("thread-a");
    await settle();
    expect(session.thread?.turns[0].status).toBe("interrupted");
    expect(session.loading).toBe(false);
  });
});

describe("session events while no view is mounted", () => {
  it("re-lists the queue when the server says it changed, merging what is held here", async () => {
    const session = await leaveWorking("thread-a");
    await session.send([{ type: "text", text: "mine" }]);
    await settle();
    mocks.queueList.mockResolvedValue([
      { id: "q1", input: [{ type: "text", text: "server" }], clientUserMessageId: "c1" },
    ]);

    emit("thread/queue/changed", { threadId: "thread-a" });
    await settle();

    expect(session.queue.entries.map((entry) => entry.input[0].text)).toEqual(["server", "mine"]);
  });

  it("drains a queued message when the background turn ends", async () => {
    const session = await leaveWorking("thread-a");
    await session.send([{ type: "text", text: "next" }]);
    expect(mocks.startTurn).not.toHaveBeenCalled();

    mocks.activeTurns.list = [];
    emit("turn/completed", { threadId: "thread-a", turn: { id: "thread-a-turn", status: "completed" } });
    await settle();

    expect(mocks.startTurn).toHaveBeenCalledWith("thread-a", [{ type: "text", text: "next" }], undefined);
    expect(session.queue.entries).toHaveLength(0);
    // Its new turn keeps it retained.
    expect(peekSession("thread-a")).toBe(session);
  });

  it("keeps goal, usage, settings and notices current", async () => {
    const session = await leaveWorking("thread-a");
    emit("thread/goal/updated", { threadId: "thread-a", goal: { objective: "Ship", status: "active" } });
    emit("thread/tokenUsage/updated", { threadId: "thread-a", tokenUsage: { total: { totalTokens: 12 } } });
    emit("thread/settings/updated", {
      threadId: "thread-a",
      threadSettings: { subagentModelPolicy: { type: "inherit" } },
    });
    emit("warning", { threadId: "thread-a", message: "Careful" });
    expect(session.goal?.objective).toBe("Ship");
    expect(session.tokenUsage).toEqual({ total: { totalTokens: 12 } });
    expect(session.subagentModelPolicy).toEqual({ type: "inherit" });
    expect(session.notice).toBe("Careful");
    emit("turn/started", { threadId: "thread-a", turn: { id: "thread-a-turn" } });
    expect(session.notice).toBeNull();
  });

  it("invalidates the cache when another client reverts the thread", async () => {
    await leaveWorking("thread-a");
    mocks.invalidateThreadCache.mockClear();
    emit("thread/reverted", { threadId: "thread-a" });
    expect(mocks.invalidateThreadCache).toHaveBeenCalledWith("thread-a");
  });

  it("names the thread off its opening exchange when that turn ends", async () => {
    await leaveWorking("thread-a");
    mocks.activeTurns.list = [];
    emit("turn/completed", { threadId: "thread-a", turn: { id: "thread-a-turn", status: "completed" } });
    expect(mocks.requestAutoName).toHaveBeenCalledWith("thread-a", "reply");
  });

  it("ignores events for threads it does not hold", async () => {
    await leaveWorking("thread-a");
    expect(() =>
      emit("item/agentMessage/delta", { threadId: "thread-x", turnId: "t", itemId: "m", delta: "?" }),
    ).not.toThrow();
  });
});

describe("session turns", () => {
  it("starts a turn with an optimistic bubble and adopts the real id", async () => {
    mocks.readThread.mockResolvedValueOnce(detail("thread-a"));
    const session = openSession("thread-a");
    await settle();
    const sending = session.send([{ type: "text", text: "Go" }], { resolvedModel: "m" } as TurnOptions);
    expect(session.activeTurn?.id).toMatch(/^local-/);
    expect(session.activeTurn?.model).toBe("m");
    expect(await sending).toBe(true);
    expect(session.thread?.turns[0].id).toBe("turn-real");
  });

  it("removes the bubble and reports a turn that failed to start", async () => {
    mocks.readThread.mockResolvedValueOnce(detail("thread-a"));
    mocks.startTurn.mockRejectedValue(new Error("boom"));
    const session = openSession("thread-a");
    await settle();
    expect(await session.send([{ type: "text", text: "Go" }])).toBe(false);
    expect(session.thread?.turns).toHaveLength(0);
    expect(session.streamError).toBe("boom");
  });

  it("queues instead of sending while a turn runs", async () => {
    const session = await leaveWorking("thread-a");
    expect(await session.send([{ type: "text", text: "later" }])).toBe(true);
    expect(mocks.startTurn).not.toHaveBeenCalled();
    expect(session.queue.entries).toHaveLength(1);
  });

  it("waits for the real turn id before interrupting an optimistic turn", async () => {
    mocks.readThread.mockResolvedValueOnce(detail("thread-a"));
    let resolveStart: (turn: unknown) => void = () => {};
    mocks.startTurn.mockImplementation(() => new Promise((resolve) => (resolveStart = resolve)));
    const session = openSession("thread-a");
    await settle();
    void session.send([{ type: "text", text: "Go" }]);
    const interrupting = session.interrupt();
    expect(mocks.interruptTurn).not.toHaveBeenCalled();
    resolveStart({ id: "turn-real", status: "inProgress" });
    await interrupting;
    expect(mocks.interruptTurn).toHaveBeenCalledWith("thread-a", "turn-real");
  });

  it("a draft joins the registry under the id it is given", () => {
    const session = draftSession("/repo");
    expect(session.thread?.cwd).toBe("/repo");
    attachSession(session, "thread-new");
    expect(session.id).toBe("thread-new");
    expect(session.thread?.id).toBe("thread-new");
    expect(openSession("thread-new")).toBe(session);
    expect(mocks.readThread).not.toHaveBeenCalled();
  });
});
