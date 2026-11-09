// @vitest-environment node
import { describe, expect, it } from "vitest";
import { FindingsStore } from "$lib/review/findings.svelte";
import type { PendingReviewStore } from "$lib/review/pendingReview.svelte";
import type { Finding } from "$lib/types";

function finding(id: string, overrides: Partial<Finding> = {}): Finding {
  return {
    id,
    agentReviewId: "run",
    prNumber: 1,
    headSha: "abc",
    path: "src/a.ts",
    line: 3,
    endLine: null,
    side: "RIGHT",
    severity: "medium",
    title: id,
    body: "",
    evidence: null,
    sameAsCommentId: null,
    state: "draft",
    dismissReason: null,
    dismissNote: null,
    ...overrides,
  };
}

describe("FindingsStore line index", () => {
  const store = new FindingsStore("/repo", 1, {} as PendingReviewStore);
  store.findings = [
    finding("a"),
    finding("b", { line: 1, endLine: 5 }),
    finding("c", { side: "LEFT" }),
    finding("d", { state: "dismissed" }),
    finding("e", { path: "src/b.ts" }),
  ];

  it("finds the Findings under a line: anchored at their last line, not dismissed", () => {
    expect(store.at("src/a.ts", "RIGHT", 3).map((entry) => entry.id)).toEqual(["a"]);
    expect(store.at("src/a.ts", "RIGHT", 5).map((entry) => entry.id)).toEqual(["b"]);
    expect(store.at("src/a.ts", "LEFT", 3).map((entry) => entry.id)).toEqual(["c"]);
    expect(store.at("src/a.ts", "RIGHT", 1)).toEqual([]);
  });

  it("lists the lines per path that hold a Finding", () => {
    expect([...(store.linesByPath.get("src/a.ts") ?? [])].sort()).toEqual(["LEFT:3", "RIGHT:3", "RIGHT:5"]);
    expect([...(store.linesByPath.get("src/b.ts") ?? [])]).toEqual(["RIGHT:3"]);
  });
});
