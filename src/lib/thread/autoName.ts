import { applyData } from "$lib/app/appData.svelte";
import { autoNameThread } from "$lib/services/api";

/** Which passes a thread has already had, so a re-render or a replayed event
 *  cannot spend a second model call on a title the thread already has. */
type Pass = "seed" | "reply";
const done = new Map<string, Set<Pass>>();

/**
 * Generate this thread's sidebar title in the background.
 *
 * Naming is cosmetic and must never delay or fail the turn it decorates, so
 * this returns immediately and swallows errors; the existing title simply
 * stands. Each pass runs at most once per thread:
 *
 * - `seed` fires as the first message is sent, off that message alone, so the
 *   sidebar stops showing a raw prompt straight away.
 * - `reply` fires when the first turn completes and re-names off the exchange,
 *   which is the first point a title can reflect what the thread turned out to
 *   be about.
 */
export function requestAutoName(threadId: string, pass: Pass, seed?: string): void {
  const passes = done.get(threadId) ?? new Set<Pass>();
  if (passes.has(pass)) return;
  passes.add(pass);
  done.set(threadId, passes);

  autoNameThread(threadId, seed)
    .then((data) => data && applyData(data))
    .catch(() => {
      // A failed pass is not retried: the thread keeps its first-message title.
    });
}

/** Test seam: forget which threads have been named. */
export function resetAutoNameState(): void {
  done.clear();
}
