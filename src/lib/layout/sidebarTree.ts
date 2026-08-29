/**
 * The sidebar's folder tree, assembled from the flat layout the backend
 * stores (`SidebarLayout`) and the flat item lists it already sends.
 *
 * Projects and threads stay flat everywhere else in the app; only the
 * sidebar nests them, and only through these pure helpers so the ordering
 * rule lives in exactly one place. Within a parent: pinned items first, then
 * anything with an explicit ordinal (folders always have one), then items
 * that were never dragged — most recently active first when the backend
 * knows that (`recency`, unreleased Codex), else in the order it listed them.
 */
import type { SidebarFolder, SidebarItemRef, SidebarLayout, SidebarPlacement } from "$lib/types";

export const ROOT_SCOPE = "";

export type TreeNode<T> =
  | { kind: "folder"; id: string; folder: SidebarFolder; children: TreeNode<T>[] }
  | { kind: "item"; id: string; item: T };

export interface TreeItemAdapter<T> {
  key: (item: T) => string;
  pinned: (item: T) => boolean;
  /** When the item was last active (Unix seconds), or null when unknown. */
  recency?: (item: T) => number | null | undefined;
}

export const emptyLayout = (): SidebarLayout => ({ folders: [], placements: [] });

const folderIds = (layout: SidebarLayout, scope: string) =>
  new Set(layout.folders.filter((folder) => folder.scope === scope).map((folder) => folder.id));

/** The folder a folder/placement claims as parent, or null when that folder
 *  no longer exists (so orphans surface at the scope root, never vanish). */
function liveParent(parentId: string | null, known: Set<string>): string | null {
  return parentId && known.has(parentId) ? parentId : null;
}

export function buildTree<T>(
  layout: SidebarLayout,
  scope: string,
  items: T[],
  adapter: TreeItemAdapter<T>,
): TreeNode<T>[] {
  const known = folderIds(layout, scope);
  const placements = new Map<string, SidebarPlacement>();
  for (const placement of layout.placements) {
    if (placement.scope === scope) placements.set(placement.itemKey, placement);
  }
  type Entry = { node: TreeNode<T>; parent: string | null; sort: [number, number, number, number] };
  const entries: Entry[] = [];
  for (const folder of layout.folders) {
    if (folder.scope !== scope) continue;
    entries.push({
      node: { kind: "folder", id: folder.id, folder, children: [] },
      parent: liveParent(folder.parentId, known),
      sort: [1, folder.ordinal, 0, 0],
    });
  }
  items.forEach((item, index) => {
    const key = adapter.key(item);
    const placement = placements.get(key);
    entries.push({
      node: { kind: "item", id: key, item },
      parent: placement ? liveParent(placement.parentId, known) : null,
      sort: [
        adapter.pinned(item) ? 0 : 1,
        placement ? placement.ordinal : Number.POSITIVE_INFINITY,
        -(adapter.recency?.(item) ?? 0),
        index,
      ],
    });
  });
  entries.sort(
    (a, b) => a.sort[0] - b.sort[0] || a.sort[1] - b.sort[1] || a.sort[2] - b.sort[2] || a.sort[3] - b.sort[3],
  );
  const byId = new Map<string, TreeNode<T>>();
  for (const entry of entries) if (entry.node.kind === "folder") byId.set(entry.node.id, entry.node);
  const roots: TreeNode<T>[] = [];
  for (const entry of entries) {
    const parent = entry.parent ? byId.get(entry.parent) : undefined;
    if (parent && parent.kind === "folder") parent.children.push(entry.node);
    else roots.push(entry.node);
  }
  return roots;
}

/** Folders whose subtree holds no items are dropped (recursively). Used by
 *  the session-focus view, where a folder of untouched threads is just noise. */
export function pruneEmptyFolders<T>(nodes: TreeNode<T>[]): TreeNode<T>[] {
  const out: TreeNode<T>[] = [];
  for (const node of nodes) {
    if (node.kind === "item") out.push(node);
    else {
      const children = pruneEmptyFolders(node.children);
      if (children.length > 0) out.push({ ...node, children });
    }
  }
  return out;
}

