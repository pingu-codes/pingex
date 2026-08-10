import type { Project, StatusCounts, WorktreeEntry } from "$lib/types";

/** The final path segment, used as a fallback display name. */
export function folderName(path: string): string {
  const trimmed = path.replace(/\/+$/, "");
  const index = trimmed.lastIndexOf("/");
  return index >= 0 ? trimmed.slice(index + 1) : trimmed;
}

/**
 * Where a temporary thread worktree lives: under the Codex home (so it
 * survives restarts, unlike the OS temp dir) but in `worktrees-tmp/` so it is
 * never mistaken for a permanent Codex worktree. Mirrors the permanent
 * `worktrees/<group>/<name>` two-level layout.
 */
export function tempWorktreeLocation(codexHome: string, repoDir: string, name: string): string {
  const home = codexHome.replace(/\/+$/, "");
  return `${home}/worktrees-tmp/${folderName(repoDir)}/${name}`;
}

export function isDirty(counts: StatusCounts | null): boolean {
  if (!counts) return false;
  return counts.staged + counts.unstaged + counts.untracked + counts.conflicted > 0;
}

/** A concise working-tree summary like "Clean" or "3 changed · 1 conflict". */
export function statusSummary(counts: StatusCounts | null): string {
  if (!counts) return "Status unavailable";
  const changed = counts.staged + counts.unstaged + counts.untracked;
  const parts: string[] = [];
  if (changed > 0) parts.push(`${changed} changed`);
  if (counts.conflicted > 0) parts.push(`${counts.conflicted} conflict${counts.conflicted === 1 ? "" : "s"}`);
  return parts.length > 0 ? parts.join(" · ") : "Clean";
}

/** "↑2 ↓1", "↑2", "↓1", or null when in sync / no upstream tracking. */
export function aheadBehindLabel(ahead: number, behind: number): string | null {
  const parts: string[] = [];
  if (ahead > 0) parts.push(`↑${ahead}`);
  if (behind > 0) parts.push(`↓${behind}`);
  return parts.length > 0 ? parts.join(" ") : null;
}

/**
 * A distinct, human-readable explanation for a worktree's problem state, or
 * null when the worktree is healthy. Distinguishes the four acceptance cases:
 * missing directory, detached HEAD, branch checked out elsewhere, and stale
 * (prunable) registration.
 */
export function worktreeProblem(entry: WorktreeEntry): string | null {
  if (entry.missingDir) return "The worktree directory is missing — prune the stale registration.";
  if (entry.prunable)
    return entry.prunableReason ? `Stale registration: ${entry.prunableReason}` : "Stale registration — safe to prune.";
  if (entry.branchCheckedOutElsewhere) return "This branch is also checked out in another worktree.";
  if (entry.detached) return "Detached HEAD — not on any branch.";
  if (entry.state === "statusUnavailable") return "Could not read this worktree's Git status.";
  return null;
}

export interface WorktreeCard {
  entry: WorktreeEntry;
  displayName: string;
  branchLabel: string;
  threadCount: number;
  dirty: boolean;
  statusLabel: string;
  aheadBehind: string | null;
  problem: string | null;
}

/** Count active threads whose cwd lives inside a worktree path. */
export function threadCountForPath(projects: Project[], path: string): number {
  const normalized = path.replace(/\/+$/, "");
  let count = 0;
  for (const project of projects) {
    for (const thread of project.threads) {
      const cwd = (thread.cwd || project.path).replace(/\/+$/, "");
      if (cwd === normalized || cwd.startsWith(`${normalized}/`)) count += 1;
    }
  }
  return count;
}

/** Map backend worktree entries to render-ready card models. Pure and tested. */
export function worktreeCards(entries: WorktreeEntry[], projects: Project[]): WorktreeCard[] {
  return entries.map((entry) => ({
    entry,
    displayName: folderName(entry.path),
    branchLabel: entry.detached
      ? entry.head
        ? `detached @ ${entry.head.slice(0, 7)}`
        : "detached"
      : (entry.branch ?? "(no branch)"),
    threadCount: threadCountForPath(projects, entry.path),
    dirty: isDirty(entry.status),
    statusLabel: statusSummary(entry.status),
    aheadBehind: aheadBehindLabel(entry.ahead, entry.behind),
    problem: worktreeProblem(entry),
  }));
}
