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
