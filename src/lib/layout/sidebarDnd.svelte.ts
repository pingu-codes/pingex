/**
 * Pointer-driven drag and drop for sidebar rows.
 *
 * Hand-rolled on pointer events rather than HTML5 drag events: Tauri's native
 * drag-drop handler is enabled for file attachments, and on the macOS webview
 * that swallows in-page `dragstart`/`drop`. Pointer events are unaffected.
 *
 * The module only owns the mechanics — threshold, tracking, cancel, commit.
 * What a drop *means* is decided by the sidebar through the `resolve` and
 * `commit` hooks, which see the tree.
 */
import type { SidebarItemRef } from "$lib/types";
import type { DropTarget } from "./sidebarTree";

export interface DragSource {
  scope: string;
  ref: SidebarItemRef;
  label: string;
}

export interface DragHooks {
  /** The drop target for the pointer over `row`, or null when invalid. */
  resolve: (row: HTMLElement, pointerY: number) => DropTarget | null;
  commit: (target: DropTarget) => void | Promise<void>;
}

/** Pointer travel before a press turns into a drag, so clicks stay clicks. */
export const DRAG_THRESHOLD = 4;

export const dnd = $state<{
  dragging: DragSource | null;
  over: DropTarget | null;
  x: number;
  y: number;
  /** Set for the click that ends a drag, so the row underneath ignores it. */
  suppressClick: boolean;
}>({ dragging: null, over: null, x: 0, y: 0, suppressClick: false });

export const rowId = (ref: SidebarItemRef) => `${ref.kind}:${ref.id}`;

export function startDrag(event: PointerEvent, source: DragSource, hooks: DragHooks): void {
  if (event.button !== 0) return;
  const origin = { x: event.clientX, y: event.clientY };
  let active = false;

  const onMove = (move: PointerEvent) => {
    if (!active) {
      if (Math.hypot(move.clientX - origin.x, move.clientY - origin.y) < DRAG_THRESHOLD) return;
      active = true;
      dnd.dragging = source;
    }
    dnd.x = move.clientX;
    dnd.y = move.clientY;
    const row = document.elementFromPoint?.(move.clientX, move.clientY)?.closest<HTMLElement>("[data-sidebar-row]");
    dnd.over = row ? hooks.resolve(row, move.clientY) : null;
  };
  const finish = (drop: boolean) => {
    window.removeEventListener("pointermove", onMove);
    window.removeEventListener("pointerup", onUp);
    window.removeEventListener("pointercancel", onCancel);
    window.removeEventListener("keydown", onKey);
    if (!active) return;
    const target = dnd.over;
    dnd.dragging = null;
    dnd.over = null;
    dnd.suppressClick = true;
    setTimeout(() => (dnd.suppressClick = false), 0);
    if (drop && target) void hooks.commit(target);
  };
  const onUp = () => finish(true);
  const onCancel = () => finish(false);
  const onKey = (key: KeyboardEvent) => {
    if (key.key === "Escape") finish(false);
  };
  window.addEventListener("pointermove", onMove);
  window.addEventListener("pointerup", onUp);
  window.addEventListener("pointercancel", onCancel);
  window.addEventListener("keydown", onKey);
}

/** Svelte action: make a row draggable and addressable for drop resolution. */
export function draggable(node: HTMLElement, params: { source: DragSource; hooks: DragHooks }) {
  let current = params;
  node.dataset.sidebarRow = rowId(current.source.ref);
  node.dataset.sidebarScope = current.source.scope;
  const onDown = (event: PointerEvent) => startDrag(event, current.source, current.hooks);
  const onClick = (event: MouseEvent) => {
    if (dnd.suppressClick) {
      event.stopPropagation();
      event.preventDefault();
    }
  };
  node.addEventListener("pointerdown", onDown);
  node.addEventListener("click", onClick, true);
  return {
    update(next: { source: DragSource; hooks: DragHooks }) {
      current = next;
      node.dataset.sidebarRow = rowId(current.source.ref);
      node.dataset.sidebarScope = current.source.scope;
    },
    destroy() {
      node.removeEventListener("pointerdown", onDown);
      node.removeEventListener("click", onClick, true);
    },
  };
}

/** Parse a `[data-sidebar-row]` element back into its scope and ref. */
export function rowRef(element: HTMLElement): { scope: string; ref: SidebarItemRef } | null {
  const row = element.dataset.sidebarRow;
  const scope = element.dataset.sidebarScope;
  if (!row || scope === undefined) return null;
  const separator = row.indexOf(":");
  const kind = row.slice(0, separator);
  if (kind !== "folder" && kind !== "item") return null;
  return { scope, ref: { kind, id: row.slice(separator + 1) } };
}
