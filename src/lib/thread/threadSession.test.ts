import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ThreadDetail, TurnOptions } from "$lib/types";

type Handler = (event: { method: string; params: unknown }) => void;

const mocks = vi.hoisted(() => ({
  invalidateThreadCache: vi.fn().mockResolvedValue(undefined),
  readThread: vi.fn(),
  getThreadGoal: vi.fn().mockResolvedValue(null),
  listSubagents: vi.fn().mockResolvedValue([]),
  startTurn: vi.fn(),
  startReview: vi.fn(),
  compactThread: vi.fn(),
  revertThread: vi.fn(),
  updateTurnSettings: vi.fn().mockResolvedValue("applied"),
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
  readThread: mocks.readThread,
  getThreadGoal: mocks.getThreadGoal,
  listSubagents: mocks.listSubagents,
  startTurn: mocks.startTurn,
  startReview: mocks.startReview,
  revertThread: mocks.revertThread,
  isRevertUnsupported: (cause: unknown) => String(cause).includes("unsupported"),
  updateTurnSettings: mocks.updateTurnSettings,
  isTurnSettingsUnsupported: (cause: unknown) => String(cause).includes("unsupported"),
  isQueueUnsupported: () => true,
  queueAdd: mocks.queueAdd,
  queueDelete: mocks.queueDelete,
  queueList: mocks.queueList,
  queueUpdate: vi.fn(),
}));
vi.mock("$lib/services/homeRouting", async (importOriginal) => {
  const actual = await importOriginal<typeof import("$lib/services/homeRouting")>();
  return {
    ...actual,
    commands: {
      ...actual.commands,
      invalidateThreadCache: mocks.invalidateThreadCache,
      compactThread: mocks.compactThread,
      interruptTurn: mocks.interruptTurn,
      queueReorder: vi.fn(),
    },
  };
});

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
  mocks.startReview.mockReset().mockResolvedValue({ id: "review", status: "inProgress", items: [] });
  mocks.compactThread.mockReset().mockResolvedValue(undefined);
  mocks.revertThread.mockReset().mockResolvedValue(undefined);
  mocks.interruptTurn.mockClear();
  mocks.startTurn.mockReset();
  mocks.updateTurnSettings.mockReset().mockResolvedValue("applied");
  mocks.startTurn.mockResolvedValue({ id: "turn-real", status: "inProgress" });
  mocks.requestAutoName.mockClear();
  mocks.getThreadGoal.mockReset();
  mocks.getThreadGoal.mockResolvedValue(null);
});

describe("events during the initial load", () => {
  function deferredRead() {
    let resolve!: (value: ThreadDetail) => void;
    mocks.readThread.mockReturnValueOnce(new Promise<ThreadDetail>((done) => (resolve = done)));
    return (value: ThreadDetail) => resolve(value);
  }

  it("replays stream events that arrived while the transcript was being read", async () => {
    mocks.activeTurns.list = ["thread-l"];
    const resolve = deferredRead();
    const session = openSession("thread-l");
    await settle();
    emit("item/agentMessage/delta", { threadId: "thread-l", turnId: "t1", itemId: "m1", delta: "Hi" });
    // Already complete in the snapshot: appending would duplicate its tail.
    emit("item/agentMessage/delta", { threadId: "thread-l", turnId: "t1", itemId: "m0", delta: "ne" });

    resolve(
      detail("thread-l", [
        { id: "t1", status: "inProgress", items: [{ id: "m0", type: "agentMessage", text: "done" }] },
      ] as ThreadDetail["turns"]),
    );
    await settle();

    const items = session.thread?.turns[0].items ?? [];
    expect(items.find((item) => item.id === "m0")?.text).toBe("done");
    expect(items.find((item) => item.id === "m1")?.text).toBe("Hi");
  });

  it("keeps a turn that completed during the read as completed, not interrupted", async () => {
    mocks.activeTurns.list = ["thread-c"];
    const resolve = deferredRead();
    const session = openSession("thread-c");
    await settle();
    emit("turn/completed", { threadId: "thread-c", turn: { id: "t1", status: "completed" } });
    mocks.activeTurns.list = [];

    resolve(detail("thread-c", [{ id: "t1", status: "inProgress", items: [] }] as ThreadDetail["turns"]));
    await settle();

    expect(session.thread?.turns[0].status).toBe("completed");
  });
});

