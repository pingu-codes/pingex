import type { MemoryEntry, MemoryHome, MemoryStore } from "$lib/types";

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
  return text.replace(/^---\r?\n[\s\S]*?\r?\n---[ \t]*(\r?\n|$)/, "");
}

/** Markdown renders; anything else (a skill's script) shows as text. */
export function isMarkdown(path: string): boolean {
  return /\.(md|markdown)$/i.test(path);
}

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/** "3m ago", "5h ago", "4d ago", or a date past a month. */
export function formatAge(seconds: number | null, now = Date.now()): string {
  if (seconds == null) return "";
  const elapsed = Math.max(0, now / 1000 - seconds);
  if (elapsed < 60) return "just now";
  if (elapsed < 3600) return `${Math.floor(elapsed / 60)}m ago`;
  if (elapsed < 86400) return `${Math.floor(elapsed / 3600)}h ago`;
  if (elapsed < 86400 * 30) return `${Math.floor(elapsed / 86400)}d ago`;
  return new Date(seconds * 1000).toLocaleDateString();
}
