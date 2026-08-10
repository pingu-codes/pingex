import { describe, expect, it } from "vitest";
import { BASELINE_TOKENS, contextStats, formatTokens, formatTokensShort } from "$lib/thread/contextUsage";
import type { ThreadTokenUsage } from "$lib/types";

function usage(lastTotal: number, contextWindow: number | null, sessionTotal = lastTotal): ThreadTokenUsage {
  const breakdown = (totalTokens: number) => ({
    totalTokens,
    inputTokens: totalTokens,
    cachedInputTokens: 0,
    cacheWriteInputTokens: 0,
    outputTokens: 0,
    reasoningOutputTokens: 0,
  });
  return { total: breakdown(sessionTotal), last: breakdown(lastTotal), modelContextWindow: contextWindow };
}

describe("contextStats", () => {
  it("reports a full window as empty when only the baseline is used", () => {
    const stats = contextStats(usage(BASELINE_TOKENS, 112_000));
    expect(stats.usedFraction).toBe(0);
    expect(stats.percentUsed).toBe(0);
    expect(stats.percentRemaining).toBe(100);
  });

  it("excludes the baseline from both sides of the ratio", () => {
    // 12k baseline + half of the remaining 100k usable window.
    const stats = contextStats(usage(BASELINE_TOKENS + 50_000, BASELINE_TOKENS + 100_000));
    expect(stats.usedFraction).toBeCloseTo(0.5);
    expect(stats.percentUsed).toBe(50);
    expect(stats.percentRemaining).toBe(50);
  });

  it("clamps once usage runs past the window", () => {
    const stats = contextStats(usage(500_000, 200_000));
    expect(stats.usedFraction).toBe(1);
    expect(stats.percentUsed).toBe(100);
    expect(stats.percentRemaining).toBe(0);
  });

  it("treats a window at or below the baseline as full", () => {
    expect(contextStats(usage(0, BASELINE_TOKENS)).percentUsed).toBe(100);
  });

  it("leaves percentages unknown when Codex reports no context window", () => {
    const stats = contextStats(usage(40_000, null));
    expect(stats.usedFraction).toBeNull();
    expect(stats.percentUsed).toBeNull();
    expect(stats.percentRemaining).toBeNull();
    expect(stats.usedTokens).toBe(40_000);
  });

  it("uses the last request for context size and the running total for the session", () => {
    const stats = contextStats(usage(60_000, 200_000, 350_000));
    expect(stats.usedTokens).toBe(60_000);
    expect(stats.sessionTotalTokens).toBe(350_000);
  });
});

describe("token formatting", () => {
  it("groups thousands", () => {
    expect(formatTokens(272_000)).toBe("272,000");
  });

  it("abbreviates large counts", () => {
    expect(formatTokensShort(840)).toBe("840");
    expect(formatTokensShort(128_000)).toBe("128K");
    expect(formatTokensShort(1_250_000)).toBe("1.3M");
  });
});
