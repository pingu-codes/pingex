import { gitStatus } from "$lib/services/api";
import type { GitStatus } from "$lib/types";

/**
 * Shared, on-demand cache of `git status` per directory. Branch chips read from
 * here so several chips for the same repo share one fetch; the Worktrees view
 * and a completed agent turn trigger explicit refreshes. Nothing here polls.
 */
export const gitStatusCache = $state<{
  byPath: Record<string, GitStatus | null>;
  loading: Record<string, boolean>;
}>({
  byPath: {},
  loading: {},
});

export async function refreshGitStatus(path: string): Promise<void> {
  if (!path || gitStatusCache.loading[path]) return;
  gitStatusCache.loading[path] = true;
  try {
    gitStatusCache.byPath[path] = await gitStatus(path);
  } catch {
    // A non-git folder or read failure just leaves the chip hidden.
    gitStatusCache.byPath[path] = null;
  } finally {
    gitStatusCache.loading[path] = false;
  }
}

/** Fetch once if we have never looked at this path. */
export function ensureGitStatus(path: string): void {
  if (path && !(path in gitStatusCache.byPath) && !gitStatusCache.loading[path]) {
    refreshGitStatus(path);
  }
}

export function statusIsDirty(status: GitStatus | null | undefined): boolean {
  if (!status) return false;
  const { staged, unstaged, untracked, conflicted } = status.counts;
  return staged + unstaged + untracked + conflicted > 0;
}
