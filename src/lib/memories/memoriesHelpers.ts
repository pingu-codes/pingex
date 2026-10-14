import { splitFrontmatter } from "$lib/integrations/integrationsHelpers";
import type { MemoryEntry, MemoryHome, MemoryStore } from "$lib/types";
import { relativeTime } from "$lib/utils/format";

/** A store with only the entries a search matched, and whether anything did. */
export type FilteredStore = MemoryStore & { total: number };
export type FilteredHome = Omit<MemoryHome, "stores"> & { stores: FilteredStore[] };

/**
 * Narrow every store to the entries whose title, description, path or kind
 * contain each word of `query`. A store whose title or project matches keeps
 * all its entries. Stores left empty by a search are dropped; with no query
 * every store stays, empty or not.
 */
export function filterMemories(homes: MemoryHome[], query: string): FilteredHome[] {
  const words = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
  const matches = (text: string) => words.every((word) => text.includes(word));
  return homes.map((home) => ({
    ...home,
    stores: home.stores
      .map((store) => {
        const storeText = [store.title, store.projectPath ?? ""].join(" ").toLowerCase();
        const entries =
          words.length === 0 || matches(storeText)
            ? store.entries
            : store.entries.filter((entry) => matches(entryText(entry)));
        return { ...store, entries, total: store.entries.length };
      })
      .filter((store) => words.length === 0 || store.entries.length > 0),
  }));
}

function entryText(entry: MemoryEntry): string {
  return [entry.title, entry.description ?? "", entry.relPath, entry.kind ?? ""].join(" ").toLowerCase();
}

/** Find an entry and the Home it lives in by its path. */
export function findEntry(
  homes: MemoryHome[],
  homeId: string | null,
  path: string | null,
): { home: MemoryHome; store: MemoryStore; entry: MemoryEntry } | null {
  if (!homeId || !path) return null;
  const home = homes.find((candidate) => candidate.homeId === homeId);
  for (const store of home?.stores ?? []) {
    const entry = store.entries.find((candidate) => candidate.path === path);
    if (home && entry) return { home, store, entry };
  }
  return null;
}

/** The first entry worth opening: an index or summary, else the first file. */
export function defaultEntry(homes: MemoryHome[]): { homeId: string; path: string } | null {
  for (const home of homes) {
    for (const store of home.stores) {
      const entry =
        store.entries.find((candidate) => candidate.kind === "index" || candidate.kind === "summary") ??
        store.entries[0];
      if (entry) return { homeId: home.homeId, path: entry.path };
    }
  }
  return null;
}

export function entryCount(homes: MemoryHome[]): number {
  return homes.reduce((sum, home) => sum + home.stores.reduce((count, store) => count + store.entries.length, 0), 0);
}

/** The markdown body without its `---` frontmatter, which the list already
 *  shows as title, description and kind. The editor keeps it. */
export function stripFrontmatter(text: string): string {
  return splitFrontmatter(text).body;
}

/** Markdown renders; anything else (a skill's script) shows as text. */
export function isMarkdown(path: string): boolean {
  return /\.(md|markdown)$/i.test(path);
}

/** "3m ago", "5h ago", "4d ago", or a date past a month. */
export function formatAge(seconds: number | null, now = Date.now()): string {
  if (seconds == null) return "";
  if (now / 1000 - seconds >= 86400 * 30) return new Date(seconds * 1000).toLocaleDateString();
  return relativeTime(seconds, { granularity: "minute", suffix: " ago", now: now / 1000 });
}
