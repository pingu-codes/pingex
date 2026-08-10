// Pure helpers for the sidebar search command: deriving the visible state,
// count labels and filter chips. Kept free of Svelte/DOM so they are unit
// testable and reused by SidebarSearch.svelte.

import type { ThreadSearchItem } from "$lib/types";

/** A single lazily-loaded group of search results (active or archived). */
export interface SearchGroup {
  items: ThreadSearchItem[];
  total: number;
  cursor: string | null;
  loading: boolean;
}

export function emptyGroup(): SearchGroup {
  return { items: [], total: 0, cursor: null, loading: false };
}

export type SearchState = "idle" | "loading" | "results" | "empty" | "error";

/**
 * Derive the overall UI state from the query, the two result groups and any
 * error. `idle` means no query has been entered yet; `empty` means the query
 * ran but matched nothing.
 */
export function searchState(input: {
  query: string;
  active: SearchGroup;
  archived: SearchGroup;
  error: unknown;
  loading: boolean;
}): SearchState {
  if (input.error) return "error";
  if (!input.query.trim()) return "idle";
  const total = input.active.total + input.archived.total;
  const loaded = input.active.items.length + input.archived.items.length;
  if (input.loading && loaded === 0) return "loading";
  if (total === 0 && !input.loading) return "empty";
  return "results";
}

/** Whether a group has more results to fetch via `Load more`. */
export function hasMore(group: SearchGroup): boolean {
  return group.cursor !== null;
}

/** "12 of 40" style label; the "of N" part is omitted when nothing loaded. */
export function countLabel(loaded: number, total: number): string {
  if (total === 0) return "0";
  if (loaded >= total) return String(total);
  return `${loaded} of ${total}`;
}

/** Message shown when a query matches nothing. */
export function noMatchLabel(query: string): string {
  return `No results for "${query.trim()}"`;
}

/**
 * Label for the project-scope filter chip. When scoped, names the project and
 * offers "all projects"; when unscoped there is no chip.
 */
export function filterChipLabel(projectName: string | null): string | null {
  if (!projectName) return null;
  return projectName;
}
