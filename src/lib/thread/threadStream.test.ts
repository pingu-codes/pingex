import { describe, expect, it } from "vitest";
import { type FakeEvent, fakeEvent } from "$lib/testing/codexEvents";
import { applyThreadEvent as applyTyped, ensureTurn, upsertItem } from "$lib/thread/threadStream";
import type { ThreadDetail, Turn } from "$lib/types";

const applyThreadEvent = (thread: ThreadDetail, event: FakeEvent) => applyTyped(thread, fakeEvent(event));

const makeThread = (turns: Turn[] = []): ThreadDetail => ({ id: "t", preview: "", cwd: "", turns });

describe("ensureTurn", () => {
  it("adopts an optimistic local turn when the real id arrives", () => {
    const turns: Turn[] = [{ id: "local-123", status: "inProgress", items: [] }];
    const turn = ensureTurn(turns, "turn-1");
    expect(turn.id).toBe("turn-1");
    expect(turns).toHaveLength(1);
  });

  it("creates a new turn when none matches", () => {
    const turns: Turn[] = [];
    ensureTurn(turns, "turn-1");
    expect(turns).toEqual([{ id: "turn-1", status: "inProgress", items: [] }]);
  });
});

describe("upsertItem", () => {
  it("replaces the optimistic local user message", () => {
    const turns: Turn[] = [
      {
        id: "turn-1",
        status: "inProgress",
        items: [{ type: "userMessage", id: "local-item-1", content: [{ type: "text", text: "hi" }] }],
      },
    ];
    upsertItem(turns, "turn-1", { type: "userMessage", id: "server-1", content: [{ type: "text", text: "hi" }] });
    expect(turns[0].items).toHaveLength(1);
    expect(turns[0].items[0].id).toBe("server-1");
  });

  it("updates an existing item by id and appends unknown items", () => {
    const turns: Turn[] = [
      { id: "turn-1", status: "inProgress", items: [{ type: "agentMessage", id: "i1", text: "a" }] },
    ];
    upsertItem(turns, "turn-1", { type: "agentMessage", id: "i1", text: "ab" });
    expect(turns[0].items[0].text).toBe("ab");
    upsertItem(turns, "turn-1", { type: "reasoning", id: "i2" });
    expect(turns[0].items).toHaveLength(2);
  });
});

