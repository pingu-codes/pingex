/** Drag-to-resize helper for panel edges, with sizes persisted to localStorage. */

export type ResizeHandleOptions = {
  axis: "x" | "y";
  /** +1 when dragging right/down grows the panel, -1 when dragging left/up grows it. */
  direction: 1 | -1;
  min: number;
  max: number;
  storageKey?: string;
  getSize: () => number;
  onResize: (size: number) => void;
};

const clamp = (value: number, min: number, max: number) => Math.min(max, Math.max(min, value));

export function loadSize(storageKey: string, fallback: number, min: number, max: number): number {
  try {
    const raw = localStorage.getItem(storageKey);
    const parsed = raw === null ? Number.NaN : Number.parseFloat(raw);
    return Number.isFinite(parsed) ? clamp(parsed, min, max) : fallback;
  } catch {
    return fallback;
  }
}

/** Svelte action for a drag handle element sitting on a resizable panel's edge. */
export function resizeHandle(node: HTMLElement, options: ResizeHandleOptions) {
  let current = options;
  let start = 0;
  let startSize = 0;

  function onPointerMove(event: PointerEvent) {
    const position = current.axis === "x" ? event.clientX : event.clientY;
    const size = clamp(startSize + (position - start) * current.direction, current.min, current.max);
    current.onResize(size);
  }

  function onPointerUp(event: PointerEvent) {
    node.releasePointerCapture(event.pointerId);
    node.removeEventListener("pointermove", onPointerMove);
    node.removeEventListener("pointerup", onPointerUp);
    document.body.style.cursor = "";
    document.body.style.userSelect = "";
    if (current.storageKey) {
      try {
        localStorage.setItem(current.storageKey, String(current.getSize()));
      } catch {
        // Persistence is best-effort.
      }
    }
  }

  function onPointerDown(event: PointerEvent) {
    if (event.button !== 0) return;
    event.preventDefault();
    start = current.axis === "x" ? event.clientX : event.clientY;
    startSize = current.getSize();
    node.setPointerCapture(event.pointerId);
    node.addEventListener("pointermove", onPointerMove);
    node.addEventListener("pointerup", onPointerUp);
    document.body.style.cursor = current.axis === "x" ? "col-resize" : "row-resize";
    document.body.style.userSelect = "none";
  }

  node.addEventListener("pointerdown", onPointerDown);
  return {
    update(next: ResizeHandleOptions) {
      current = next;
    },
    destroy() {
      node.removeEventListener("pointerdown", onPointerDown);
    },
  };
}
