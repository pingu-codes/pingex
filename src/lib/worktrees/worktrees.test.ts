// @vitest-environment node
import { describe, expect, it } from "vitest";
import { worktreeEntry } from "$lib/services/preview/factories";
import type { Project } from "$lib/types";
import {
  adoptableWorktrees,
  aheadBehindLabel,
  isDirty,
  prNumberOf,
  statusSummary,
  stripRemotePrefix,
  threadCountForPath,
  worktreeCards,
  worktreeProblem,
} from "$lib/worktrees/worktrees";

describe("worktree helpers", () => {
  it("strips the remote from a remote-tracking branch name", () => {
    expect(stripRemotePrefix("origin/feat/x")).toBe("feat/x");
    expect(stripRemotePrefix("origin/main")).toBe("main");
    expect(stripRemotePrefix("main")).toBe("main");
    expect(stripRemotePrefix("/odd")).toBe("/odd");
  });

  it("derives status summaries", () => {
    expect(statusSummary(null)).toBe("Status unavailable");
    expect(statusSummary({ staged: 0, unstaged: 0, untracked: 0, conflicted: 0 })).toBe("Clean");
    expect(statusSummary({ staged: 1, unstaged: 2, untracked: 0, conflicted: 1 })).toBe("3 changed · 1 conflict");
  });

  it("summarises ahead/behind and dirtiness", () => {
    expect(aheadBehindLabel(0, 0)).toBeNull();
    expect(aheadBehindLabel(2, 0)).toBe("↑2");
    expect(aheadBehindLabel(2, 1)).toBe("↑2 ↓1");
    expect(isDirty(null)).toBe(false);
    expect(isDirty({ staged: 0, unstaged: 0, untracked: 1, conflicted: 0 })).toBe(true);
  });

  it("gives each acceptance state a distinct problem message", () => {
    expect(worktreeProblem(worktreeEntry())).toBeNull();
    expect(worktreeProblem(worktreeEntry({ missingDir: true }))).toMatch(/missing/i);
    expect(worktreeProblem(worktreeEntry({ detached: true, branch: null }))).toMatch(/detached/i);
    expect(worktreeProblem(worktreeEntry({ branchCheckedOutElsewhere: true }))).toMatch(/another worktree/i);
    expect(worktreeProblem(worktreeEntry({ prunable: true, prunableReason: "gitdir gone" }))).toMatch(/stale/i);
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
            hidden: false,
            inTempWorktree: false,
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
            hidden: false,
            inTempWorktree: false,
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
            hidden: false,
            inTempWorktree: false,
          },
        ],
      },
    ];
    expect(threadCountForPath(projects, "/repo/wt")).toBe(2);
    expect(threadCountForPath(projects, "/repo/other")).toBe(1);
  });

  it("builds card models with branch labels for detached HEAD", () => {
    const cards = worktreeCards(
      [
        worktreeEntry({ path: "/repo/main", isMain: true, branch: "main", ahead: 3 }),
        worktreeEntry({ detached: true, branch: null }),
      ],
      [],
    );
    expect(cards[0].displayName).toBe("main");
    expect(cards[0].branchLabel).toBe("main");
    expect(cards[0].aheadBehind).toBe("↑3");
    expect(cards[1].branchLabel).toBe("detached @ abcdef1");
  });

  it("lists managed PR worktrees with their PR, since they can hold real work", () => {
    const managed = worktreeEntry({
      path: "/home/.codex/pr-worktrees/repo/pr-9",
      isManagedPrWorktree: true,
      branch: "pr/9",
    });
    const adopted = worktreeEntry({
      path: "/home/.codex/review-checkouts/repo/pr-8",
      isManagedPrWorktree: true,
      branch: "agent-experiment",
    });
    const plain = worktreeEntry({ path: "/repo/pr-7", branch: "pr/7" });
    const cards = worktreeCards([worktreeEntry({ path: "/repo", isMain: true }), managed, adopted, plain], []);

    expect(cards.map((card) => card.entry.path)).toEqual(["/repo", managed.path, adopted.path, plain.path]);
    expect(prNumberOf(managed)).toBe(9);
    expect(prNumberOf(adopted)).toBe(8);
    // What Pingex recorded wins over what the names suggest.
    const recorded = { ...adopted, purpose: { kind: "managedPr" as const, repoDir: "/repo", number: 12 } };
    expect(prNumberOf(recorded)).toBe(12);
    expect(prNumberOf(plain)).toBeNull();
  });

  it("offers only linked, present, permanent worktrees not yet in the sidebar", () => {
    const listed = { path: "/repo/listed/" } as Project;
    const entries = [
      worktreeEntry({ path: "/repo", isMain: true }),
      worktreeEntry({ path: "/repo/gone", missingDir: true }),
      worktreeEntry({ path: "/repo/stale", prunable: true }),
      worktreeEntry({ path: "/home/.codex/worktrees-tmp/repo/tmp", isTemp: true }),
      worktreeEntry({ path: "/home/.codex/pr-worktrees/repo/pr-9", isManagedPrWorktree: true }),
      worktreeEntry({ path: "/repo/listed" }),
      worktreeEntry({ path: "/repo/feature" }),
    ];
    expect(adoptableWorktrees(entries, [listed]).map((e) => e.path)).toEqual(["/repo/feature"]);
  });
});
