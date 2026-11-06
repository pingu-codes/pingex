import { argNamed, onCommandSettled } from "$lib/services/commandEvents";
import { fetchQuery, invalidate, touchesLocal } from "$lib/services/queryCache.svelte";
import type { WorktreeEntry } from "$lib/types";
import { trimSlash } from "$lib/utils/path";
import { PATH_ARGUMENTS } from "$lib/worktrees/gitStatus.svelte";

/**
 * `git worktree list` per repository, fetched only while the sidebar nests
 * worktrees. A non-git folder caches an empty list so it is asked once.
 * Nothing here polls: the sidebar asks again when its project list changes,
 * and a git or review mutation in one of a repository's worktrees re-reads
 * that repository. Reads go through the query cache, so callers share one.
 */
export const worktreeMap = $state<{
  byRepo: Record<string, WorktreeEntry[]>;
  /** Unadded worktree path → the repository project it was listed under. */
  unaddedParents: Record<string, string>;
}>({ byRepo: {}, unaddedParents: {} });

const loading = new Map<string, Promise<void>>();
const fetchedAt = new Map<string, number>();

/** Read `repo`'s listing, joining a read already running. The cache waits
 *  out a read that a mutation made stale, so a joined read is current. */
function refresh(repo: string): Promise<void> {
  const running = loading.get(repo);
  if (running) return running;
  const next = (async () => {
    try {
      worktreeMap.byRepo[repo] = await fetchQuery("gitWorktrees", repo);
    } catch {
      worktreeMap.byRepo[repo] = [];
    } finally {
      loading.delete(repo);
      fetchedAt.set(repo, Date.now());
    }
  })();
  loading.set(repo, next);
  return next;
}

/** Fetch every repository not seen within `maxAgeMs`. */
export function ensureWorktrees(repos: string[], maxAgeMs = 5000): void {
  const now = Date.now();
  for (const repo of repos) {
    if (now - (fetchedAt.get(repo) ?? 0) > maxAgeMs) void refresh(repo);
  }
}

/** The repository an unadded worktree directory (or a path inside one) belongs to. */
export function unaddedWorktreeParent(cwd: string): string | null {
  const directory = trimSlash(cwd);
  let best: string | null = null;
  for (const root of Object.keys(worktreeMap.unaddedParents)) {
    if ((directory === root || directory.startsWith(`${root}/`)) && root.length > (best?.length ?? -1)) best = root;
  }
  return best ? worktreeMap.unaddedParents[best] : null;
}

/** Forget every listing, so the next `ensureWorktrees` asks git again. */
export function resetWorktreeMap(): void {
  worktreeMap.byRepo = {};
  worktreeMap.unaddedParents = {};
  fetchedAt.clear();
}

/** The listed repositories a mutation on `path` could have changed: the
 *  repository itself, or the one listing `path` as a worktree. */
function reposTouching(path: string): string[] {
  return Object.entries(worktreeMap.byRepo).flatMap(([repo, entries]) =>
    repo === path || entries.some((entry) => entry.path === path) ? [repo] : [],
  );
}

onCommandSettled((command) => {
  if (!touchesLocal(command.name)) return;
  const repos = new Set<string>();
  for (const name of PATH_ARGUMENTS) {
    const path = argNamed(command, name);
    if (typeof path === "string") for (const repo of reposTouching(path)) repos.add(repo);
  }
  for (const repo of repos) {
    invalidate({ names: ["gitWorktrees"], repo });
    void refresh(repo);
  }
});
