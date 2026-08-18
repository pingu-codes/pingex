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
  fetchedAt: Record<string, number>;
}>({
  byPath: {},
  loading: {},
  fetchedAt: {},
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
    gitStatusCache.fetchedAt[path] = Date.now();
  }
}

/**
 * Fetch if we have never looked at this path, or the cached entry is older than
 * `maxAgeMs` — so a re-mounted chip picks up a branch switched outside the app.
 */
export function ensureGitStatus(path: string, maxAgeMs = 5000): void {
  if (!path || gitStatusCache.loading[path]) return;
  const seen = path in gitStatusCache.byPath;
  const stale = Date.now() - (gitStatusCache.fetchedAt[path] ?? 0) > maxAgeMs;
  if (!seen || stale) refreshGitStatus(path);
}

let lastRefreshAll = 0;
/**
 * Refresh every cached path (throttled). Called when the window regains focus,
 * since branch switches usually happen in a terminal or another app.
 */
export function refreshAllGitStatus(throttleMs = 1000): void {
  const now = Date.now();
  if (now - lastRefreshAll < throttleMs) return;
  lastRefreshAll = now;
  for (const path of Object.keys(gitStatusCache.byPath)) refreshGitStatus(path);
}

export function statusIsDirty(status: GitStatus | null | undefined): boolean {
  if (!status) return false;
  const { staged, unstaged, untracked, conflicted } = status.counts;
  return staged + unstaged + untracked + conflicted > 0;
}