describe("applyThreadEvent", () => {
  it("accumulates agent message deltas", () => {
    const thread = makeThread();
    applyThreadEvent(thread, {
      method: "item/agentMessage/delta",
      params: { threadId: "t", turnId: "turn-1", itemId: "m1", delta: "Hello " },
    });
    applyThreadEvent(thread, {
      method: "item/agentMessage/delta",
      params: { threadId: "t", turnId: "turn-1", itemId: "m1", delta: "world" },
    });
    expect(thread.turns[0].items[0].text).toBe("Hello world");
  });

  it("marks a message as streaming while deltas arrive and clears it on completion", () => {
    const thread = makeThread();
    applyThreadEvent(thread, {
      method: "item/agentMessage/delta",
      params: { threadId: "t", turnId: "turn-1", itemId: "m1", delta: "Adding x to y" },
    });
    expect(thread.turns[0].items[0].streaming).toBe(true);
    applyThreadEvent(thread, {
      method: "item/completed",
      params: { threadId: "t", turnId: "turn-1", item: { type: "agentMessage", id: "m1", text: "Adding x to y" } },
    });
    expect(thread.turns[0].items[0].streaming).toBeUndefined();
  });

  it("pads reasoning summaries to the summary index before appending deltas", () => {
    const thread = makeThread();
    applyThreadEvent(thread, {
      method: "item/reasoning/summaryPartAdded",
      params: { threadId: "t", turnId: "turn-1", itemId: "r1", summaryIndex: 1 },
    });
    applyThreadEvent(thread, {
      method: "item/reasoning/summaryTextDelta",
      params: { threadId: "t", turnId: "turn-1", itemId: "r1", summaryIndex: 1, delta: "thinking" },
    });
    expect(thread.turns[0].items[0].summary).toEqual(["", "thinking"]);
  });

  it("keeps the streamed reasoning summary when the completed item omits it", () => {
    const thread = makeThread();
    applyThreadEvent(thread, {
      method: "item/reasoning/summaryTextDelta",
      params: { threadId: "t", turnId: "turn-1", itemId: "r1", summaryIndex: 0, delta: "Weighing it" },
    });
    // Codex reports reasoning text only as deltas; the item that ends it
    // carries an empty summary, and taking that at face value would leave the
    // transcript with a "Worked for Ns" section that expands to nothing.
    applyThreadEvent(thread, {
      method: "item/completed",
      params: { threadId: "t", turnId: "turn-1", item: { type: "reasoning", id: "r1", summary: [] } },
    });
    expect(thread.turns[0].items[0].summary).toEqual(["Weighing it"]);
  });

  it("lets a reported summary replace the streamed one", () => {
    const thread = makeThread();
    applyThreadEvent(thread, {
      method: "item/reasoning/summaryTextDelta",
      params: { threadId: "t", turnId: "turn-1", itemId: "r1", summaryIndex: 0, delta: "Partial" },
    });
    applyThreadEvent(thread, {
      method: "item/completed",
      params: { threadId: "t", turnId: "turn-1", item: { type: "reasoning", id: "r1", summary: ["Full text"] } },
    });
    expect(thread.turns[0].items[0].summary).toEqual(["Full text"]);
  });

  it("unions the files a patch reports as it applies", () => {
    const thread = makeThread();
    applyThreadEvent(thread, {
      method: "item/fileChange/patchUpdated",
      params: {
        threadId: "t",
        turnId: "turn-1",
        itemId: "fc1",
        changes: [{ path: "src/a.ts", kind: { type: "update" }, diff: "+a" }],
      },
    });
    // A later report need not repeat every file the earlier one named, so
    // replacing the array outright would lose the edit to `a.ts`.
    applyThreadEvent(thread, {
      method: "item/fileChange/patchUpdated",
      params: {
        threadId: "t",
        turnId: "turn-1",
        itemId: "fc1",
        changes: [
          { path: "src/b.ts", kind: { type: "add" }, diff: "+b" },
          { path: "src/a.ts", kind: { type: "update" }, diff: "+a\n+a2" },
        ],
      },
    });
    expect(thread.turns[0].items[0].changes).toEqual([
      { path: "src/a.ts", kind: { type: "update" }, diff: "+a\n+a2" },
      { path: "src/b.ts", kind: { type: "add" }, diff: "+b" },
    ]);
  });

  it("keeps the streamed file changes when the completed item drops them", () => {
    const thread = makeThread();
    applyThreadEvent(thread, {
      method: "item/fileChange/patchUpdated",
      params: {
        threadId: "t",
        turnId: "turn-1",
        itemId: "fc1",
        changes: [
          { path: "src/a.ts", kind: { type: "update" }, diff: "+a" },
          { path: "src/b.ts", kind: { type: "add" }, diff: "+b" },
        ],
      },
    });
    applyThreadEvent(thread, {
      method: "item/completed",
      params: {
        threadId: "t",
        turnId: "turn-1",
        item: { type: "fileChange", id: "fc1", changes: [] },
      },
    });
    expect(thread.turns[0].items[0].changes?.map((change) => change.path)).toEqual(["src/a.ts", "src/b.ts"]);
  });

  it("lets a completed file change add files without dropping the streamed ones", () => {
    const thread = makeThread();
    applyThreadEvent(thread, {
      method: "item/fileChange/patchUpdated",
      params: {
        threadId: "t",
        turnId: "turn-1",
        itemId: "fc1",
        changes: [{ path: "src/a.ts", kind: { type: "update" }, diff: "+a" }],
      },
    });
    applyThreadEvent(thread, {
      method: "item/completed",
      params: {
        threadId: "t",
        turnId: "turn-1",
        item: {
          type: "fileChange",
          id: "fc1",
          changes: [{ path: "src/b.ts", kind: { type: "add" }, diff: "+b" }],
        },
      },
    });
    expect(thread.turns[0].items[0].changes?.map((change) => change.path)).toEqual(["src/a.ts", "src/b.ts"]);
  });

  it("marks turn completion and copies timing fields", () => {
    const thread = makeThread([{ id: "turn-1", status: "inProgress", items: [] }]);
    const outcome = applyThreadEvent(thread, {
      method: "turn/completed",
      params: { threadId: "t", turn: { id: "turn-1", status: "completed", durationMs: 1200 } },
    });
    expect(outcome.turnCompleted).toBe(true);
    expect(thread.turns[0].status).toBe("completed");
    expect(thread.turns[0].durationMs).toBe(1200);
  });

  it("flags collab tool calls and surfaces stream errors", () => {
    const thread = makeThread();
    const collab = applyThreadEvent(thread, {
      method: "item/completed",
      params: { threadId: "t", turnId: "turn-1", item: { type: "collabAgentToolCall", id: "c1" } },
    });
    expect(collab.collabToolCall).toBe(true);
    const errored = applyThreadEvent(thread, {
      method: "error",
      params: { threadId: "t", error: { message: "boom" } },
    });
    expect(errored.streamError).toBe("boom");
  });
});

