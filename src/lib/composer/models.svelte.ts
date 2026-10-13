/** Model discovery is cached by Home, including in-flight requests. */

import type { HarnessChoice } from "$lib/composer/composerPrefs.svelte";
import { listHarnessModels } from "$lib/services/api";
import { currentHomeKey } from "$lib/services/homeRouting";
import type { Model } from "$lib/types";

type Entry = { models: Model[] | null; error: string | null };
const entries = $state<Record<string, Entry>>({});
const inFlight = new Map<string, Promise<void>>();
const keyFor = (harness: string) => `${currentHomeKey(harness)}:${harness}`;

/** The models of the current Home for `harness`, once loaded. */
export function harnessModels(harness: HarnessChoice): Model[] | null {
  return entries[keyFor(harness)]?.models ?? null;
}
export function harnessModelsError(harness: HarnessChoice): string | null {
  return entries[keyFor(harness)]?.error ?? null;
}
export function ensureHarnessModels(harness: HarnessChoice): Promise<void> {
  return ensure(harness);
}

function ensure(harness: HarnessChoice): Promise<void> {
  const key = keyFor(harness);
  if (entries[key]?.models !== undefined && entries[key].models !== null) return Promise.resolve();
  const pending = inFlight.get(key);
  if (pending) return pending;
  entries[key] = { models: null, error: null };
  const request = (async () => {
    try {
      entries[key] = { models: await listHarnessModels(harness), error: null };
    } catch (cause) {
      entries[key] = { models: [], error: cause instanceof Error ? cause.message : String(cause) };
    } finally {
      inFlight.delete(key);
    }
  })();
  inFlight.set(key, request);
  return request;
}

export function modelLabel(id: string): string {
  return harnessModels("codex")?.find((model) => model.id === id)?.displayName ?? id;
}
