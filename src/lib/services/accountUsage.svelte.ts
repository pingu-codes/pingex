import { readAccountRateLimits } from "$lib/services/api";
import { currentHomeKey } from "$lib/services/homeRouting";
import type { RateLimitSnapshot } from "$lib/types";
import { mergeSnapshot } from "$lib/utils/rateLimits";

type Usage = { snapshot: RateLimitSnapshot | null; byLimitId: Record<string, RateLimitSnapshot>; error: string | null };
const homes = $state<Record<string, Usage>>({});
const empty: Usage = { snapshot: null, byLimitId: {}, error: null };
const current = () => homes[currentHomeKey("codex")] ?? empty;

export const accountUsage = {
  get snapshot() {
    return current().snapshot;
  },
  set snapshot(value: RateLimitSnapshot | null) {
    usageFor(currentHomeKey("codex")).snapshot = value;
  },
  get byLimitId() {
    return current().byLimitId;
  },
  set byLimitId(value: Record<string, RateLimitSnapshot>) {
    usageFor(currentHomeKey("codex")).byLimitId = value;
  },
  get error() {
    return current().error;
  },
  set error(value: string | null) {
    usageFor(currentHomeKey("codex")).error = value;
  },
};

function usageFor(key: string): Usage {
  homes[key] ??= { snapshot: null, byLimitId: {}, error: null };
  return homes[key];
}

export function applyRateLimitUpdate(update: RateLimitSnapshot, homeKey = currentHomeKey("codex")): void {
  const usage = usageFor(homeKey);
  usage.snapshot = mergeSnapshot(usage.snapshot, update);
  if (update.limitId) usage.byLimitId[update.limitId] = mergeSnapshot(usage.byLimitId[update.limitId] ?? null, update);
  usage.error = null;
}

export async function refreshAccountUsage(): Promise<void> {
  const key = currentHomeKey("codex");
  try {
    const response = await readAccountRateLimits();
    if (response?.rateLimits) applyRateLimitUpdate(response.rateLimits, key);
    const usage = usageFor(key);
    for (const snapshot of Object.values(response?.rateLimitsByLimitId ?? {})) {
      if (snapshot.limitId) usage.byLimitId[snapshot.limitId] = snapshot;
    }
  } catch (cause) {
    usageFor(key).error = cause instanceof Error ? cause.message : String(cause);
  }
}