describe("applyThreadEvent — streaming Codex added later", () => {
  it("streams plan text the way it streams an agent message", () => {
    const thread = makeThread();
    applyThreadEvent(thread, {
      method: "item/plan/delta",
      params: { threadId: "t", turnId: "turn-1", itemId: "p1", delta: "Step " },
    });
    applyThreadEvent(thread, {
      method: "item/plan/delta",
      params: { threadId: "t", turnId: "turn-1", itemId: "p1", delta: "one" },
    });
    expect(thread.turns[0].items[0]).toMatchObject({ type: "plan", text: "Step one", streaming: true });
  });

  it("accumulates raw reasoning by content index", () => {
    const thread = makeThread();
    applyThreadEvent(thread, {
      method: "item/reasoning/textDelta",
      params: { threadId: "t", turnId: "turn-1", itemId: "r1", contentIndex: 1, delta: "second" },
    });
    applyThreadEvent(thread, {
      method: "item/reasoning/textDelta",
      params: { threadId: "t", turnId: "turn-1", itemId: "r1", contentIndex: 0, delta: "first" },
    });
    expect(thread.turns[0].items[0].content).toEqual(["first", "second"]);
  });

  // The completed reasoning item repeats neither half of what it streamed.
  it("keeps streamed raw reasoning when the item completes empty", () => {
    const thread = makeThread();
    applyThreadEvent(thread, {
      method: "item/reasoning/textDelta",
      params: { threadId: "t", turnId: "turn-1", itemId: "r1", contentIndex: 0, delta: "Weighing" },
    });
    applyThreadEvent(thread, {
      method: "item/completed",
      params: { threadId: "t", turnId: "turn-1", item: { type: "reasoning", id: "r1", summary: [], content: [] } },
    });
    expect(thread.turns[0].items[0].content).toEqual(["Weighing"]);
  });

  it("appends what Codex typed into an interactive command", () => {
    const thread = makeThread();
    applyThreadEvent(thread, {
      method: "item/commandExecution/outputDelta",
      params: { threadId: "t", turnId: "turn-1", itemId: "c1", delta: "Continue? " },
    });
    applyThreadEvent(thread, {
      method: "item/commandExecution/terminalInteraction",
      params: { threadId: "t", turnId: "turn-1", itemId: "c1", processId: 1, stdin: "y\n" },
    });
    expect(thread.turns[0].items[0].aggregatedOutput).toBe("Continue? y\n");
  });

  it("replaces rather than accumulates MCP progress", () => {
    const thread = makeThread();
    for (const message of ["Fetching…", "Parsing…"]) {
      applyThreadEvent(thread, {
        method: "item/mcpToolCall/progress",
        params: { threadId: "t", turnId: "turn-1", itemId: "m1", message },
      });
    }
    expect(thread.turns[0].items[0].progress).toBe("Parsing…");
  });
});

