// Builders for the records the preview fixtures and the tests fill in by the
// dozen. Each starts from a plain, idle record and takes the fields that
// matter to the caller.

import type { InboxPr, ThreadSummary, WorktreeEntry } from "$lib/types";

/** A clean linked worktree on branch `feature`. */
export function worktreeEntry(overrides: Partial<WorktreeEntry> = {}): WorktreeEntry {
  return {
    path: "/repo/wt-feature",
    head: "abcdef1",
    branch: "feature",
    detached: false,
    bare: false,
    locked: false,
    lockReason: null,
    prunable: false,
    prunableReason: null,
    isMain: false,
    isCodexManaged: false,
    isTemp: false,
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

/** An idle, unpinned top-level thread updated just now. */
export function threadSummary(overrides: Partial<ThreadSummary> & Pick<ThreadSummary, "id" | "title">): ThreadSummary {
  return {
    cwd: "",
    updatedAt: Date.now() / 1000,
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
    ...overrides,
  };
}

/** An open, green PR by `ciaran` on `feature` into `main`, nobody asked to review. */
export function inboxPr(overrides: Partial<InboxPr> & Pick<InboxPr, "number" | "title">): InboxPr {
  const sha = (seed: number) => String(seed).repeat(40).slice(0, 40);
  return {
    author: "ciaran",
    url: `https://github.com/ciaran/codex-custom/pull/${overrides.number}`,
    isDraft: false,
    createdAt: "2026-11-03T09:00:00Z",
    updatedAt: "2026-11-05T09:00:00Z",
    headRef: "feature",
    headSha: sha(overrides.number),
    baseRef: "main",
    baseSha: sha(1),
    headRepo: null,
    maintainerCanModify: false,
    reviewDecision: null,
    reviewRequests: [],
    reviews: [],
    checks: "SUCCESS",
    unresolvedThreads: 0,
    additions: 40,
    deletions: 8,
    changedFiles: 3,
    ...overrides,
  };
}
