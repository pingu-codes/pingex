import { Folder, FolderGit2, GitPullRequestArrow, Layers3, SquareStack } from "@lucide/svelte";
import type { WorktreePurpose } from "$lib/types";

export interface WorktreeKind {
  icon: typeof Folder;
  /** What the icon means; null for a plain folder or workspace, which need no gloss. */
  tooltip: string | null;
  /** A short name for badges and the context pill. */
  label: string;
}

/** The icon and explanation for a sidebar entry: a folder, a multi-project
 *  workspace, or a worktree, told apart by what the worktree is for. */
export function worktreeKind(kind: string, purpose: WorktreePurpose | null | undefined): WorktreeKind {
  if (kind === "multiProject") return { icon: Layers3, tooltip: null, label: "Workspace" };
  if (kind !== "worktree") return { icon: Folder, tooltip: null, label: "Folder" };
  if (purpose?.kind === "managedPr") {
    return {
      icon: GitPullRequestArrow,
      tooltip: `PR worktree for #${purpose.number}. Pingex manages it and removes it once the PR has closed and it holds no work.`,
      label: `PR #${purpose.number}`,
    };
  }
  if (purpose?.kind === "stack") {
    const prs = purpose.prs.map((pr) => `#${pr.number}`).join(", ");
    return {
      icon: SquareStack,
      tooltip: `Stack worktree for "${purpose.name}"${prs ? `: ${prs}` : ""}. Switching PR switches its branch.`,
      label: "Stack",
    };
  }
  return { icon: FolderGit2, tooltip: "Codex worktree", label: "Worktree" };
}