describe("applyThreadEvent — notices", () => {
  it("presents a retryable error as a notice, not a failure", () => {
    const outcome = applyThreadEvent(makeThread(), {
      method: "error",
      params: { threadId: "t", willRetry: true, error: { message: "Upstream hiccup." } },
    });
    expect(outcome.streamError).toBeUndefined();
    expect(outcome.notice).toBe("Upstream hiccup. Retrying…");
  });

  it.each(["warning", "guardianWarning", "configWarning"])("surfaces %s", (method) => {
    const outcome = applyThreadEvent(makeThread(), {
      method,
      params: { threadId: "t", message: "Careful." },
    });
    expect(outcome.notice).toBe("Careful.");
  });

  // A deprecation notice spells its text differently to the rest.
  it("reads a deprecation notice's summary and details", () => {
    const outcome = applyThreadEvent(makeThread(), {
      method: "deprecationNotice",
      params: { threadId: "t", summary: "Old flag", details: "use --new" },
    });
    expect(outcome.notice).toBe("Old flag — use --new");
  });

  it("announces a model reroute", () => {
    const outcome = applyThreadEvent(makeThread(), {
      method: "model/rerouted",
      params: { threadId: "t", fromModel: "a", toModel: "b" },
    });
    expect(outcome.notice).toBe("Switched from a to b.");
  });

  it("reports a hook that failed and stays quiet about one that did not", () => {
    const failed = applyThreadEvent(makeThread(), {
      method: "hook/completed",
      params: {
        threadId: "t",
        run: { eventName: "preToolUse", status: "failed", entries: [{ kind: "error", text: "exit 1" }] },
      },
    });
    expect(failed.notice).toBe("Hook preToolUse failed: exit 1");
    const fine = applyThreadEvent(makeThread(), {
      method: "hook/completed",
      params: { threadId: "t", run: { eventName: "preToolUse", status: "completed", entries: [] } },
    });
    expect(fine.notice).toBeUndefined();
  });

  it("only buffers loudly when Codex asks for the UI", () => {
    const shown = applyThreadEvent(makeThread(), {
      method: "model/safetyBuffering/updated",
      params: { threadId: "t", showBufferingUi: true },
    });
    expect(shown.notice).toBeTruthy();
    const quiet = applyThreadEvent(makeThread(), {
      method: "model/safetyBuffering/updated",
      params: { threadId: "t", showBufferingUi: false },
    });
    expect(quiet.notice).toBeUndefined();
    expect(quiet.bufferingEnded).toBe(true);
    expect(shown.bufferingEnded).toBeUndefined();
  });
});

describe("applyThreadEvent — guardian reviews", () => {
  const review = { status: "denied", riskLevel: "high", rationale: "Deletes the repo." };

  it("attaches the verdict to the item it judged", () => {
    const thread = makeThread([
      { id: "turn-1", status: "inProgress", items: [{ type: "commandExecution", id: "c1" }] },
    ]);
    applyThreadEvent(thread, {
      method: "item/autoApprovalReview/completed",
      params: { threadId: "t", turnId: "turn-1", targetItemId: "c1", review },
    });
    expect(thread.turns[0].items[0].guardianReview).toEqual(review);
  });

  // Network-policy reviews belong to no single item.
  it("drops a review with no target rather than guessing one", () => {
    const thread = makeThread([
      { id: "turn-1", status: "inProgress", items: [{ type: "commandExecution", id: "c1" }] },
    ]);
    applyThreadEvent(thread, {
      method: "item/autoApprovalReview/completed",
      params: { threadId: "t", turnId: "turn-1", targetItemId: null, review },
    });
    expect(thread.turns[0].items[0].guardianReview).toBeUndefined();
  });
});

