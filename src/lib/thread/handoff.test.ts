import { describe, expect, it } from "vitest";
import { canHandoff, cwdBelongsTo, dirName, handoffHomeIssue, shortHomeName } from "$lib/thread/handoff";
import type { HandoffOpen } from "$lib/types";

function open(overrides: Partial<HandoffOpen> = {}): HandoffOpen {
  return {
    kind: "thread",
    threadId: "t1",
    path: "/repo/wt",
    requestedHome: "/home/.codex-work",
    label: null,
    homeMatches: false,
    homeExists: true,
    ...overrides,
  };
}

describe("shortHomeName", () => {
  it("uses the final path segment", () => {
    expect(shortHomeName("/Users/me/.codex-personal")).toBe(".codex-personal");
    expect(shortHomeName("~/.codex")).toBe(".codex");
  });
  it("tolerates trailing slashes and empties", () => {
    expect(shortHomeName("/home/.codex/")).toBe(".codex");
    expect(shortHomeName(null)).toBe("home");
  });
});

describe("dirName", () => {
  it("returns the last directory segment", () => {
    expect(dirName("/repo/wt-feature")).toBe("wt-feature");
    expect(dirName("/repo/wt/")).toBe("wt");
    expect(dirName(null)).toBe("");
  });
});

describe("cwdBelongsTo", () => {
  it("matches equal and nested paths", () => {
    expect(cwdBelongsTo("/repo/wt", "/repo/wt")).toBe(true);
    expect(cwdBelongsTo("/repo/wt/sub", "/repo/wt")).toBe(true);
    expect(cwdBelongsTo("/repo/wt/", "/repo/wt")).toBe(true);
  });
  it("rejects sibling and unrelated paths", () => {
    expect(cwdBelongsTo("/repo/other", "/repo/wt")).toBe(false);
    // A prefix that is not a path boundary must not match.
    expect(cwdBelongsTo("/repo/wt-2", "/repo/wt")).toBe(false);
  });
  it("treats a missing requested path as no constraint", () => {
    expect(cwdBelongsTo("/anything", null)).toBe(true);
    expect(cwdBelongsTo(null, "/repo")).toBe(false);
  });
});

describe("canHandoff", () => {
  it("requires a thread id, cwd, and home", () => {
    expect(canHandoff("t1", "/repo", "/home/.codex")).toBe(true);
    expect(canHandoff(null, "/repo", "/home/.codex")).toBe(false);
    expect(canHandoff("t1", "", "/home/.codex")).toBe(false);
    expect(canHandoff("t1", "/repo", null)).toBe(false);
  });
});

describe("handoffHomeIssue", () => {
  it("is null when the home matches", () => {
    expect(handoffHomeIssue(open({ homeMatches: true }))).toBeNull();
  });
  it("is null when the home differs but exists (a switch is offered)", () => {
    expect(handoffHomeIssue(open({ homeMatches: false, homeExists: true }))).toBeNull();
  });
  it("names the missing home when it does not exist", () => {
    const issue = handoffHomeIssue(open({ homeMatches: false, homeExists: false }));
    expect(issue).toContain("/home/.codex-work");
    expect(issue).toContain("not found");
  });
});
