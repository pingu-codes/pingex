import { getCurrentWindow } from "@tauri-apps/api/window";
import { isTauri } from "$lib/services/api";

/**
 * Makes an element act like a native macOS titlebar: click-drag moves the
 * window, double-click toggles maximize. Interactive children (buttons,
 * inputs, links) are excluded so they stay clickable.
 */
export function dragRegion(node: HTMLElement) {
  const onMouseDown = (event: MouseEvent) => {
    if (!isTauri() || event.button !== 0) return;
    const target = event.target as HTMLElement;
    if (target.closest("button, a, input, textarea, select, [data-no-drag]")) return;
    event.preventDefault();
    const win = getCurrentWindow();
    if (event.detail === 2) {
      void win.toggleMaximize();
    } else {
      void win.startDragging();
    }
  };

  node.addEventListener("mousedown", onMouseDown);
  return {
    destroy() {
      node.removeEventListener("mousedown", onMouseDown);
    },
  };
}
