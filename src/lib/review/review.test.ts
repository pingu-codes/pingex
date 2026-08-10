import { describe, expect, it } from "vitest";
import {
  addableLines,
  changeStat,
  checksFailing,
  checksLabel,
  commentThreads,
  conversationComments,
  fileChange,
  reviewPrompt,
  staleBanner,
  threadsForFile,
} from "$lib/review/review";
import type { PrComment, PrFile, PrFreshness } from "$lib/types";

function file(overrides: Partial<PrFile> = {}): PrFile {
  return {
    path: "src/a.ts",
    oldPath: null,
    status: "modified",
    additions: 2,
    deletions: 1,
    patch: "@@ -1,2 +1,3 @@\n ctx\n-old\n+new\n+extra",
    patchTruncated: false,
    hunks: [
      {
        header: "@@ -1,2 +1,3 @@",
        oldStart: 1,
        oldLines: 2,
        newStart: 1,
        newLines: 3,
        lines: [
          { kind: "context", content: "ctx", oldLine: 1, newLine: 1 },
          { kind: "del", content: "old", oldLine: 2, newLine: null },
          { kind: "add", content: "new", oldLine: null, newLine: 2 },
          { kind: "add", content: "extra", oldLine: null, newLine: 3 },
        ],
      },
    ],
    ...overrides,
  };
}

function comment(overrides: Partial<PrComment> = {}): PrComment {
  return {
    id: 1,
    author: "dev",
    body: "hi",
    createdAt: "t",
    path: null,
    line: null,
    side: null,
    threadId: null,
    isResolved: false,
    ...overrides,
  };
}

describe("review helpers", () => {
  it("maps file status to a DiffBlock change kind", () => {
    expect(fileChange(file()).kind.type).toBe("update");
    expect(fileChange(file({ status: "added" })).kind.type).toBe("add");
    expect(fileChange(file({ status: "removed" })).kind.type).toBe("delete");
    expect(fileChange(file({ status: "renamed", oldPath: "old.ts" })).kind).toEqual({
      type: "rename",
      movePath: "old.ts",
    });
  });

  it("summarises change stats", () => {
    expect(changeStat(file({ additions: 5, deletions: 2 }))).toBe("+5 −2");
    expect(changeStat(file({ additions: 3, deletions: 0 }))).toBe("+3");
    expect(changeStat(file({ additions: 0, deletions: 0 }))).toBe("no change");
  });

  it("lists addable lines with correct side and anchors", () => {
    const lines = addableLines(file());
    // context (RIGHT), del (LEFT), two adds (RIGHT)
    expect(lines.map((l) => l.side)).toEqual(["RIGHT", "LEFT", "RIGHT", "RIGHT"]);
    expect(lines[1]).toMatchObject({ side: "LEFT", line: 2, anchor: "line:src/a.ts:LEFT:2" });
    expect(lines[3]).toMatchObject({ side: "RIGHT", line: 3, anchor: "line:src/a.ts:RIGHT:3" });
  });

  it("groups inline comments into threads by threadId then path:line", () => {
    const comments: PrComment[] = [
      comment({ id: 1, path: "a.ts", line: 4, side: "RIGHT", threadId: "T1" }),
      comment({ id: 2, path: "a.ts", line: 4, side: "RIGHT", threadId: "T1", isResolved: true }),
      comment({ id: 3, path: "b.ts", line: 9, side: "RIGHT", threadId: null }),
      comment({ id: 4, path: null }),
    ];
    const threads = commentThreads(comments);
    expect(threads).toHaveLength(2);
    const t1 = threads.find((t) => t.key === "T1")!;
    expect(t1.comments.map((c) => c.id)).toEqual([1, 2]);
    // A resolved marker on any comment resolves the whole thread.
    expect(t1.resolved).toBe(true);
    const t2 = threads.find((t) => t.path === "b.ts")!;
    expect(t2.key).toBe("b.ts:9:RIGHT");
    expect(t2.resolved).toBe(false);
  });

  it("filters threads for a file and separates conversation comments", () => {
    const comments: PrComment[] = [
      comment({ id: 1, path: "a.ts", line: 8, threadId: "T1" }),
      comment({ id: 2, path: "a.ts", line: 2, threadId: "T2" }),
      comment({ id: 3, path: null, body: "general" }),
    ];
    const forA = threadsForFile(comments, "a.ts");
    // Sorted by line ascending.
    expect(forA.map((t) => t.line)).toEqual([2, 8]);
    expect(conversationComments(comments).map((c) => c.id)).toEqual([3]);
  });

  it("renders check labels and failure state", () => {
    expect(checksLabel(null)).toBeNull();
    expect(checksLabel({ total: 0, passing: 0, failing: 0, pending: 0 })).toBeNull();
    expect(checksLabel({ total: 4, passing: 3, failing: 0, pending: 1 })).toBe("3 passing · 1 pending");
    expect(checksFailing({ total: 2, passing: 1, failing: 1, pending: 0 })).toBe(true);
    expect(checksFailing({ total: 2, passing: 2, failing: 0, pending: 0 })).toBe(false);
  });

  it("shows the stale banner only on drift", () => {
    expect(staleBanner(null)).toBeNull();
    const fresh: PrFreshness = { stale: false, remoteHead: "abc", remoteUpdatedAt: "t" };
    expect(staleBanner(fresh)).toBeNull();
    const stale: PrFreshness = { stale: true, remoteHead: "deadbeefcafe", remoteUpdatedAt: "t2" };
    expect(staleBanner(stale)).toMatch(/changed on the remote/);
    expect(staleBanner(stale)).toContain("deadbee");
  });

  it("builds a Codex review prompt embedding the diffs", () => {
    const prompt = reviewPrompt("Fix loader", "main", "fix", [file(), file({ patchTruncated: true, patch: "" })]);
    expect(prompt).toContain("Fix loader");
    expect(prompt).toContain("fix → main");
    expect(prompt).toContain("src/a.ts");
    // Truncated patches are omitted from the prompt.
    expect(prompt.match(/src\/a\.ts/g)?.length).toBe(1);
  });
});
