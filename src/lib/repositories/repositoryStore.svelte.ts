// Each project's repository state (cloned, or no GitHub remote feeding it)
// and the PRs-only repositories, for the sidebar and Add repository. Read
// from the backend once per project and refreshed after Add repository
// changes something.

import { commands } from "$lib/services/homeRouting";
import type { Repository } from "$lib/types";

export const repositoryStore = $state({
  /** By project path. */
  byPath: {} as Record<string, Repository>,
  prsOnly: [] as Repository[],
});

let prsOnlyLoaded = false;
const pending = new Set<string>();

async function loadPath(path: string) {
  pending.add(path);
  try {
    repositoryStore.byPath[path] = await commands.reviewRepository(path);
  } catch {
    // Not a git folder, or it is gone: it shows as a plain project.
  } finally {
    pending.delete(path);
  }
}

export async function refreshPrsOnly(): Promise<void> {
  prsOnlyLoaded = true;
  try {
    repositoryStore.prsOnly = (await commands.reviewPrsOnlyRepositories()) ?? [];
  } catch {
    repositoryStore.prsOnly = [];
  }
}

/** Load whatever is not loaded yet for `paths`, and the PRs-only list. */
export async function ensureRepositories(paths: readonly string[]): Promise<void> {
  const missing = paths.filter((path) => !(path in repositoryStore.byPath) && !pending.has(path));
  await Promise.all([...missing.map(loadPath), prsOnlyLoaded ? null : refreshPrsOnly()]);
}

/** Reload `paths` and the PRs-only list, after something changed them. */
export async function refreshRepositories(paths: readonly string[]): Promise<void> {
  await Promise.all([...paths.map(loadPath), refreshPrsOnly()]);
}

/** Every GitHub repository something already feeds into the inbox. */
export function addedSlugs(): Set<string> {
  const repositories = [...Object.values(repositoryStore.byPath), ...repositoryStore.prsOnly];
  return new Set(repositories.flatMap((repository) => repository.feeds.map((feed) => feed.slug.toLowerCase())));
}

export function resetRepositories() {
  repositoryStore.byPath = {};
  repositoryStore.prsOnly = [];
  prsOnlyLoaded = false;
  pending.clear();
}
