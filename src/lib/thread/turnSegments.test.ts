// @vitest-environment node
import { describe, expect, it } from "vitest";
import {
  functionCallOutputText,
  rendersSomething,
  splitTurn,
  turnSegments,
  workedLabel,
} from "$lib/thread/turnSegments";
import type { Turn } from "$lib/types";

describe("splitTurn", () => {
  it("folds completed non-Codex tools while leaving running calls in order", () => {
    const items = [
      { type: "dynamicToolCall", id: "read", status: "completed" },
      { type: "dynamicToolCall", id: "agent", status: "inProgress" },
      { type: "dynamicToolCall", id: "parallel-read", status: "completed" },
    ];
    const turn = { id: "turn", status: "inProgress", items };
    expect(splitTurn(turn, "claude").body).toEqual([
      { kind: "work", items: items.slice(0, 1), collapsed: true },
      { kind: "work", items: items.slice(1) },
    ]);
    expect(splitTurn(turn).body).toEqual([{ kind: "work", items }]);
    expect(splitTurn(turn, "codex").body).toEqual([{ kind: "work", items }]);
  });

  it.each(["completed", "failed", "declined", "cancelled", "interrupted"])("folds a %s non-Codex tail", (status) => {
    const items = [{ type: "dynamicToolCall", id: "agent", status }];
    expect(splitTurn({ id: "turn", status: "inProgress", items }, "claude").body).toEqual([
      { kind: "work", items, collapsed: true },
    ]);
  });

  it("keeps current reasoning visible after finished tools", () => {
    const items = [
      { type: "reasoning", id: "thought", summary: ["Earlier thought"] },
      { type: "dynamicToolCall", id: "read", status: "completed" },
      { type: "reasoning", id: "latest", summary: ["Still thinking"] },
    ];
    expect(splitTurn({ id: "turn", status: "inProgress", items }, "claude").body).toEqual([
      { kind: "work", items: items.slice(0, 2), collapsed: true },
      { kind: "work", items: items.slice(2) },
    ]);
  });

  it("keeps answered questions visible as message segments", () => {
    const turn: Turn = {
      id: "turn-1",
      status: "completed",
      items: [
        { type: "reasoning", id: "item_1", summary: ["Thinking"] },
        { type: "userInputAnswered", id: "item_2" },
        { type: "commandExecution", id: "item_3" },
        { type: "agentMessage", id: "item_4" },
      ],
    };
    const { body } = splitTurn(turn);
    expect(body.map((segment) => segment.kind)).toEqual(["work", "message", "work", "message"]);
    expect(body[1]).toEqual({ kind: "message", item: turn.items[1] });
  });

  it("does not open a work section for items that render nothing", () => {
    const turn: Turn = {
      id: "turn-1",
      status: "completed",
      items: [
        { type: "agentMessage", id: "item_1", text: "On it" },
        // Reasoning whose text never arrived, then a type this app does not
        // draw: a "Worked for Ns" header over these would expand to nothing.
        { type: "reasoning", id: "item_2", summary: [] },
        { type: "reasoning", id: "item_3", summary: ["  "] },
        { type: "somethingNewFromCodex", id: "item_4" },
        { type: "agentMessage", id: "item_5", text: "Done" },
      ],
    };
    expect(splitTurn(turn).body.map((segment) => segment.kind)).toEqual(["message", "message"]);
  });

  it("keeps one work section across an item it cannot draw", () => {
    const turn: Turn = {
      id: "turn-1",
      status: "completed",
      items: [
        { type: "commandExecution", id: "item_1" },
        { type: "somethingNewFromCodex", id: "item_2" },
        { type: "commandExecution", id: "item_3" },
      ],
    };
    const { body } = splitTurn(turn);
    expect(body).toHaveLength(1);
    expect(body[0].kind === "work" && body[0].items.map((item) => item.id)).toEqual(["item_1", "item_3"]);
  });
});

describe("rendersSomething", () => {
  it("accepts reasoning only once its text has arrived", () => {
    expect(rendersSomething({ type: "reasoning", id: "a" })).toBe(false);
    expect(rendersSomething({ type: "reasoning", id: "a", summary: ["", ""] })).toBe(false);
    expect(rendersSomething({ type: "reasoning", id: "a", summary: ["", "Weighing"] })).toBe(true);
  });

  it("accepts the item types the transcript draws", () => {
    expect(rendersSomething({ type: "subAgentActivity", id: "a", kind: "started" })).toBe(true);
    expect(rendersSomething({ type: "commandExecution", id: "a" })).toBe(true);
    expect(rendersSomething({ type: "imageView", id: "a" })).toBe(true);
  });
});

describe("turnSegments", () => {
  // The live path and the completed path have to agree about what is drawable,
  // or an unknown type leaves a gap mid-turn that vanishes when the turn ends.
  it("skips a type this app cannot draw", () => {
    const segments = turnSegments([
      { type: "commandExecution", id: "item_1" },
      { type: "somethingNewFromCodex", id: "item_2" },
      { type: "commandExecution", id: "item_3" },
    ]);
    expect(segments.map((segment) => segment.kind === "item" && segment.item.id)).toEqual(["item_1", "item_3"]);
  });

  // Unlike the completed path, though: an empty reasoning item mid-turn is what
  // the "Working…" shimmer hangs off, so it has to survive.
  it("keeps a reasoning item whose text has not arrived yet", () => {
    const segments = turnSegments([{ type: "reasoning", id: "item_1", summary: [] }]);
    expect(segments).toEqual([{ kind: "reasoning", items: [{ type: "reasoning", id: "item_1", summary: [] }] }]);
  });

  it("groups consecutive reasoning items and breaks the run on anything else", () => {
    const segments = turnSegments([
      { type: "reasoning", id: "r1", summary: ["a"] },
      { type: "reasoning", id: "r2", summary: ["b"] },
      { type: "commandExecution", id: "c1" },
      { type: "reasoning", id: "r3", summary: ["c"] },
    ]);
    expect(segments.map((segment) => segment.kind)).toEqual(["reasoning", "item", "reasoning"]);
  });
});

describe("functionCallOutputText", () => {
  it("reads a string output as-is and joins text parts, counting images", () => {
    expect(functionCallOutputText({ type: "functionCallOutput", id: "i", output: "42" })).toBe("42");
    expect(
      functionCallOutputText({
        type: "functionCallOutput",
        id: "i",
        output: [{ type: "inputText", text: "a" }, { type: "inputImage" }, { type: "inputText", text: "b" }],
      }),
    ).toBe("a\nb\n[1 image]");
    expect(functionCallOutputText({ type: "functionCallOutput", id: "i" })).toBe("");
  });
});

describe("workedLabel", () => {
  const worked = (durationMs: number) => workedLabel({ id: "t", status: "completed", items: [], durationMs } as Turn);

  it("never shows 60 seconds", () => {
    expect(worked(59_700)).toBe("Worked for 1m 00s");
    expect(worked(119_700)).toBe("Worked for 2m 00s");
    expect(worked(61_000)).toBe("Worked for 1m 01s");
    expect(worked(42_400)).toBe("Worked for 42s");
    expect(worked(300)).toBe("Worked for 300ms");
  });
});
