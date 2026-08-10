import { describe, expect, it } from "vitest";
import { estimateCost, formatCost, priceFor } from "$lib/thread/usageCost";
import type { ThreadTokenUsage } from "$lib/types";

const usage: ThreadTokenUsage = {
  total: {
    totalTokens: 1_200_000,
    inputTokens: 1_000_000,
    cachedInputTokens: 800_000,
    outputTokens: 200_000,
    reasoningOutputTokens: 120_000,
  },
  last: {
    totalTokens: 60_000,
    inputTokens: 50_000,
    cachedInputTokens: 40_000,
    outputTokens: 10_000,
    reasoningOutputTokens: 6_000,
  },
  modelContextWindow: 272_000,
};

describe("priceFor", () => {
  it("matches the longest model-id prefix", () => {
    expect(priceFor("gpt-5-mini")).toEqual(priceFor("gpt-5-mini-2025"));
    expect(priceFor("gpt-5-mini").output).toBe(2);
    expect(priceFor("gpt-5.2-codex").output).toBe(10);
  });

  it("falls back to the GPT-5 rate for unknown ids", () => {
    expect(priceFor("something-else")).toEqual(priceFor("gpt-5"));
    expect(priceFor(null)).toEqual(priceFor("gpt-5"));
  });
});

describe("estimateCost", () => {
  it("bills cached input at the cached rate and skips reasoning double-counting", () => {
    // 200k uncached in @ $1.25 + 800k cached @ $0.125 + 200k out @ $10 per 1M.
    expect(estimateCost(usage, "gpt-5.2-codex")).toBeCloseTo(0.25 + 0.1 + 2, 6);
  });

  it("returns null without usage", () => {
    expect(estimateCost(null, "gpt-5.2-codex")).toBeNull();
  });
});

describe("formatCost", () => {
  it("formats dollars and sub-cent spend", () => {
    expect(formatCost(2.345)).toBe("$2.35");
    expect(formatCost(0.004)).toBe("<$0.01");
    expect(formatCost(0)).toBe("$0.00");
    expect(formatCost(null)).toBeNull();
  });
});
