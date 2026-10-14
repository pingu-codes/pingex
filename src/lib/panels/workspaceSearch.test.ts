import { describe, expect, it } from "vitest";
import type { WorkspaceSearchResults } from "$lib/types";
import { isEmptyResults } from "./workspaceSearch";

const emptyGroup = { items: [], nextCursor: null, hasMore: false };

function results(overrides: Partial<WorkspaceSearchResults> = {}): WorkspaceSearchResults {
  return {
    projectFiles: { ...emptyGroup },
    threads: { ...emptyGroup },
    messages: { ...emptyGroup },
    generation: 0,
    ...overrides,
  };
}

describe("workspace search helpers", () => {
  it("treats null and all-empty groups as empty", () => {
    expect(isEmptyResults(null)).toBe(true);
    expect(isEmptyResults(results())).toBe(true);
  });

  it("treats a result in any group as non-empty", () => {
    const populated = results({
      projectFiles: {
        items: [{ path: "a.ts", fileName: "a.ts", lineNumber: 1, preview: "x", nameMatch: false }],
        nextCursor: null,
        hasMore: false,
      },
      threads: {
        items: [{ threadId: "t1", title: "Login", cwd: "/p" }],
        nextCursor: "2",
        hasMore: true,
      },
    });
    expect(isEmptyResults(populated)).toBe(false);
  });
});
