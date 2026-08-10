import type { FileUpdateChange, ThreadItem } from "$lib/types";

/**
 * Every file the thread's patches touched, deduped by path.
 *
 * A file edited three times is one entry carrying the latest diff, held in the
 * position it was first touched — the list is a stable inventory of what the
 * thread changed, not a log that reshuffles as work continues.
 *
 * Both the floating menu's Outputs section and the Changes panel read this, so
 * neither can end up showing a different set of files from the other.
 */
export function collectFileChanges(items: ThreadItem[]): FileUpdateChange[] {
  const byPath = new Map<string, FileUpdateChange>();
  for (const item of items) {
    if (item.type !== "fileChange") continue;
    for (const change of item.changes ?? []) byPath.set(change.path, change);
  }
  return [...byPath.values()];
}

/**
 * Fold incoming changes into the ones already held, by path. Codex reports a
 * patch as it applies, and a later report need not repeat every file an earlier
 * one named, so replacing the array outright silently drops files — an edit
 * made early in a multi-file patch disappears from the thread's file list.
 * Incoming wins per path (it carries the newer diff) and first-touched order
 * is preserved.
 */
export function mergeFileChanges(
  existing: FileUpdateChange[] | undefined,
  incoming: FileUpdateChange[] | undefined,
): FileUpdateChange[] {
  const byPath = new Map<string, FileUpdateChange>();
  for (const change of existing ?? []) byPath.set(change.path, change);
  for (const change of incoming ?? []) byPath.set(change.path, change);
  return [...byPath.values()];
}

/** What happened to a file, in the words the menu and the diff header use. */
export function changeLabel(kind: string): string {
  if (kind === "add") return "New";
  if (kind === "delete") return "Deleted";
  if (kind === "move") return "Renamed";
  return "Edited";
}
