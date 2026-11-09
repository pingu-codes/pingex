// What a page had open when it was left: its filters, tab and scroll. Pages
// unmount on every navigation, so each one reads its record as it mounts and
// keeps it current, and opens as it was left however it is reached again:
// Back, the sidebar or a row. In memory only, so a restart starts fresh.

/** Enough for a long session; the least recently opened go first. */
const LIMIT = 100;

const records = new Map<string, object>();

/** The live record for `key`, made by `fresh` the first time. Mutate it. */
export function place<T extends object>(key: string, fresh: () => T): T {
  const record = (records.get(key) as T | undefined) ?? fresh();
  records.delete(key);
  records.set(key, record);
  if (records.size > LIMIT) {
    const oldest = records.keys().next().value;
    if (oldest !== undefined) records.delete(oldest);
  }
  return record;
}

export function forgetPlaces(): void {
  records.clear();
}
