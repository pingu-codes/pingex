import { describe, expect, it } from "vitest";
import type { Project, WorktreeEntry } from "$lib/types";
import {
  aheadBehindLabel,
  folderName,
  isDirty,
  isTempWorktreePath,
  statusSummary,
  stripRemotePrefix,
  tempWorktreeLocation,
  threadCountForPath,
  worktreeCards,
  worktreeProblem,
} from "$lib/worktrees/worktrees";

function entry(overrides: Partial<WorktreeEntry> = {}): WorktreeEntry {
  return {
    path: "/repo/wt",
    head: "abcdef1234567890",
    branch: "feature",
    detached: false,
    bare: false,
    locked: false,
    lockReason: null,
    prunable: false,
    prunableReason: null,
    isMain: false,
    isCodexManaged: false,
    missingDir: false,
    branchCheckedOutElsewhere: false,
    upstream: null,
    ahead: 0,
    behind: 0,
    status: { staged: 0, unstaged: 0, untracked: 0, conflicted: 0 },
    state: null,
    ...overrides,
  };
}

describe("worktree helpers", () => {
  it("strips the remote from a remote-tracking branch name", () => {
    expect(stripRemotePrefix("origin/feat/x")).toBe("feat/x");
    expect(stripRemotePrefix("origin/main")).toBe("main");
    expect(stripRemotePrefix("main")).toBe("main");
    expect(stripRemotePrefix("/odd")).toBe("/odd");
  });

  it("derives folder names and status summaries", () => {
    expect(folderName("/a/b/c/")).toBe("c");
    expect(folderName("/a/b/c")).toBe("c");
    expect(statusSummary(null)).toBe("Status unavailable");
    expect(statusSummary({ staged: 0, unstaged: 0, untracked: 0, conflicted: 0 })).toBe("Clean");
    expect(statusSummary({ staged: 1, unstaged: 2, untracked: 0, conflicted: 1 })).toBe("3 changed · 1 conflict");
  });

  it("keeps temporary worktrees in a restart-safe, repository-scoped Codex location", () => {
    expect(tempWorktreeLocation("/home/me/.codex/", "/projects/example/", "abc123")).toBe(
      "/home/me/.codex/worktrees-tmp/example/abc123",
    );
    expect(isTempWorktreePath("/home/me/.codex/worktrees-tmp/example/abc123")).toBe(true);
    expect(isTempWorktreePath("/home/me/.codex/worktrees/example/feature")).toBe(false);
    expect(isTempWorktreePath("/projects/example")).toBe(false);
  });

  it("summarises ahead/behind and dirtiness", () => {
    expect(aheadBehindLabel(0, 0)).toBeNull();
    expect(aheadBehindLabel(2, 0)).toBe("↑2");
    expect(aheadBehindLabel(2, 1)).toBe("↑2 ↓1");
    expect(isDirty(null)).toBe(false);
    expect(isDirty({ staged: 0, unstaged: 0, untracked: 1, conflicted: 0 })).toBe(true);
  });

  it("gives each acceptance state a distinct problem message", () => {
    expect(worktreeProblem(entry())).toBeNull();
    expect(worktreeProblem(entry({ missingDir: true }))).toMatch(/missing/i);
    expect(worktreeProblem(entry({ detached: true, branch: null }))).toMatch(/detached/i);
    expect(worktreeProblem(entry({ branchCheckedOutElsewhere: true }))).toMatch(/another worktree/i);
    expect(worktreeProblem(entry({ prunable: true, prunableReason: "gitdir gone" }))).toMatch(/stale/i);
  });

  it("counts threads whose cwd lives under the worktree path", () => {
    const projects: Project[] = [
      {
        path: "/repo",
        name: "repo",
        kind: "folder",
        workspaceId: null,
        archived: false,
        instructions: "",
        sources: [],
        pinned: false,
        expanded: true,
        threads: [
          {
            id: "1",
            cwd: "/repo/wt",
            title: "a",
            updatedAt: 0,
            status: "idle",
            pinned: false,
            parentThreadId: null,
            agentNickname: null,
            agentRole: null,
            projectId: null,
            sectionId: null,
            subagentCount: 0,
          },
          {
            id: "2",
            cwd: "/repo/wt/sub",
            title: "b",
            updatedAt: 0,
            status: "idle",
            pinned: false,
            parentThreadId: null,
            agentNickname: null,
            agentRole: null,
            projectId: null,
            sectionId: null,
            subagentCount: 0,
          },
          {
            id: "3",
            cwd: "/repo/other",
            title: "c",
            updatedAt: 0,
            status: "idle",
            pinned: false,
            parentThreadId: null,
            agentNickname: null,
            agentRole: null,
            projectId: null,
            sectionId: null,
            subagentCount: 0,
          },
        ],
      },
    ];
    expect(threadCountForPath(projects, "/repo/wt")).toBe(2);
    expect(threadCountForPath(projects, "/repo/other")).toBe(1);
  });

  it("builds card models with branch labels for detached HEAD", () => {
    const cards = worktreeCards(
      [entry({ path: "/repo/main", isMain: true, branch: "main", ahead: 3 }), entry({ detached: true, branch: null })],
      [],
    );
    expect(cards[0].displayName).toBe("main");
    expect(cards[0].branchLabel).toBe("main");
    expect(cards[0].aheadBehind).toBe("↑3");
    expect(cards[1].branchLabel).toBe("detached @ abcdef1");
  });
});
