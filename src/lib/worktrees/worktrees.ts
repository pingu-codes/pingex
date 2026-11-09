import type { Project, StatusCounts, WorktreeEntry } from "$lib/types";
import { basename, trimSlash } from "$lib/utils/path";

/**
 * The local branch name a remote-tracking branch would check out as:
 * `origin/feat/x` → `feat/x`. Local names pass through unchanged.
 */
export function stripRemotePrefix(name: string): string {
  const index = name.indexOf("/");
  return index > 0 ? name.slice(index + 1) : name;
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
  const normalized = trimSlash(path);
  let count = 0;
  for (const project of projects) {
    for (const thread of project.threads) {
      const cwd = trimSlash(thread.cwd || project.path);
      if (cwd === normalized || cwd.startsWith(`${normalized}/`)) count += 1;
    }
  }
  return count;
}

/**
 * Linked worktrees that could be adopted as sidebar projects: not the main
 * tree, still on disk, not temporary scaffolding, and not listed already.
 */
export function adoptableWorktrees(entries: WorktreeEntry[], projects: Project[]): WorktreeEntry[] {
  const listed = new Set(projects.map((project) => trimSlash(project.path)));
  return entries.filter(
    (entry) =>
      !entry.isMain &&
      !entry.missingDir &&
      !entry.prunable &&
      !entry.isTemp &&
      !entry.isManagedPrWorktree &&
      !listed.has(trimSlash(entry.path)),
  );
}

/** The PR a managed worktree belongs to, from its `pr/<n>` branch or
 *  `pr-<n>` directory; `null` for any other worktree. */
export function prNumberOf(entry: WorktreeEntry): number | null {
  if (!entry.isManagedPrWorktree) return null;
  const match = entry.branch?.match(/^pr\/(\d+)/) ?? basename(entry.path).match(/^pr-(\d+)$/);
  return match ? Number(match[1]) : null;
}

/** The branch a worktree is on, or where its detached HEAD points. */
export function branchLabel(entry: WorktreeEntry): string {
  if (entry.detached) return entry.head ? `detached @ ${entry.head.slice(0, 7)}` : "detached";
  return entry.branch ?? "(no branch)";
}

/** Map backend worktree entries to render-ready card models. Pure and tested.
 *  Managed PR worktrees are listed too: they can hold real work, so they have
 *  to be findable and removable. */
export function worktreeCards(entries: WorktreeEntry[], projects: Project[]): WorktreeCard[] {
  return entries.map((entry) => ({
    entry,
    displayName: basename(entry.path),
    branchLabel: branchLabel(entry),
    threadCount: threadCountForPath(projects, entry.path),
    dirty: isDirty(entry.status),
    statusLabel: statusSummary(entry.status),
    aheadBehind: aheadBehindLabel(entry.ahead, entry.behind),
    problem: worktreeProblem(entry),
  }));
}
