import { beforeEach, describe, expect, it } from "vitest";
import { accountUsage, applyRateLimitUpdate, refreshAccountUsage } from "$lib/services/accountUsage.svelte";

beforeEach(() => {
  accountUsage.snapshot = null;
  accountUsage.byLimitId = {};
  accountUsage.error = null;
});

describe("accountUsage", () => {
  it("reads the preview snapshot and its per-model buckets", async () => {
    await refreshAccountUsage();
    expect(accountUsage.snapshot?.primary?.windowDurationMins).toBe(10_080);
    expect(Object.keys(accountUsage.byLimitId).sort()).toEqual(["codex", "codex_spark"]);
    expect(accountUsage.error).toBeNull();
  });

  it("merges sparse rolling updates into the last snapshot", async () => {
    await refreshAccountUsage();
    applyRateLimitUpdate({ limitId: "codex", primary: { usedPercent: 55, windowDurationMins: 10_080 } });
    expect(accountUsage.snapshot?.primary?.usedPercent).toBe(55);
    // planType came from the full read and must survive a partial update.
    expect(accountUsage.snapshot?.planType).toBe("pro");
    expect(accountUsage.byLimitId.codex.primary?.usedPercent).toBe(55);
  });
});
