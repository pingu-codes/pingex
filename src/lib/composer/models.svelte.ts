/**
 * The model list, fetched once per window and shared.
 *
 * Several places need it — the composer's picker, the quick-chat window, and
 * the transcript, which only wants to turn a stored model id into the name the
 * user picked. Each fetching its own copy would mean several `model/list`
 * round-trips and no way for a component that never opens a picker to label
 * anything, so the list lives here and the in-flight request is shared.
 */
import { listModels } from "$lib/services/api";
import type { Model } from "$lib/types";

const state = $state<{ models: Model[] | null; error: string | null }>({ models: null, error: null });
let inFlight: Promise<void> | null = null;

/** The models, or null before the first successful fetch. */
export function models(): Model[] | null {
  return state.models;
}

/** Why the list could not be fetched, if it could not be. */
export function modelsError(): string | null {
  return state.error;
}

/** Fetch the list unless it is already loaded or on its way. */
export function ensureModels(): Promise<void> {
  if (state.models !== null) return Promise.resolve();
  inFlight ??= (async () => {
    try {
      state.models = await listModels();
      state.error = null;
    } catch (cause) {
      state.error = cause instanceof Error ? cause.message : String(cause);
      state.models = [];
    } finally {
      inFlight = null;
    }
  })();
  return inFlight;
}

/** The name the user knows a model by, falling back to its raw id. */
export function modelLabel(id: string): string {
  return state.models?.find((model) => model.id === id)?.displayName ?? id;
}
