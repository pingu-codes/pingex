import { beforeEach, describe, expect, it } from "vitest";
import { isStale, loadHideOldThreads, sidebarPrefs, THREAD_AGE_LIMIT_SECONDS } from "./sidebarPrefs.svelte";

describe("sidebarPrefs", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("hides old threads by default", () => {
    expect(loadHideOldThreads()).toBe(true);
  });

  it("round-trips the switch", () => {
    sidebarPrefs.setHideOldThreads(false);
    expect(sidebarPrefs.hideOldThreads).toBe(false);
    expect(loadHideOldThreads()).toBe(false);
    sidebarPrefs.setHideOldThreads(true);
    expect(loadHideOldThreads()).toBe(true);
  });

  it("treats threads over a day old as stale", () => {
    const now = 1_000_000;
    expect(isStale(now, now)).toBe(false);
    expect(isStale(now - THREAD_AGE_LIMIT_SECONDS, now)).toBe(false);
    expect(isStale(now - THREAD_AGE_LIMIT_SECONDS - 1, now)).toBe(true);
  });
});
