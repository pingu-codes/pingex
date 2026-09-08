import type { Model } from "$lib/types";

export function fastTier(model: Model | null): string | undefined {
  return model?.speedTiers?.find((tier) => tier.name.toLowerCase() === "fast")?.id;
}

export function fastModeState(tier: string | null | undefined, model: Model | null): boolean | undefined {
  if (tier === undefined) return undefined;
  if (tier === null || tier === "default") return false;
  if (tier === "priority" || tier === "fast") return true;
  const fast = fastTier(model);
  if (!fast) return undefined;
  return tier === fast;
}