/** Stable partition at every level: nodes whose subtree contains an active
 *  item first, the rest after, each half keeping its existing order — so
 *  pinned/dragged ordering still holds within the two groups. */
export function hoistActive<T>(nodes: TreeNode<T>[], isActive: (item: T) => boolean): TreeNode<T>[] {
  const hasActive = (node: TreeNode<T>): boolean =>
    node.kind === "item" ? isActive(node.item) : node.children.some(hasActive);
  const lifted = nodes.map((node) =>
    node.kind === "folder" ? { ...node, children: hoistActive(node.children, isActive) } : node,
  );
  return [...lifted.filter(hasActive), ...lifted.filter((node) => !hasActive(node))];
}

export const refOf = <T>(node: TreeNode<T>): SidebarItemRef => ({ kind: node.kind, id: node.id });
export const sameRef = (a: SidebarItemRef, b: SidebarItemRef) => a.kind === b.kind && a.id === b.id;

/** Every item in the subtree, depth-first. */
export function flattenItems<T>(nodes: TreeNode<T>[]): T[] {
  const out: T[] = [];
  for (const node of nodes) {
    if (node.kind === "item") out.push(node.item);
    else out.push(...flattenItems(node.children));
  }
  return out;
}

/** The child list of `parentId` (null = the scope root), or null if no such folder. */
export function childrenOf<T>(tree: TreeNode<T>[], parentId: string | null): TreeNode<T>[] | null {
  if (parentId === null) return tree;
  for (const node of tree) {
    if (node.kind !== "folder") continue;
    if (node.id === parentId) return node.children;
    const nested = childrenOf(node.children, parentId);
    if (nested) return nested;
  }
  return null;
}

/** The parent folder id of `ref` within the tree (null = root, undefined = not found). */
export function parentOf<T>(
  tree: TreeNode<T>[],
  ref: SidebarItemRef,
  parent: string | null = null,
): string | null | undefined {
  for (const node of tree) {
    if (sameRef(refOf(node), ref)) return parent;
    if (node.kind === "folder") {
      const found = parentOf(node.children, ref, node.id);
      if (found !== undefined) return found;
    }
  }
  return undefined;
}

/** Whether `candidate` is `folderId` or sits somewhere beneath it. */
export function isFolderOrDescendant(folders: SidebarFolder[], folderId: string, candidate: string | null): boolean {
  let current = candidate;
  for (let hops = 0; current && hops <= folders.length; hops++) {
    if (current === folderId) return true;
    current = folders.find((folder) => folder.id === current)?.parentId ?? null;
  }
  return false;
}

/** Where a drag would land: under `parentId`, in front of `before` (or last). */
export interface DropTarget {
  parentId: string | null;
  before: SidebarItemRef | null;
  /** The row the pointer is over and which of its edges lit up; for drawing only. */
  rowId: string;
  zone: "before" | "after" | "inside";
}

/**
 * Turn a pointer position over a row into a drop target. The top third means
 * "before this row", the bottom third "after it"; the middle means "into it"
 * for folders and snaps to the nearest edge for anything else.
 */
export function resolveDrop<T>(
  tree: TreeNode<T>[],
  layout: SidebarLayout,
  dragging: SidebarItemRef,
  row: SidebarItemRef,
  pointerY: number,
  rect: { top: number; height: number },
): DropTarget | null {
  if (sameRef(dragging, row)) return null;
  if (
    dragging.kind === "folder" &&
    row.kind === "folder" &&
    isFolderOrDescendant(layout.folders, dragging.id, row.id)
  ) {
    return null;
  }
  const rowParent = parentOf(tree, row);
  if (rowParent === undefined) return null;
  const fraction = rect.height > 0 ? (pointerY - rect.top) / rect.height : 0.5;
  const rowId = `${row.kind}:${row.id}`;
  if (row.kind === "folder" && fraction >= 1 / 3 && fraction <= 2 / 3) {
    return { parentId: row.id, before: null, rowId, zone: "inside" };
  }
  const siblings = childrenOf(tree, rowParent) ?? [];
  const index = siblings.findIndex((node) => sameRef(refOf(node), row));
  if (fraction < 0.5) return { parentId: rowParent, before: row, rowId, zone: "before" };
  const next = siblings[index + 1];
  return { parentId: rowParent, before: next ? refOf(next) : null, rowId, zone: "after" };
}

