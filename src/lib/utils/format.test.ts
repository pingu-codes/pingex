import { describe, expect, it } from "vitest";
import { parseTokenBudget } from "$lib/thread/goalBudget";
import { formatBytes, formatTokens, formatTokensShort, relativeTime } from "$lib/utils/format";

describe("formatBytes", () => {
  it("formats bytes, kilobytes and megabytes", () => {
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(2048)).toBe("2 KB");
    expect(formatBytes(12_595)).toBe("12.3 KB");
    expect(formatBytes(300_000)).toBe("293 KB");
    expect(formatBytes(3_500_000)).toBe("3.3 MB");
  });
});

describe("token formatting", () => {
  it("groups thousands", () => {
    expect(formatTokens(272_000)).toBe("272,000");
  });

  it("abbreviates large counts", () => {
    expect(formatTokensShort(840)).toBe("840");
    expect(formatTokensShort(1_234)).toBe("1.2k");
    expect(formatTokensShort(128_000)).toBe("128k");
    expect(formatTokensShort(1_250_000)).toBe("1.3M");
    expect(formatTokensShort(2_000_000)).toBe("2M");
  });

  it("round-trips through the budget parser", () => {
    for (const tokens of [250_000, 500_000, 1_000_000, 2_000_000]) {
      expect(parseTokenBudget(formatTokensShort(tokens))).toBe(tokens);
    }
  });
});

describe("relativeTime", () => {
  const now = 1_000_000_000;

  it("counts days by default", () => {
    expect(relativeTime(now - 60, { now })).toBe("Today");
    expect(relativeTime(now - 86_400, { now })).toBe("Yesterday");
    expect(relativeTime(now - 3 * 86_400, { now })).toBe("3d");
    expect(relativeTime(now - 3 * 86_400, { now, suffix: " ago" })).toBe("3d ago");
    expect(relativeTime(now + 600, { now })).toBe("Today");
  });

  it("counts minutes and hours at minute granularity", () => {
    const opts = { now, granularity: "minute", suffix: " ago" } as const;
    expect(relativeTime(now - 30, opts)).toBe("just now");
    expect(relativeTime(now - 5 * 60, opts)).toBe("5m ago");
    expect(relativeTime(now - 2 * 3600, opts)).toBe("2h ago");
    expect(relativeTime(now - 4 * 86_400, opts)).toBe("4d ago");
  });
});