// Replays the notification sequence a real `/review` produces, taken from a
// journalled review: Codex streams every item under the id `review/start`
// returned, announces a `turn/started` under an unrelated id, and never sends a
// `turn/completed` for either. Both of those left a turn running for good.
describe("applyThreadEvent — reviews", () => {
  const REVIEW = "019fd778-0946-7d33-a8d6-79ee863fb820";
  const BOOKKEEPING = "019fd778-09cf-7321-8695-ae4671069e15";

  const enterReview = (thread: ThreadDetail) => {
    applyThreadEvent(thread, {
      method: "item/completed",
      params: {
        threadId: "t",
        turnId: REVIEW,
        item: { type: "enteredReviewMode", id: "r1", review: "current changes" },
      },
    });
  };

  const exitReview = (thread: ThreadDetail) => {
    return applyThreadEvent(thread, {
      method: "item/completed",
      params: {
        threadId: "t",
        turnId: REVIEW,
        item: { type: "exitedReviewMode", id: "r2", review: "No issues found." },
      },
    });
  };

  it("stops showing the turn as running once the review exits", () => {
    const thread = makeThread([{ id: REVIEW, status: "inProgress", items: [] }]);
    enterReview(thread);
    const outcome = exitReview(thread);
    expect(thread.turns.every((turn) => turn.status !== "inProgress")).toBe(true);
    expect(outcome.turnCompleted).toBe(true);
  });

  it("ignores the turn Codex opens under its own id mid-review", () => {
    const thread = makeThread([{ id: REVIEW, status: "inProgress", items: [] }]);
    enterReview(thread);
    applyThreadEvent(thread, {
      method: "turn/started",
      params: { threadId: "t", turn: { id: BOOKKEEPING } },
    });
    expect(thread.turns.map((turn) => turn.id)).toEqual([REVIEW]);
    exitReview(thread);
    expect(thread.turns.every((turn) => turn.status !== "inProgress")).toBe(true);
  });

  it("still opens a turn Codex starts once the review has ended", () => {
    const thread = makeThread([{ id: REVIEW, status: "inProgress", items: [] }]);
    enterReview(thread);
    exitReview(thread);
    applyThreadEvent(thread, {
      method: "turn/started",
      params: { threadId: "t", turn: { id: "turn-next" } },
    });
    expect(thread.turns.map((turn) => turn.id)).toEqual([REVIEW, "turn-next"]);
  });

  it("applies a completion whose id matches nothing to the running turn", () => {
    const thread = makeThread([{ id: REVIEW, status: "inProgress", items: [] }]);
    applyThreadEvent(thread, {
      method: "turn/completed",
      params: { threadId: "t", turn: { id: BOOKKEEPING, status: "completed", durationMs: 90 } },
    });
    expect(thread.turns).toHaveLength(1);
    expect(thread.turns[0].status).toBe("completed");
    expect(thread.turns[0].durationMs).toBe(90);
  });

  it("ends the turn on an error Codex will not retry", () => {
    const thread = makeThread([{ id: "turn-1", status: "inProgress", items: [] }]);
    applyThreadEvent(thread, {
      method: "error",
      params: { threadId: "t", willRetry: false, error: { message: "Boom." } },
    });
    expect(thread.turns[0].status).toBe("failed");
  });

  // Codex ≥0.151 explains an alignment stop and suggests how to carry on;
  // that belongs on the failed turn, not in a toast.
  it("keeps a misalignment explanation on the failed turn instead of a stream error", () => {
    const thread = makeThread([{ id: "turn-1", status: "inProgress", items: [] }]);
    const misalignment = { errorType: "x", detailedExplanation: "Out of bounds.", steer: { message: "Try safer." } };
    const outcome = applyThreadEvent(thread, {
      method: "error",
      params: { threadId: "t", willRetry: false, error: { message: "Stopped.", misalignment } },
    });
    expect(outcome.streamError).toBeUndefined();
    expect(outcome.misalignment).toEqual(misalignment);
    expect(thread.turns[0].status).toBe("failed");
    expect(thread.turns[0].error).toEqual({ message: "Stopped.", misalignment });

    // A completion that follows without its own error keeps the details.
    applyThreadEvent(thread, {
      method: "turn/completed",
      params: { threadId: "t", turn: { id: "turn-1", status: "failed" } },
    });
    expect(thread.turns[0].error?.misalignment).toEqual(misalignment);
  });

  it("leaves the turn running for an error Codex will retry", () => {
    const thread = makeThread([{ id: "turn-1", status: "inProgress", items: [] }]);
    applyThreadEvent(thread, {
      method: "error",
      params: { threadId: "t", willRetry: true, error: { message: "Boom." } },
    });
    expect(thread.turns[0].status).toBe("inProgress");
  });

  // The session store treats an unmatched completion as "this thread is idle",
  // so the transcript must not leave any other turn running either.
  it("ends every running turn on a completion whose id matches nothing", () => {
    const thread = makeThread([
      { id: "turn-1", status: "inProgress", items: [] },
      { id: "turn-2", status: "inProgress", items: [] },
    ]);
    applyThreadEvent(thread, {
      method: "turn/completed",
      params: { threadId: "t", turn: { id: "unknown", status: "completed" } },
    });
    expect(thread.turns.map((turn) => turn.status)).toEqual(["completed", "completed"]);
  });

  it("leaves an unrelated running turn alone when a review exits", () => {
    const thread = makeThread([
      {
        id: REVIEW,
        status: "inProgress",
        items: [{ type: "enteredReviewMode", id: "r1" }],
      },
      { id: "turn-queued", status: "inProgress", items: [] },
    ]);
    applyThreadEvent(thread, {
      method: "item/completed",
      params: { threadId: "t", turnId: REVIEW, item: { type: "exitedReviewMode", id: "r2" } },
    });
    expect(thread.turns.find((turn) => turn.id === REVIEW)?.status).toBe("completed");
    expect(thread.turns.find((turn) => turn.id === "turn-queued")?.status).toBe("inProgress");
  });
});