/** The sibling order the backend should store after dropping `item` at `target`. */
export function siblingsAfterDrop<T>(tree: TreeNode<T>[], item: SidebarItemRef, target: DropTarget): SidebarItemRef[] {
  const current = (childrenOf(tree, target.parentId) ?? []).map(refOf).filter((ref) => !sameRef(ref, item));
  const at = target.before ? current.findIndex((ref) => sameRef(ref, target.before as SidebarItemRef)) : -1;
  if (at < 0) return [...current, item];
  return [...current.slice(0, at), item, ...current.slice(at)];
}

/** Whether a drop would change nothing (same parent, same neighbours). */
export function isNoopDrop<T>(tree: TreeNode<T>[], item: SidebarItemRef, target: DropTarget): boolean {
  const parent = parentOf(tree, item);
  if (parent !== target.parentId) return false;
  const before = (childrenOf(tree, parent) ?? []).map(refOf);
  const after = siblingsAfterDrop(tree, item, target);
  return before.length === after.length && before.every((ref, index) => sameRef(ref, after[index]));
}

// ---------------------------------------------------------------------------
// In-memory mutations mirroring the backend, used by the preview (non-Tauri)
// build so the browser demo behaves like the app.

const BUMP = 1_000_000;

export function placeInLayout(
  layout: SidebarLayout,
  scope: string,
  item: SidebarItemRef,
  parentId: string | null,
  siblings: SidebarItemRef[],
): SidebarLayout {
  const folders = layout.folders.map((folder) =>
    folder.scope === scope && folder.parentId === parentId ? { ...folder, ordinal: folder.ordinal + BUMP } : folder,
  );
  const placements = layout.placements.map((placement) =>
    placement.scope === scope && placement.parentId === parentId
      ? { ...placement, ordinal: placement.ordinal + BUMP }
      : placement,
  );
  const ordinalOf = (ref: SidebarItemRef) => siblings.findIndex((sibling) => sameRef(sibling, ref));
  for (const folder of folders) {
    if (folder.scope !== scope) continue;
    if (folder.id === item.id && item.kind === "folder") folder.parentId = parentId;
    const ordinal = ordinalOf({ kind: "folder", id: folder.id });
    if (ordinal >= 0 && folder.parentId === parentId) folder.ordinal = ordinal;
  }
  const seen = new Set<string>();
  for (const placement of placements) {
    if (placement.scope !== scope) continue;
    const ordinal = ordinalOf({ kind: "item", id: placement.itemKey });
    if (placement.itemKey === item.id && item.kind === "item") placement.parentId = parentId;
    if (ordinal >= 0 && placement.parentId === parentId) {
      placement.ordinal = ordinal;
      seen.add(placement.itemKey);
    }
  }
  siblings.forEach((sibling, ordinal) => {
    if (sibling.kind === "item" && !seen.has(sibling.id)) {
      placements.push({ itemKey: sibling.id, scope, parentId, ordinal });
    }
  });
  return { folders, placements };
}

export function deleteFromLayout(layout: SidebarLayout, folderId: string): SidebarLayout {
  const target = layout.folders.find((folder) => folder.id === folderId);
  if (!target) return layout;
  const lift = <R extends { parentId: string | null; ordinal: number }>(row: R): R =>
    row.parentId === folderId ? { ...row, parentId: target.parentId, ordinal: row.ordinal + BUMP } : row;
  return {
    folders: layout.folders.filter((folder) => folder.id !== folderId).map(lift),
    placements: layout.placements.map(lift),
  };
}

export function nextOrdinal(layout: SidebarLayout, scope: string, parentId: string | null): number {
  const ordinals = [
    ...layout.folders.filter((f) => f.scope === scope && f.parentId === parentId).map((f) => f.ordinal),
    ...layout.placements.filter((p) => p.scope === scope && p.parentId === parentId).map((p) => p.ordinal),
  ];
  return ordinals.length ? Math.max(...ordinals) + 1 : 0;
}
