/** Model discovery is cached by Home, including in-flight requests. */
import { listHarnessModels, listModels } from "$lib/services/api";
import { currentHomeKey } from "$lib/services/homeRouting";
import type { Model } from "$lib/types";

type Entry = { models: Model[] | null; error: string | null };
const entries = $state<Record<string, Entry>>({});
const inFlight = new Map<string, Promise<void>>();
const keyFor = (harness: string) => `${currentHomeKey(harness)}:${harness}`;

export function models(): Model[] | null {
  return entries[keyFor("codex")]?.models ?? null;
}
export function modelsError(): string | null {
  return entries[keyFor("codex")]?.error ?? null;
}
export function claudeModels(): Model[] | null {
  return entries[keyFor("claude")]?.models ?? null;
}
export function claudeModelsError(): string | null {
  return entries[keyFor("claude")]?.error ?? null;
}
export function ensureModels(): Promise<void> {
  return ensure("codex");
}
export function ensureClaudeModels(): Promise<void> {
  return ensure("claude");
}

function ensure(harness: "codex" | "claude"): Promise<void> {
  const key = keyFor(harness);
  if (entries[key]?.models !== undefined && entries[key].models !== null) return Promise.resolve();
  const pending = inFlight.get(key);
  if (pending) return pending;
  entries[key] = { models: null, error: null };
  const request = (async () => {
    try {
      entries[key] = { models: await (harness === "codex" ? listModels() : listHarnessModels(harness)), error: null };
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
  return models()?.find((model) => model.id === id)?.displayName ?? id;
}
