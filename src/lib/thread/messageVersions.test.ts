import { describe, expect, it } from "vitest";
import {
  groupForTurn,
  isPendingEditTurn,
  newestLeaf,
  rootThreadId,
  versionsForTurn,
} from "$lib/thread/messageVersions";
import type { ThreadBranch } from "$lib/types";

function branch(overrides: Partial<ThreadBranch> & Pick<ThreadBranch, "threadId" | "parentThreadId">): ThreadBranch {
  return {
    groupTurnId: "turn-2",
    replacedTurnId: "turn-2",
    inheritedTurns: 1,
    editTurnId: null,
    createdAt: 1,
    updatedAt: null,
    ...overrides,
  };
}

describe("versionsForTurn", () => {
  it("is null for a message that was never edited", () => {
    expect(versionsForTurn("turn-2", [], "root")).toBeNull();
    expect(versionsForTurn("turn-9", [branch({ threadId: "fork-1", parentThreadId: "root" })], "root")).toBeNull();
  });

  it("shows the original as 1 of 2 and the edit as 2 of 2", () => {
    const branches = [branch({ threadId: "fork-1", parentThreadId: "root", editTurnId: "turn-2b" })];
    expect(versionsForTurn("turn-2", branches, "root")).toEqual({
      index: 0,
      count: 2,
      prevThreadId: null,
      nextThreadId: "fork-1",
    });
    expect(versionsForTurn("turn-2b", branches, "fork-1")).toEqual({
      index: 1,
      count: 2,
      prevThreadId: "root",
      nextThreadId: null,
    });
  });

  it("keeps an edit of an edit in the original's group, in creation order", () => {
    const branches = [
      branch({
        threadId: "fork-2",
        parentThreadId: "fork-1",
        replacedTurnId: "turn-2b",
        editTurnId: "turn-2c",
        createdAt: 2,
      }),
      branch({ threadId: "fork-1", parentThreadId: "root", editTurnId: "turn-2b", createdAt: 1 }),
    ];
    expect(versionsForTurn("turn-2b", branches, "fork-1")).toMatchObject({
      index: 1,
      count: 3,
      nextThreadId: "fork-2",
    });
    expect(versionsForTurn("turn-2c", branches, "fork-2")).toMatchObject({
      index: 2,
      count: 3,
      prevThreadId: "fork-1",
    });
    expect(groupForTurn("turn-2c", branches)).toBe("turn-2");
    expect(groupForTurn("turn-7", branches)).toBe("turn-7");
  });

  it("places an inherited edit turn by its id wherever it is shown", () => {
    // fork-later branched from fork-1 after the edit, so it still contains turn-2b.
    const branches = [
      branch({ threadId: "fork-1", parentThreadId: "root", editTurnId: "turn-2b", createdAt: 1 }),
      branch({
        threadId: "fork-later",
        parentThreadId: "fork-1",
        groupTurnId: "turn-5",
        replacedTurnId: "turn-5",
        createdAt: 2,
      }),
    ];
    expect(versionsForTurn("turn-2b", branches, "fork-later")).toMatchObject({ index: 1, count: 2 });
  });

  it("falls back to the thread on show while a fork's edit turn is unknown", () => {
    const branches = [branch({ threadId: "fork-1", parentThreadId: "root" })];
    expect(versionsForTurn("local-1", branches, "fork-1")).toMatchObject({ index: 1, count: 2, prevThreadId: "root" });
    expect(versionsForTurn("local-1", branches, "root")).toBeNull();
    expect(isPendingEditTurn("fork-1", 1, branches)).toBe(true);
    expect(isPendingEditTurn("fork-1", 0, branches)).toBe(false);
    expect(isPendingEditTurn("root", 1, branches)).toBe(false);
  });
});

describe("rootThreadId", () => {
  it("walks up through nested branches", () => {
    const branches = [
      branch({ threadId: "fork-1", parentThreadId: "root" }),
      branch({ threadId: "fork-2", parentThreadId: "fork-1" }),
    ];
    expect(rootThreadId("fork-2", branches)).toBe("root");
    expect(rootThreadId("root", branches)).toBe("root");
    expect(rootThreadId(null, branches)).toBeNull();
  });
});

describe("newestLeaf", () => {
  const branches = [
    branch({ threadId: "fork-1", parentThreadId: "root", updatedAt: 10, createdAt: 1 }),
    branch({ threadId: "fork-2", parentThreadId: "fork-1", updatedAt: 30, createdAt: 2 }),
    branch({ threadId: "fork-3", parentThreadId: "root", updatedAt: 20, createdAt: 3 }),
  ];

  it("picks the most recently active thread in the family", () => {
    expect(newestLeaf("root", branches)).toBe("fork-2");
    expect(newestLeaf("fork-3", branches)).toBe("fork-3");
  });

  it("lets the root win when it was active last", () => {
    expect(newestLeaf("root", branches, () => 40)).toBe("root");
  });

  it("stays put without any activity to compare", () => {
    const unknown = branches.map((b) => ({ ...b, updatedAt: null }));
    expect(newestLeaf("root", unknown)).toBe("root");
  });
});
