import { describe, expect, it } from "vitest";
import type { StatusFile } from "$lib/types";
import { groupStatusFiles, selectionAfterRefresh } from "./gitStatusFiles";

const file = (path: string, state: string, code: string): StatusFile => ({ path, state, code });

describe("groupStatusFiles", () => {
  it("puts a path in every section its code names", () => {
    const groups = groupStatusFiles([
      file("a.ts", "staged", "M."),
      file("b.ts", "unstaged", ".M"),
      file("c.ts", "staged", "MM"),
      file("d.ts", "untracked", ""),
      file("e.ts", "conflicted", "UU"),
      file("f.ts", "ignored", ""),
    ]);
    expect(groups.staged.map((r) => r.path)).toEqual(["a.ts", "c.ts"]);
    expect(groups.unstaged.map((r) => r.path)).toEqual(["b.ts", "c.ts"]);
    expect(groups.untracked.map((r) => r.path)).toEqual(["d.ts"]);
    expect(groups.conflicted.map((r) => r.path)).toEqual(["e.ts"]);
    expect(groups.staged[0].change).toBe("M");
    expect(groups.untracked[0].change).toBe("?");
  });
});

describe("selectionAfterRefresh", () => {
  const groups = groupStatusFiles([file("a.ts", "unstaged", ".M"), file("b.ts", "staged", "A.")]);

  it("keeps a row that still exists", () => {
    expect(selectionAfterRefresh({ path: "a.ts", section: "unstaged" }, groups)).toEqual({
      path: "a.ts",
      section: "unstaged",
    });
  });

  it("follows a path that moved sections", () => {
    expect(selectionAfterRefresh({ path: "a.ts", section: "staged" }, groups)).toEqual({
      path: "a.ts",
      section: "unstaged",
    });
  });

  it("drops a path that is gone", () => {
    expect(selectionAfterRefresh({ path: "z.ts", section: "staged" }, groups)).toBeNull();
    expect(selectionAfterRefresh(null, groups)).toBeNull();
  });
});