describe("sessions retention", () => {
  it("updates running speed without changing the thread default", async () => {
    const session = await leaveWorking("fast");
    session.thread!.speedTier = "default";
    await session.updateLiveSettings({ speedTier: "priority" });
    expect(session.runningSpeedTier).toBe("priority");
    expect(session.thread!.speedTier).toBe("default");
  });

  it("keeps confirmed speed on refusal and reports next-turn application", async () => {
    const session = await leaveWorking("legacy");
    session.runningSpeedTier = "priority";
    mocks.updateTurnSettings.mockRejectedValueOnce(new Error("codex-live-speed-unsupported"));
    await session.updateLiveSettings({ speedTier: "default" });
    expect(session.runningSpeedTier).toBe("priority");
    expect(session.notice).toBe("Fast mode will change next turn.");
  });

  it("serializes rapid toggles and ignores confirmation after the turn finishes", async () => {
    const session = await leaveWorking("rapid");
    let resolve!: (status: string) => void;
    mocks.updateTurnSettings.mockImplementationOnce(
      () =>
        new Promise<string>((done) => {
          resolve = done;
        }),
    );
    const first = session.updateLiveSettings({ speedTier: "priority" });
    const second = session.updateLiveSettings({ speedTier: "default" });
    await settle();
    expect(mocks.updateTurnSettings).toHaveBeenCalledTimes(1);
    resolve("applied");
    await Promise.all([first, second]);
    expect(session.runningSpeedTier).toBe("default");
    expect(mocks.updateTurnSettings).toHaveBeenCalledTimes(2);
    mocks.updateTurnSettings.mockImplementationOnce(
      () =>
        new Promise<string>((done) => {
          resolve = done;
        }),
    );
    const ending = session.updateLiveSettings({ speedTier: "priority" });
    await settle();
    session.thread!.turns[0].status = "completed";
    resolve("applied");
    await ending;
    expect(session.runningSpeedTier).toBe("default");
  });
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

  it("goes idle when a goal's last turn completes beside an orphaned one", async () => {
    mocks.readThread.mockResolvedValueOnce(detail("thread-a"));
    const session = openSession("thread-a");
    await settle();
    mocks.activeTurns.list = ["thread-a"];
    emit("turn/started", { threadId: "thread-a", turn: { id: "goal-1" } });
    emit("item/completed", {
      threadId: "thread-a",
      turnId: "goal-1",
      item: { type: "agentMessage", id: "m1", text: "working" },
    });
    // Items under a turn nothing announced open a running turn of their own.
    emit("item/completed", {
      threadId: "thread-a",
      turnId: "stray",
      item: { type: "agentMessage", id: "m2", text: "GOAL-DONE-OK" },
    });
    mocks.activeTurns.list = [];
    emit("thread/goal/updated", { threadId: "thread-a", goal: { objective: "Ship", status: "complete" } });
    emit("turn/completed", { threadId: "thread-a", turn: { id: "goal-1", status: "completed" } });

    expect(session.activeTurn).toBeNull();
    releaseSession(session);
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

  it("names the thread off its opening exchange when that turn ends, and only then", async () => {
    await leaveWorking("thread-a");
    expect(mocks.requestAutoName).not.toHaveBeenCalled();
    mocks.activeTurns.list = [];
    emit("turn/completed", { threadId: "thread-a", turn: { id: "thread-a-turn", status: "completed" } });
    expect(mocks.requestAutoName).toHaveBeenCalledWith("thread-a", "reply");
  });

  it("leaves an established thread's title alone as later turns complete", async () => {
    mocks.readThread.mockResolvedValueOnce(
      detail("thread-b", [
        { id: "turn-1", status: "completed", items: [] },
        { id: "turn-2", status: "inProgress", items: [] },
      ]),
    );
    mocks.activeTurns.list = ["thread-b"];
    openSession("thread-b");
    await settle();
    emit("turn/completed", { threadId: "thread-b", turn: { id: "turn-2", status: "completed" } });
    expect(mocks.requestAutoName).not.toHaveBeenCalled();
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

describe("session compaction", () => {
  it("queues a send while compacting and releases only when the compaction lands", async () => {
    mocks.readThread.mockResolvedValueOnce(detail("thread-a"));
    const session = openSession("thread-a");
    await settle();
    await session.compact();

    // Before Codex has even announced the compaction turn, a send must wait.
    expect(await session.send([{ type: "text", text: "meanwhile" }])).toBe(true);
    expect(mocks.startTurn).not.toHaveBeenCalled();
    expect(session.queue.entries).toHaveLength(1);
    expect(session.working()).toBe(true);

    emit("turn/started", { threadId: "thread-a", turn: { id: "compact-turn", status: "inProgress" } });
    // A different turn ending (a race) does not release the meter.
    emit("turn/completed", { threadId: "thread-a", turn: { id: "other-turn", status: "completed" } });
    await settle();
    expect(session.compacting).toBe(true);
    expect(mocks.startTurn).not.toHaveBeenCalled();

    emit("thread/compacted", { threadId: "thread-a", turnId: "compact-turn" });
    emit("turn/completed", { threadId: "thread-a", turn: { id: "compact-turn", status: "completed" } });
    await settle();
    expect(session.compacting).toBe(false);
    expect(mocks.startTurn).toHaveBeenCalledWith("thread-a", [{ type: "text", text: "meanwhile" }], undefined);
  });

  it("releases the meter when the stream drops", async () => {
    mocks.readThread.mockResolvedValueOnce(detail("thread-a"));
    const session = openSession("thread-a");
    await settle();
    await session.compact();
    session.disconnected();
    expect(session.compacting).toBe(false);
  });
});

describe("session turn options", () => {
  it("runs a send that has no options of its own with the composer's", async () => {
    mocks.readThread.mockResolvedValueOnce(detail("thread-a"));
    const session = openSession("thread-a");
    await settle();
    const options = { collaborationMode: { mode: "plan" } } as TurnOptions;
    session.turnOptions = () => options;

    expect(await session.send([{ type: "text", text: "Go" }])).toBe(true);
    expect(mocks.startTurn).toHaveBeenCalledWith("thread-a", [{ type: "text", text: "Go" }], options);
  });

  it("keeps the options a send brings with it", async () => {
    mocks.readThread.mockResolvedValueOnce(detail("thread-a"));
    const session = openSession("thread-a");
    await settle();
    session.turnOptions = () => ({ model: "other" });
    const own = { model: "mine" } as TurnOptions;

    await session.send([{ type: "text", text: "Go" }], own);
    expect(mocks.startTurn).toHaveBeenCalledWith("thread-a", expect.anything(), own);
  });

  it("notes a mode the harness changed by itself", async () => {
    mocks.readThread.mockResolvedValueOnce(detail("thread-a"));
    const session = openSession("thread-a");
    await settle();
    emit("thread/collaborationMode/changed", { threadId: "thread-a", mode: "default" });
    expect(session.collaborationMode).toBe("default");
    emit("thread/permissionMode/changed", { threadId: "thread-a", mode: "bypassPermissions" });
    expect(session.permissionMode).toBe("bypassPermissions");
  });
});

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (cause: Error) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}

async function idleSession() {
  mocks.readThread.mockResolvedValueOnce(
    detail("thread-a", [
      { id: "old", status: "completed", items: [] },
      { id: "last", status: "completed", items: [] },
    ]),
  );
  const session = openSession("thread-a");
  await settle();
  return session;
}

describe("session operation lifetime", () => {
  it("retains a pending review and stops its real turn before output", async () => {
    const session = await idleSession();
    const request = deferred<{ id: string; status: string; items: [] }>();
    mocks.startReview.mockReturnValueOnce(request.promise);
    const reviewing = session.review({ type: "uncommittedChanges" });
    const stopping = session.interrupt();
    await session.review({ type: "uncommittedChanges" });
    await session.compact();
    await session.undoTurns(1);
    releaseSession(session);
    expect(peekSession("thread-a")).toBe(session);
    expect(mocks.startReview).toHaveBeenCalledTimes(1);
    expect(mocks.compactThread).not.toHaveBeenCalled();
    expect(mocks.revertThread).not.toHaveBeenCalled();
    expect(mocks.interruptTurn).not.toHaveBeenCalled();
    request.resolve({ id: "review", status: "inProgress", items: [] });
    await Promise.all([reviewing, stopping]);
    expect(session.activeTurn?.id).toBe("review");
    expect(mocks.interruptTurn).toHaveBeenCalledWith("thread-a", "review");
  });

  it("does not resurrect a completed review or stop the queued successor", async () => {
    const session = await idleSession();
    const request = deferred<{ id: string; status: string; items: [] }>();
    mocks.startReview.mockReturnValueOnce(request.promise);
    const reviewing = session.review({ type: "uncommittedChanges" });
    const stopping = session.interrupt();
    await session.send([{ type: "text", text: "next" }]);
    emit("turn/completed", { threadId: "thread-a", turn: { id: "review", status: "completed", items: [] } });
    expect(mocks.startTurn).not.toHaveBeenCalled();
    request.resolve({ id: "review", status: "inProgress", items: [] });
    await Promise.all([reviewing, stopping]);
    await settle();
    expect(session.thread?.turns.find((turn) => turn.id === "review")?.status).toBe("completed");
    expect(mocks.interruptTurn).not.toHaveBeenCalled();
    expect(mocks.startTurn).toHaveBeenCalledTimes(1);
  });

  it.each(["review", "compact", "undo"] as const)("releases queue exclusion after %s failure", async (kind) => {
    const session = await idleSession();
    const request = deferred<never>();
    const mock = kind === "review" ? mocks.startReview : kind === "compact" ? mocks.compactThread : mocks.revertThread;
    mock.mockReturnValueOnce(request.promise);
    const operation =
      kind === "review"
        ? session.review({ type: "uncommittedChanges" })
        : kind === "compact"
          ? session.compact()
          : session.undoTurns(1);
    await session.send([{ type: "text", text: "next" }]);
    expect(mocks.startTurn).not.toHaveBeenCalled();
    request.reject(new Error("failed"));
    await operation;
    await settle();
    expect(session.starting).toBe(false);
    expect(session.compacting).toBe(false);
    expect(mocks.startTurn).toHaveBeenCalledTimes(1);
    expect(session.thread?.turns.some((turn) => turn.id === "last")).toBe(true);
  });

  it("keeps compaction excluded when its event precedes request resolution", async () => {
    const session = await idleSession();
    const request = deferred<void>();
    mocks.compactThread.mockReturnValueOnce(request.promise);
    const compacting = session.compact();
    await session.send([{ type: "text", text: "next" }]);
    emit("thread/compacted", { threadId: "thread-a" });
    expect(mocks.startTurn).not.toHaveBeenCalled();
    request.resolve();
    await compacting;
    await settle();
    expect(mocks.startTurn).toHaveBeenCalledTimes(1);
  });

  it("finishes hidden undo, clamps the count, and drops the idle session", async () => {
    const session = await idleSession();
    const request = deferred<void>();
    mocks.revertThread.mockReturnValueOnce(request.promise);
    const undoing = session.undoTurns(20);
    releaseSession(session);
    expect(peekSession("thread-a")).toBe(session);
    request.resolve();
    await undoing;
    expect(mocks.revertThread).toHaveBeenCalledWith("thread-a", "old", []);
    expect(session.thread?.turns).toEqual([]);
    expect(peekSession("thread-a")).toBeNull();
    expect(mocks.invalidateThreadCache).toHaveBeenCalledWith("thread-a");
  });

  it("keeps history and explains when Codex cannot rewind the thread", async () => {
    const session = await idleSession();
    mocks.invalidateThreadCache.mockClear();
    mocks.revertThread.mockRejectedValueOnce(
      new Error("codex-revert-unsupported: this thread's history format cannot be rewound"),
    );
    await session.undoTurns(1);
    expect(session.thread?.turns).toHaveLength(2);
    expect(session.streamError).toBeTruthy();
    expect(session.streamError).not.toContain("codex-revert-unsupported");
    expect(mocks.invalidateThreadCache).not.toHaveBeenCalled();
    mocks.revertThread.mockRejectedValueOnce(new Error("ordinary failure"));
    await session.undoTurns(1);
    expect(session.streamError).toBe("ordinary failure");
    expect(session.thread?.turns).toHaveLength(2);
  });

  it.each(["review", "compact", "undo"] as const)("ignores late %s responses after disconnect", async (kind) => {
    const session = await idleSession();
    const request = deferred<unknown>();
    const mock = kind === "review" ? mocks.startReview : kind === "compact" ? mocks.compactThread : mocks.revertThread;
    mock.mockReturnValueOnce(request.promise);
    const operation =
      kind === "review"
        ? session.review({ type: "uncommittedChanges" })
        : kind === "compact"
          ? session.compact()
          : session.undoTurns(1);
    await session.send([{ type: "text", text: "next" }]);
    session.disconnected();
    request.resolve({ id: "review", status: "inProgress", items: [] });
    await operation;
    await settle();
    expect(session.thread?.turns).toHaveLength(2);
    expect(session.activeTurn).toBeNull();
    expect(mocks.startTurn).not.toHaveBeenCalled();
    expect(session.streamError).toBe("Lost connection to Codex.");
  });

  it("owns draft creation failure without issuing a review", async () => {
    const request = deferred<string>();
    const session = draftSession("/repo", () => request.promise);
    const reviewing = session.review({ type: "uncommittedChanges" });
    releaseSession(session);
    expect(session.working()).toBe(true);
    request.reject(new Error("creation failed"));
    await reviewing;
    expect(session.starting).toBe(false);
    expect(session.streamError).toBe("creation failed");
    expect(mocks.startReview).not.toHaveBeenCalled();
  });
});

describe("operation recovery", () => {
  it("does not finish compaction for an old completion before its start", async () => {
    const session = await idleSession();
    await session.compact();
    await session.send([{ type: "text", text: "next" }]);
    emit("turn/completed", { threadId: "thread-a", turn: { id: "last", status: "completed" } });
    expect(session.compacting).toBe(true);
    expect(mocks.startTurn).not.toHaveBeenCalled();
  });

  it("allows explicit retry after disconnect without reviving old review", async () => {
    const session = await idleSession();
    const request = deferred<unknown>();
    mocks.startReview.mockReturnValueOnce(request.promise);
    const reviewing = session.review({ type: "uncommittedChanges" });
    session.disconnected();
    expect(await session.send([{ type: "text", text: "retry" }])).toBe(true);
    request.resolve({ id: "stale-review", status: "inProgress", items: [] });
    await reviewing;
    expect(session.activeTurn?.id).toBe("turn-real");
    emit("turn/completed", { threadId: "thread-a", turn: { id: "turn-real", status: "completed" } });
    expect(session.activeTurn).toBeNull();
  });

  it.each(["review", "compact", "undo"] as const)("ignores %s resolution after disposal", async (kind) => {
    const session = await idleSession();
    const request = deferred<unknown>();
    const mock = kind === "review" ? mocks.startReview : kind === "compact" ? mocks.compactThread : mocks.revertThread;
    mock.mockReturnValueOnce(request.promise);
    const operation =
      kind === "review"
        ? session.review({ type: "uncommittedChanges" })
        : kind === "compact"
          ? session.compact()
          : session.undoTurns(1);
    session.dispose();
    request.resolve({ id: "review", status: "inProgress", items: [] });
    await operation;
    expect(session.thread?.turns).toHaveLength(2);
    expect(mocks.startTurn).not.toHaveBeenCalled();
    expect(attachSession(session, "resurrected")).toBe(false);
    expect(peekSession("resurrected")).toBeNull();
  });

  it("ignores empty undo", async () => {
    const session = await idleSession();
    await session.undoTurns(10);
    expect(session.thread?.turns).toEqual([]);
    await session.undoTurns(1);
    expect(mocks.revertThread).toHaveBeenCalledTimes(1);
  });
});

it("rejects draft attachment when creation resolves after registry disconnect", async () => {
  const request = deferred<string>();
  const session = draftSession("/repo", async (owner) => {
    const id = await request.promise;
    if (!attachSession(owner, id)) throw new Error("interrupted");
    return id;
  });
  const reviewing = session.review({ type: "uncommittedChanges" });
  emit("disconnected", null);
  request.resolve("late-draft");
  await reviewing;
  expect(mocks.startReview).not.toHaveBeenCalled();
  expect(peekSession("late-draft")).toBeNull();
});

it("does not invalidate an idle draft for a disconnect before its first review", async () => {
  const session = draftSession("/repo", async (owner) => {
    expect(attachSession(owner, "new-draft")).toBe(true);
    return "new-draft";
  });
  emit("disconnected", null);
  await session.review({ type: "uncommittedChanges" });
  expect(mocks.startReview).toHaveBeenCalledWith("new-draft", { type: "uncommittedChanges" });
  expect(session.activeTurn?.id).toBe("review");
});
