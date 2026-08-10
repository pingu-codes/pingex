import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { WorkspaceSearchResults } from "$lib/types";
import { debounce, emptyStateLabel, isEmptyResults, totalMatchCount } from "./workspaceSearch";

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
    expect(totalMatchCount(null)).toBe(0);
  });

  it("counts matches across every group", () => {
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
    expect(totalMatchCount(populated)).toBe(2);
  });

  it("builds a trimmed empty-state label", () => {
    expect(emptyStateLabel("  svelte  ")).toBe('No matches for "svelte"');
  });
});

describe("debounce", () => {
  // Only fake the timer functions; jsdom's requestAnimationFrame is read-only.
  beforeEach(() => vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] }));
  afterEach(() => vi.useRealTimers());

  it("runs only the last call after the wait elapses", () => {
    const spy = vi.fn();
    const run = debounce(spy, 200);
    run("a");
    run("b");
    run("c");
    expect(spy).not.toHaveBeenCalled();
    vi.advanceTimersByTime(200);
    expect(spy).toHaveBeenCalledTimes(1);
    expect(spy).toHaveBeenCalledWith("c");
  });

  it("cancel() drops a pending call", () => {
    const spy = vi.fn();
    const run = debounce(spy, 200);
    run("a");
    run.cancel();
    vi.advanceTimersByTime(500);
    expect(spy).not.toHaveBeenCalled();
  });
});