// On a thread with a goal, Codex runs the turn under a different id from the
// one `turn/start` returned (observed against codex 0.147.0): the response
// named one id, `turn/started` and every item another. Without adoption the
// user's message sat in a turn that never ended while the reply landed in a
// second one.
describe("applyThreadEvent — turn/started under a new id", () => {
  it("renames the turn still waiting on its own message rather than opening a second", () => {
    const thread = makeThread([
      { id: "turn-1", status: "completed", items: [] },
      {
        id: "response-id",
        status: "inProgress",
        items: [{ type: "userMessage", id: "local-item-1", content: [{ type: "text", text: "hi" }] }],
      },
    ]);
    applyThreadEvent(thread, { method: "turn/started", params: { threadId: "t", turn: { id: "real-id" } } });
    expect(thread.turns.map((turn) => turn.id)).toEqual(["turn-1", "real-id"]);
    applyThreadEvent(thread, {
      method: "item/completed",
      params: { threadId: "t", turnId: "real-id", item: { type: "userMessage", id: "u1", content: [] } },
    });
    applyThreadEvent(thread, {
      method: "turn/completed",
      params: { threadId: "t", turn: { id: "real-id", status: "completed" } },
    });
    expect(thread.turns[1].items.map((item) => item.id)).toEqual(["u1"]);
    expect(thread.turns.every((turn) => turn.status !== "inProgress")).toBe(true);
  });

  it("leaves a running turn that already has server items alone", () => {
    const thread = makeThread([
      { id: "turn-1", status: "inProgress", items: [{ type: "agentMessage", id: "m1", text: "…" }] },
    ]);
    applyThreadEvent(thread, { method: "turn/started", params: { threadId: "t", turn: { id: "turn-2" } } });
    expect(thread.turns.map((turn) => turn.id)).toEqual(["turn-1", "turn-2"]);
  });
});
