import type { WorkspaceSearchResults } from "$lib/types";

/** True when every result group is empty (drives the "no matches" state). */
export function isEmptyResults(results: WorkspaceSearchResults | null): boolean {
  if (!results) return true;
  return (
    results.projectFiles.items.length === 0 && results.threads.items.length === 0 && results.messages.items.length === 0
  );
}

/** Total number of matches across all groups. */
export function totalMatchCount(results: WorkspaceSearchResults | null): number {
  if (!results) return 0;
  return results.projectFiles.items.length + results.threads.items.length + results.messages.items.length;
}

/** Label for the empty state, e.g. `No matches for "svelte"`. */
export function emptyStateLabel(query: string): string {
  return `No matches for "${query.trim()}"`;
}

/** A trailing-edge debounce with a `cancel()` method. The last call within
 *  `wait` ms wins; pending calls are dropped. */
export function debounce<A extends unknown[]>(
  fn: (...args: A) => void,
  wait: number,
): ((...args: A) => void) & { cancel: () => void } {
  let timer: ReturnType<typeof setTimeout> | null = null;
  const wrapped = (...args: A) => {
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => {
      timer = null;
      fn(...args);
    }, wait);
  };
  wrapped.cancel = () => {
    if (timer) clearTimeout(timer);
    timer = null;
  };
  return wrapped;
}
