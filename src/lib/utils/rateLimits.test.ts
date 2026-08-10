import { describe, expect, it } from "vitest";
import type { RateLimitSnapshot } from "$lib/types";
import {
  mergeSnapshot,
  primaryUsageWindow,
  resetLabel,
  usageToneClass,
  usageWindows,
  windowLabel,
} from "$lib/utils/rateLimits";

const snapshot: RateLimitSnapshot = {
  limitId: "codex",
  primary: { usedPercent: 34, windowDurationMins: 300, resetsAt: 1000 },
  secondary: { usedPercent: 61, windowDurationMins: 10_080, resetsAt: 5000 },
};

describe("windowLabel", () => {
  it("names windows by their length", () => {
    expect(windowLabel(300)).toBe("5h");
    expect(windowLabel(1440)).toBe("Daily");
    expect(windowLabel(10_080)).toBe("Weekly");
    expect(windowLabel(43_200)).toBe("Monthly");
    expect(windowLabel(null)).toBe("Usage");
  });
});

describe("usageWindows", () => {
  it("orders windows shortest first and reports the remainder", () => {
    const windows = usageWindows(snapshot);
    expect(windows.map((window) => window.label)).toEqual(["5h", "Weekly"]);
    expect(windows[1].remainingPercent).toBe(39);
  });

  it("drops windows Codex did not report", () => {
    expect(usageWindows({ primary: null, secondary: null })).toEqual([]);
    expect(usageWindows(null)).toEqual([]);
  });

  it("clamps out-of-range percentages", () => {
    const [window] = usageWindows({ primary: { usedPercent: 140, windowDurationMins: 300 } });
    expect(window.usedPercent).toBe(100);
    expect(window.remainingPercent).toBe(0);
  });
});

describe("primaryUsageWindow", () => {
  it("prefers the weekly window", () => {
    expect(primaryUsageWindow(snapshot)?.label).toBe("Weekly");
  });

  it("falls back to the longest window when there is no weekly one", () => {
    const fallback = primaryUsageWindow({
      primary: { usedPercent: 10, windowDurationMins: 300 },
      secondary: { usedPercent: 20, windowDurationMins: 1440 },
    });
    expect(fallback?.label).toBe("Daily");
  });
});

describe("resetLabel", () => {
  const now = 1_000_000_000_000;

  it("formats minute, hour and day countdowns", () => {
    expect(resetLabel(now / 1000 + 90 * 60, now)).toBe("resets in 1h 30m");
    expect(resetLabel(now / 1000 + 45 * 60, now)).toBe("resets in 45m");
    expect(resetLabel(now / 1000 + 52 * 3600, now)).toBe("resets in 2d 4h");
    expect(resetLabel(now / 1000 - 10, now)).toBe("resets now");
    expect(resetLabel(null, now)).toBeNull();
  });
});

describe("usageToneClass", () => {
  it("escalates as the window fills", () => {
    expect(usageToneClass(10)).toContain("primary");
    expect(usageToneClass(80)).toContain("warning");
    expect(usageToneClass(95)).toContain("error");
  });
});

describe("mergeSnapshot", () => {
  it("keeps fields a sparse rolling update omits", () => {
    const merged = mergeSnapshot(snapshot, {
      primary: { usedPercent: 40, windowDurationMins: 300, resetsAt: 1200 },
    });
    expect(merged.primary?.usedPercent).toBe(40);
    expect(merged.secondary?.usedPercent).toBe(61);
    expect(merged.limitId).toBe("codex");
  });

  it("uses the update as-is when nothing was known", () => {
    expect(mergeSnapshot(null, snapshot)).toEqual(snapshot);
  });
});
