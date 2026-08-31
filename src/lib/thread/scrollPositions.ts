/**
 * Where each thread's transcript was scrolled to, kept for the app session.
 *
 * Switching threads remounts `ThreadView`, which would otherwise open every
 * transcript at the top. A thread left at the bottom comes back at the bottom
 * even after new content arrived; anywhere else comes back at the same offset.
 */

/** Distance from the bottom, in px, still treated as "at the bottom". */
export const NEAR_BOTTOM_PX = 120;

export interface ScrollPosition {
  top: number;
  atBottom: boolean;
}

const positions: Record<string, ScrollPosition> = {};

export function isNearBottom(el: HTMLElement): boolean {
  return el.scrollHeight - el.scrollTop - el.clientHeight < NEAR_BOTTOM_PX;
}

/**
 * Whether the transcript should keep following the live bottom after a
 * user-initiated scroll. Any move up detaches — even inside the near-bottom
 * band, otherwise a single wheel tick during fast streaming is snapped back
 * before the next one lands. Moving down re-attaches only once near the bottom.
 */
export function nextFollowing(prev: boolean, lastTop: number, el: HTMLElement): boolean {
  if (el.scrollTop < lastTop) return false;
  if (isNearBottom(el)) return true;
  return prev;
}

/**
 * Whether the scroller is pinned to the very bottom (within `slop` px of
 * rounding error), as opposed to the wide `isNearBottom` re-attach band.
 */
export function isAtBottom(el: HTMLElement, slop = 2): boolean {
  return el.scrollHeight - el.scrollTop - el.clientHeight <= slop;
}

export function rememberScroll(threadId: string, el: HTMLElement): void {
  positions[threadId] = { top: el.scrollTop, atBottom: isNearBottom(el) };
}

export function recallScroll(threadId: string): ScrollPosition | null {
  return positions[threadId] ?? null;
}

/** Test seam. */
export function resetScrollPositions(): void {
  for (const id of Object.keys(positions)) delete positions[id];
}
