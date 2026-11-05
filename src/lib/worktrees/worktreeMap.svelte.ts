import { commands } from "$lib/services/homeRouting";
import type { WorktreeEntry } from "$lib/types";
import { trimSlash } from "$lib/utils/path";

/**
 * `git worktree list` per repository, fetched only while the sidebar nests
 * worktrees. A non-git folder caches an empty list so it is asked once.
 * Nothing here polls: the sidebar asks again when its project list changes.
 */
export const worktreeMap = $state<{
  byRepo: Record<string, WorktreeEntry[]>;
  /** Unadded worktree path → the repository project it was listed under. */
  unaddedParents: Record<string, string>;
}>({ byRepo: {}, unaddedParents: {} });

const loading = new Set<string>();
const fetchedAt = new Map<string, number>();

async function refresh(repo: string): Promise<void> {
  if (loading.has(repo)) return;
  loading.add(repo);
  try {
    worktreeMap.byRepo[repo] = await commands.gitWorktrees(repo);
  } catch {
    worktreeMap.byRepo[repo] = [];
  } finally {
    loading.delete(repo);
    fetchedAt.set(repo, Date.now());
  }
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
