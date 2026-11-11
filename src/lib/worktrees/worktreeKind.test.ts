import { FolderGit2, GitPullRequestArrow, SquareStack } from "@lucide/svelte";
import { describe, expect, it } from "vitest";
import type { WorktreePurpose } from "$lib/types";
import { worktreeKind } from "$lib/worktrees/worktreeKind";

const stack: WorktreePurpose = {
  kind: "stack",
  repoDir: "/repo",
  number: 7,
  name: "billing",
  baseRef: "main",
  prs: [
    { number: 101, title: "Add the ledger", headRef: "billing/ledger" },
    { number: 102, title: "Charge from the ledger", headRef: "billing/charge" },
  ],
};

describe("worktreeKind", () => {
  it("gives a managed PR worktree its own icon and says who looks after it", () => {
    const kind = worktreeKind("worktree", { kind: "managedPr", repoDir: "/repo", number: 57 });
    expect(kind.icon).toBe(GitPullRequestArrow);
    expect(kind.tooltip).toBe(
      "PR worktree for #57. Pingex manages it and removes it once the PR has closed and it holds no work.",
    );
  });

  it("gives a stack worktree its own icon and names the stack and its PRs", () => {
    const kind = worktreeKind("worktree", stack);
    expect(kind.icon).toBe(SquareStack);
    expect(kind.tooltip).toBe('Stack worktree for "billing": #101, #102. Switching PR switches its branch.');
  });

  it("leaves any other worktree, and anything that is not one, as it was", () => {
    expect(worktreeKind("worktree", null)).toMatchObject({ icon: FolderGit2, tooltip: "Codex worktree" });
    expect(worktreeKind("folder", stack).tooltip).toBeNull();
    expect(worktreeKind("multiProject", null).tooltip).toBeNull();
  });
});
