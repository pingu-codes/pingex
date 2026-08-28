import { beforeEach, describe, expect, it, vi } from "vitest";
import type { QueuedSubmission, TurnOptions, UserInputPart } from "$lib/types";

const mocks = vi.hoisted(() => ({
  queueAdd: vi.fn(),
  queueDelete: vi.fn(),
  queueList: vi.fn(),
  queueUpdate: vi.fn(),
  queueReorder: vi.fn(),
}));

vi.mock("$lib/services/api", () => ({
  isQueueUnsupported: (cause: unknown) =>
    (cause instanceof Error ? cause.message : String(cause)).startsWith("codex-queue-unsupported"),
  queueAdd: mocks.queueAdd,
  queueDelete: mocks.queueDelete,
  queueList: mocks.queueList,
  queueUpdate: mocks.queueUpdate,
  queueReorder: mocks.queueReorder,
}));

import { isLocalOnly } from "$lib/thread/queueEntries";
import { ThreadQueue } from "$lib/thread/threadQueue.svelte";

const unsupported = new Error("codex-queue-unsupported: this Codex version is older than the thread/queue APIs");

const text = (value: string): UserInputPart[] => [{ type: "text", text: value }];
const texts = (queue: ThreadQueue) => queue.entries.map((entry) => entry.input[0].text);
const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

/** A queue on a thread whose owner we script: `idle` flips when the fake turn
 *  ends, `send` records what reached Codex. */
function harness(overrides: { threadId?: string | null; sendResult?: boolean } = {}) {
  const state = { idle: false, threadId: overrides.threadId === undefined ? "thread-1" : overrides.threadId };
  const send = vi.fn(async (_input: UserInputPart[], _options?: TurnOptions) => overrides.sendResult ?? true);
  const interrupt = vi.fn(async () => {});
  const onNotice = vi.fn();
  const onError = vi.fn();
  const queue = new ThreadQueue({
    threadId: () => state.threadId,
    send,
    interrupt,
    idle: () => state.idle,
    onNotice,
    onError,
  });
  const finishTurn = () => {
    state.idle = true;
    queue.maybeDrain();
  };
  return { queue, state, send, interrupt, onNotice, onError, finishTurn };
}

beforeEach(() => {
  mocks.queueAdd.mockReset();
  mocks.queueDelete.mockReset();
  mocks.queueList.mockReset();
  mocks.queueUpdate.mockReset();
  mocks.queueReorder.mockReset();
  mocks.queueAdd.mockImplementation((_threadId, input, clientUserMessageId) =>
    Promise.resolve({ id: `q-${clientUserMessageId}`, input, clientUserMessageId }),
  );
  mocks.queueDelete.mockResolvedValue(true);
  mocks.queueList.mockResolvedValue([]);
  mocks.queueUpdate.mockResolvedValue(undefined);
  mocks.queueReorder.mockResolvedValue(undefined);
});

describe("ThreadQueue on a Codex that holds the queue", () => {
  it("shows the message at once and swaps in the server's entry", async () => {
    const { queue } = harness();
    const adding = queue.add(text("Then do this"));
    expect(texts(queue)).toEqual(["Then do this"]);
    expect(queue.entries[0].id).toMatch(/^pending-/);
    await adding;
    expect(queue.entries[0].id).toMatch(/^q-/);
    expect(mocks.queueAdd).toHaveBeenCalledWith("thread-1", text("Then do this"), expect.any(String));
  });

  it("takes the head off the server before sending it, with its options", async () => {
    const { queue, send, finishTurn } = harness();
    const options = { model: "gpt-5.2-codex" } as TurnOptions;
    await queue.add(text("Then do this"), options);
    const [entry] = queue.entries;

    finishTurn();
    await settle();

    expect(mocks.queueDelete).toHaveBeenCalledWith("thread-1", entry.id);
    expect(send).toHaveBeenCalledWith(text("Then do this"), options);
    expect(queue.entries).toHaveLength(0);
  });

  it("puts a message back, marked local, when the send reached nothing", async () => {
    const { queue, send, finishTurn } = harness({ sendResult: false });
    await queue.add(text("Then do this"));

    finishTurn();
    await settle();

    expect(send).toHaveBeenCalledTimes(1);
    expect(texts(queue)).toEqual(["Then do this"]);
    expect(isLocalOnly(queue.entries[0])).toBe(true);
    // Nothing changed, so nothing retries by itself — that would spin.
    queue.maybeDrain();
    await settle();
    expect(send).toHaveBeenCalledTimes(1);
    // The user sending again is the retry.
    queue.unblock();
    queue.maybeDrain();
    await settle();
    expect(send).toHaveBeenCalledTimes(2);
  });

  it("edits on the server too", async () => {
    const { queue } = harness();
    await queue.add(text("Then do this"));
    const [entry] = queue.entries;
    await queue.edit(entry, text("Do that instead"));
    expect(texts(queue)).toEqual(["Do that instead"]);
    expect(mocks.queueUpdate).toHaveBeenCalledWith("thread-1", entry.id, text("Do that instead"));
  });

  it("reports an edit the server refused without losing the local change", async () => {
    const { queue, onError } = harness();
    mocks.queueUpdate.mockRejectedValue(new Error("nope"));
    await queue.add(text("Then do this"));
    await queue.edit(queue.entries[0], text("Do that instead"));
    expect(texts(queue)).toEqual(["Do that instead"]);
    expect(onError).toHaveBeenCalledWith("Could not update the queued message: nope");
  });

  it("promote reorders on the server and stops the running turn", async () => {
    const { queue, interrupt } = harness();
    await queue.add(text("First"));
    await queue.add(text("Second"));
    await queue.promote(queue.entries[1]);
    expect(texts(queue)).toEqual(["Second", "First"]);
    expect(mocks.queueReorder).toHaveBeenCalledWith("thread-1", [queue.entries[0].id, queue.entries[1].id]);
    expect(interrupt).toHaveBeenCalled();
  });

  it("ignores a server re-list while its own write is in flight", async () => {
    const { queue } = harness();
    let resolveAdd: (entry: QueuedSubmission) => void = () => {};
    mocks.queueAdd.mockImplementation(() => new Promise((resolve) => (resolveAdd = resolve)));
    const adding = queue.add(text("Mine"));
    queue.syncFromServer();
    expect(mocks.queueList).not.toHaveBeenCalled();
    resolveAdd({ id: "q-1", input: text("Mine"), clientUserMessageId: queue.entries[0].clientUserMessageId });
    await adding;
    expect(queue.entries[0].id).toBe("q-1");
  });

  it("merges a re-list rather than replacing what this window holds", async () => {
    const { queue } = harness();
    mocks.queueAdd.mockRejectedValue(unsupported);
    await queue.add(text("Mine alone"));
    mocks.queueList.mockResolvedValue([{ id: "q-other", input: text("Theirs"), clientUserMessageId: "c-other" }]);

    queue.syncFromServer();
    await settle();

    expect(texts(queue)).toEqual(["Theirs", "Mine alone"]);
  });
});

