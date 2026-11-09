// Which parts of the review page's Diffs pane are near the viewport, so a
// file's rows mount only there (`DiffChunk.svelte`). One IntersectionObserver
// per pane: the scroll container marked `data-diff-pane`.

import type { Attachment } from "svelte/attachments";

/** How far beyond the pane's edges counts as near: 1.5 pane heights. */
const MARGIN = "150% 0px";

type Pane = { observer: IntersectionObserver; listeners: Map<Element, (near: boolean) => void> };
const panes = new WeakMap<Element, Pane>();

function paneFor(root: Element): Pane {
  let pane = panes.get(root);
  if (!pane) {
    const listeners = new Map<Element, (near: boolean) => void>();
    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) listeners.get(entry.target)?.(entry.isIntersecting);
      },
      { root, rootMargin: MARGIN },
    );
    pane = { observer, listeners };
    panes.set(root, pane);
  }
  return pane;
}

/**
 * Calls `onChange` as the element comes near the Diffs pane's viewport and
 * leaves it. Outside a pane, or without IntersectionObserver (tests), it is
 * near at once and stays so.
 */
export function nearViewport(onChange: (near: boolean) => void): Attachment<HTMLElement> {
  return (element) => {
    const root = element.closest("[data-diff-pane]");
    if (!root || typeof IntersectionObserver === "undefined") {
      onChange(true);
      return;
    }
    const pane = paneFor(root);
    pane.listeners.set(element, onChange);
    pane.observer.observe(element);
    return () => {
      pane.observer.unobserve(element);
      pane.listeners.delete(element);
    };
  };
}

let lastScroll = -Infinity;
if (typeof document !== "undefined") {
  // Scroll doesn't bubble; capturing at the document sees every pane's.
  document.addEventListener("scroll", () => (lastScroll = performance.now()), { capture: true, passive: true });
}

/** Whether anything scrolled in the last moment: hover work waits it out. */
export function scrolling(): boolean {
  return performance.now() - lastScroll < 120;
}
