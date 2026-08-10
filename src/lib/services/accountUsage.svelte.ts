import { readAccountRateLimits } from "$lib/services/api";
import type { RateLimitSnapshot } from "$lib/types";
import { mergeSnapshot } from "$lib/utils/rateLimits";

/**
 * Latest account rate-limit snapshot, shared by every usage meter in the app.
 * Codex pushes `account/rateLimits/updated` during turns; `refreshAccountUsage`
 * covers the cold start before the first turn of a session.
 */
export const accountUsage = $state<{
  snapshot: RateLimitSnapshot | null;
  /** Per-model buckets (e.g. the separate Spark limit), from the last full read. */
  byLimitId: Record<string, RateLimitSnapshot>;
  error: string | null;
}>({
  snapshot: null,
  byLimitId: {},
  error: null,
});

/** Apply a (possibly sparse) rolling update from Codex. */
export function applyRateLimitUpdate(update: RateLimitSnapshot): void {
  accountUsage.snapshot = mergeSnapshot(accountUsage.snapshot, update);
  if (update.limitId) {
    accountUsage.byLimitId[update.limitId] = mergeSnapshot(accountUsage.byLimitId[update.limitId] ?? null, update);
  }
  accountUsage.error = null;
}

export async function refreshAccountUsage(): Promise<void> {
  try {
    const response = await readAccountRateLimits();
    if (response?.rateLimits) applyRateLimitUpdate(response.rateLimits);
    for (const snapshot of Object.values(response?.rateLimitsByLimitId ?? {})) {
      if (snapshot.limitId) accountUsage.byLimitId[snapshot.limitId] = snapshot;
    }
  } catch (cause) {
    // A missing snapshot only hides the meter, so keep this non-fatal.
    accountUsage.error = cause instanceof Error ? cause.message : String(cause);
  }
}