describe("ThreadQueue when Codex cannot hold the queue", () => {
  beforeEach(() => {
    mocks.queueAdd.mockRejectedValue(unsupported);
  });

  it("keeps the message as local-only and says nothing about it", async () => {
    const { queue, onNotice } = harness();
    await queue.add(text("Then do this"));
    expect(texts(queue)).toEqual(["Then do this"]);
    expect(isLocalOnly(queue.entries[0])).toBe(true);
    // An old Codex is not an error, so nothing to read.
    expect(onNotice).not.toHaveBeenCalled();
  });

  it("explains a queue that exists but refused, without losing the message", async () => {
    const { queue, onNotice } = harness();
    mocks.queueAdd.mockRejectedValue(new Error("queue cannot contain more than 100 submissions"));
    await queue.add(text("One too many"));
    expect(texts(queue)).toEqual(["One too many"]);
    expect(onNotice).toHaveBeenCalledWith(expect.stringContaining("queue cannot contain more than 100 submissions"));
  });

  it("drains held messages in the order they were typed once the turn ends", async () => {
    const { queue, send, finishTurn } = harness();
    await queue.add(text("First"));
    await queue.add(text("Second"));
    expect(send).not.toHaveBeenCalled();

    finishTurn();
    await settle();

    expect(send.mock.calls.map((call) => call[0])).toEqual([text("First"), text("Second")]);
    expect(queue.entries).toHaveLength(0);
    // Nothing on the server to delete for a local entry.
    expect(mocks.queueDelete).not.toHaveBeenCalled();
  });

  it("promote puts the message first without a server reorder", async () => {
    const { queue, interrupt, send, finishTurn } = harness();
    await queue.add(text("First"));
    await queue.add(text("Second"));
    await queue.promote(queue.entries[1]);
    expect(mocks.queueReorder).not.toHaveBeenCalled();
    expect(interrupt).toHaveBeenCalled();

    finishTurn();
    await settle();
    expect(send.mock.calls[0][0]).toEqual(text("Second"));
  });

  it("does not let a server listing drop a message only this window has", async () => {
    const { queue } = harness();
    await queue.add(text("Mine alone"));
    mocks.queueList.mockResolvedValue([]);
    queue.syncFromServer();
    await settle();
    expect(texts(queue)).toEqual(["Mine alone"]);
  });
});

describe("ThreadQueue on a draft thread", () => {
  it("holds messages locally and drains them once the thread exists", async () => {
    const { queue, state, send } = harness({ threadId: null });
    await queue.add(text("Then do this"));
    expect(mocks.queueAdd).not.toHaveBeenCalled();
    expect(isLocalOnly(queue.entries[0])).toBe(true);
    // The thread got created and its opening turn finished.
    state.threadId = "thread-1";
    state.idle = true;
    queue.maybeDrain();
    await settle();
    expect(send).toHaveBeenCalledWith(text("Then do this"), undefined);
  });

  it("never asks the server about a draft", () => {
    const { queue } = harness({ threadId: null });
    queue.syncFromServer();
    expect(mocks.queueList).not.toHaveBeenCalled();
  });
});
