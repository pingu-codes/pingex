import { onEffect } from "$lib/services/commandEffects";
import { fetchQuery, invalidate } from "$lib/services/queryCache.svelte";
import type { GitStatus } from "$lib/types";

/**
 * Shared, on-demand cache of `git status` per directory. Branch chips read from
 * here so several chips for the same repo share one fetch; the Worktrees view
 * and a completed agent turn trigger explicit refreshes, and so does any git
 * or review mutation on a known path. Reads go through the query cache, so a
 * second caller joins the read in flight. Nothing here polls.
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

async function read(path: string): Promise<void> {
  gitStatusCache.loading[path] = true;
  try {
    gitStatusCache.byPath[path] = await fetchQuery("gitStatus", path);
  } catch {
    // A non-git folder or read failure just leaves the chip hidden.
    gitStatusCache.byPath[path] = null;
  } finally {
    gitStatusCache.loading[path] = false;
    gitStatusCache.fetchedAt[path] = Date.now();
  }
}

const refreshing = new Map<string, Promise<void>>();

/** Read `path` again now. A refresh already running is joined: a mutation's
 *  own refresh (below) and the caller's are then one `git status`. */
export function refreshGitStatus(path: string): Promise<void> {
  if (!path) return Promise.resolve();
  const running = refreshing.get(path);
  if (running) return running;
  invalidate({ names: ["gitStatus"], repo: path });
  const next = read(path).finally(() => refreshing.delete(path));
  refreshing.set(path, next);
  return next;
}

/**
 * Fetch if we have never looked at this path, or the cached entry is older than
 * `maxAgeMs` — so a re-mounted chip picks up a branch switched outside the app.
 */
export function ensureGitStatus(path: string, maxAgeMs = 5000): void {
  if (!path || gitStatusCache.loading[path]) return;
  const seen = path in gitStatusCache.byPath;
  const stale = Date.now() - (gitStatusCache.fetchedAt[path] ?? 0) > maxAgeMs;
  if (!seen || stale) void read(path);
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
  for (const path of Object.keys(gitStatusCache.byPath)) void refreshGitStatus(path);
}

// A commit, checkout, pull or worktree change through the app: refresh the
// chips of the directories it named.
onEffect(({ changes, paths }) => {
  if (!changes.has("git")) return;
  for (const path of paths) if (path in gitStatusCache.byPath) void refreshGitStatus(path);
});
