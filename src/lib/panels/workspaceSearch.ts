import type { WorkspaceSearchResults } from "$lib/types";

/** True when every result group is empty (drives the "no matches" state). */
export function isEmptyResults(results: WorkspaceSearchResults | null): boolean {
  if (!results) return true;
  return (
    results.projectFiles.items.length === 0 && results.threads.items.length === 0 && results.messages.items.length === 0
  );
}
