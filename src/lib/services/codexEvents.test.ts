import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("$lib/services/api", () => ({ recordUserInputRequest: vi.fn().mockResolvedValue(undefined) }));

import {
  activeTurns,
  approvals,
  elicitations,
  previewEmit,
  previewEmitServerRequest,
  turnPlans,
  userInputRequests,
} from "$lib/services/codexEvents.svelte";

beforeEach(() => {
  approvals.list = [];
  userInputRequests.list = [];
  elicitations.list = [];
  turnPlans.byThread = {};
  activeTurns.list = [];
  // Review tracking is module-level; a disconnect resets it between cases.
  previewEmit({ method: "disconnected", params: null });
});

describe("server requests", () => {
  it("turns a permission request into an approval carrying the profile", () => {
    previewEmitServerRequest({
      requestId: 1,
      method: "item/permissions/requestApproval",
      params: {
        threadId: "t",
        turnId: "turn-1",
        itemId: "i1",
        cwd: "/repo",
        permissions: { network: { enabled: true } },
      },
    });

    expect(approvals.list).toEqual([
      {
        requestId: 1,
        kind: "permissions",
        threadId: "t",
        turnId: "turn-1",
        itemId: "i1",
        cwd: "/repo",
        reason: undefined,
        permissions: { network: { enabled: true } },
      },
    ]);
  });

  it("queues an MCP elicitation with its mode and schema", () => {
    previewEmitServerRequest({
      requestId: 2,
      method: "mcpServer/elicitation/request",
      params: {
        threadId: "t",
        turnId: null,
        serverName: "linear",
        mode: "form",
        message: "Which team?",
        requestedSchema: { type: "object", properties: {} },
      },
    });

    expect(elicitations.list).toMatchObject([{ requestId: 2, serverName: "linear", mode: "form" }]);
  });

  // Newer Codex builds put real values (e.g. `suggestion_id`) in `_meta` where
  // older ones sent null; it is carried so the response can echo it.
  it("keeps the elicitation's _meta for the response", () => {
    previewEmitServerRequest({
      requestId: 4,
      method: "mcpServer/elicitation/request",
      params: {
        threadId: "t",
        serverName: "codex_apps",
        mode: "form",
        _meta: { suggestion_id: "request_plugin_install_install-github" },
        message: "Allow?",
        requestedSchema: { type: "object", properties: {} },
      },
    });

    expect(elicitations.list[0]?.meta).toEqual({ suggestion_id: "request_plugin_install_install-github" });
  });

  // Anything unrecognised stalls its turn until the user is told, so it must
  // not disappear silently.
  it("warns rather than swallowing a method it does not know", () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});

    previewEmitServerRequest({ requestId: 3, method: "something/new", params: {} });

    expect(warn).toHaveBeenCalledWith(expect.stringContaining("something/new"), {});
    expect(approvals.list).toHaveLength(0);
    warn.mockRestore();
  });
});

describe("serverRequest/resolved", () => {
  it("clears whichever card was waiting on that request", () => {
    previewEmitServerRequest({
      requestId: 4,
      method: "item/commandExecution/requestApproval",
      params: { threadId: "t", turnId: "turn-1", itemId: "i1", command: "ls" },
    });
    previewEmitServerRequest({
      requestId: 5,
      method: "mcpServer/elicitation/request",
      params: { threadId: "t", serverName: "linear", mode: "form", message: "?" },
    });

    previewEmit({ method: "serverRequest/resolved", params: { threadId: "t", requestId: 4 } });

    expect(approvals.list).toHaveLength(0);
    expect(elicitations.list).toHaveLength(1);
  });
});

describe("turn plans", () => {
  const planEvent = (turnId: string, steps: { step: string; status: string }[]) => ({
    method: "turn/plan/updated",
    params: { threadId: "t", turnId, explanation: "Doing it", plan: steps },
  });

  it("keeps the latest plan per thread", () => {
    previewEmit(planEvent("turn-1", [{ step: "a", status: "pending" }]));
    previewEmit(planEvent("turn-1", [{ step: "a", status: "completed" }]));

    expect(turnPlans.byThread.t).toEqual({
      turnId: "turn-1",
      explanation: "Doing it",
      steps: [{ step: "a", status: "completed" }],
    });
  });

  it("drops the plan when its own turn ends", () => {
    previewEmit(planEvent("turn-1", [{ step: "a", status: "pending" }]));

    previewEmit({ method: "turn/completed", params: { threadId: "t", turn: { id: "turn-1" } } });

    expect(turnPlans.byThread.t).toBeUndefined();
  });

  // A late `turn/completed` for an earlier turn must not wipe the plan the
  // turn now running has already built.
  it("leaves a newer turn's plan alone", () => {
    previewEmit(planEvent("turn-2", [{ step: "a", status: "pending" }]));

    previewEmit({ method: "turn/completed", params: { threadId: "t", turn: { id: "turn-1" } } });

    expect(turnPlans.byThread.t?.turnId).toBe("turn-2");
  });
});

describe("active turns during reviews", () => {
  it("tracks a normal turn's start and completion", () => {
    previewEmit({ method: "turn/started", params: { threadId: "t", turn: { id: "turn-1" } } });
    expect(activeTurns.list).toEqual(["t"]);
    previewEmit({ method: "turn/completed", params: { threadId: "t", turn: { id: "turn-1" } } });
    expect(activeTurns.list).toEqual([]);
  });

  it("ignores the bookkeeping turn/started Codex announces mid-review", () => {
    previewEmit({
      method: "item/completed",
      params: { threadId: "t", turnId: "review-1", item: { type: "enteredReviewMode", id: "r1" } },
    });
    previewEmit({ method: "turn/started", params: { threadId: "t", turn: { id: "phantom" } } });
    expect(activeTurns.list).toEqual([]);
  });

  it("marks the thread idle and resumes normal tracking once the review exits", () => {
    previewEmit({ method: "turn/started", params: { threadId: "t", turn: { id: "review-1" } } });
    previewEmit({
      method: "item/completed",
      params: { threadId: "t", turnId: "review-1", item: { type: "enteredReviewMode", id: "r1" } },
    });
    previewEmit({
      method: "item/completed",
      params: { threadId: "t", turnId: "review-1", item: { type: "exitedReviewMode", id: "r2" } },
    });
    expect(activeTurns.list).toEqual([]);
    previewEmit({ method: "turn/started", params: { threadId: "t", turn: { id: "turn-2" } } });
    expect(activeTurns.list).toEqual(["t"]);
  });

  it("stops suppressing turn/started when a review dies on an error", () => {
    previewEmit({
      method: "item/completed",
      params: { threadId: "t", turnId: "review-1", item: { type: "enteredReviewMode", id: "r1" } },
    });
    previewEmit({ method: "error", params: { threadId: "t", willRetry: false } });
    previewEmit({ method: "turn/started", params: { threadId: "t", turn: { id: "turn-2" } } });
    expect(activeTurns.list).toEqual(["t"]);
  });
});
