/**
 * Threads the user has started or opened since the app launched. Runtime-only
 * by design: the sidebar's "session focus" view is about *this* sitting, so
 * the set starts empty on every launch and is never persisted.
 */
import { SvelteSet } from "svelte/reactivity";

export const touchedThreads = new SvelteSet<string>();

export function touchThread(id: string | null | undefined): void {
  if (id) touchedThreads.add(id);
}

export function isTouched(id: string): boolean {
  return touchedThreads.has(id);
}

/** Test hook. */
export function resetTouched(): void {
  touchedThreads.clear();
}
