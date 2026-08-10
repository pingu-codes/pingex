/** Dismissal helper for hand-rolled popups: "the user is done with this panel". */

import { trackInteractOutside } from "@zag-js/interact-outside";

/**
 * Svelte action that calls `onOutside` when a pointer press or a focus move
 * lands outside `node`.
 *
 * Backed by the same module Skeleton's own Popover uses for dismissal, because
 * the naive version is wrong here: we ship in a Tauri webview, which is WebKit
 * on macOS, and WebKit does not focus a button on mousedown. A plain `focusout`
 * handler would therefore fire with `relatedTarget === null` the instant you
 * press a row inside the panel, unmounting it before its `click` ever ran.
 *
 * Note this deliberately does not react to the window losing focus — it listens
 * on `focusin`, which cmd-tabbing away never fires, so the panel survives an
 * app switch and is still there when you come back.
 */
export function clickOutside(node: HTMLElement, onOutside: () => void) {
  // Held in a mutable local so `update` can swap the callback without tearing
  // down and re-registering the listeners.
  let current = onOutside;
  const stop = trackInteractOutside(node, { onInteractOutside: () => current() });
  return {
    update(next: () => void) {
      current = next;
    },
    destroy: stop,
  };
}
