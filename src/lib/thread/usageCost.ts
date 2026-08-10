import type { ThreadTokenUsage } from "$lib/types";

/** USD per 1M tokens for one model family. */
export interface ModelPrice {
  input: number;
  cachedInput: number;
  output: number;
}

/**
 * List API prices per 1M tokens, matched by model-id prefix (longest first).
 * Codex never reports a price, so any cost we show is an estimate of what the
 * same tokens would cost on the API — a ChatGPT plan is billed by the windows
 * in the rate-limit meter instead. Reasoning tokens are already part of
 * `outputTokens`, so they are not charged again here.
 */
export const MODEL_PRICES: Record<string, ModelPrice> = {
  "gpt-5-nano": { input: 0.05, cachedInput: 0.005, output: 0.4 },
  "gpt-5-mini": { input: 0.25, cachedInput: 0.025, output: 2 },
  "gpt-5": { input: 1.25, cachedInput: 0.125, output: 10 },
};

/** Fallback for unrecognised ids — the GPT-5 family rate. */
export const DEFAULT_PRICE: ModelPrice = MODEL_PRICES["gpt-5"];

export function priceFor(modelId: string | null | undefined): ModelPrice {
  if (!modelId) return DEFAULT_PRICE;
  const id = modelId.toLowerCase();
  const key = Object.keys(MODEL_PRICES)
    .filter((candidate) => id.startsWith(candidate))
    .sort((a, b) => b.length - a.length)[0];
  return key ? MODEL_PRICES[key] : DEFAULT_PRICE;
}

/**
 * Estimated USD spend for a thread's cumulative usage. `cachedInputTokens` is
 * a subset of `inputTokens`, so the uncached remainder is billed at full rate.
 */
export function estimateCost(usage: ThreadTokenUsage | null, modelId: string | null): number | null {
  if (!usage) return null;
  const price = priceFor(modelId);
  const cached = Math.max(usage.total.cachedInputTokens, 0);
  const uncachedInput = Math.max(usage.total.inputTokens - cached, 0);
  const output = Math.max(usage.total.outputTokens, 0);
  return (uncachedInput * price.input + cached * price.cachedInput + output * price.output) / 1_000_000;
}

/** `$0.42`, `$1.20`, or `<$0.01` for a non-zero rounding to nothing. */
export function formatCost(usd: number | null): string | null {
  if (usd === null) return null;
  if (usd > 0 && usd < 0.01) return "<$0.01";
  return `$${usd.toFixed(2)}`;
}
