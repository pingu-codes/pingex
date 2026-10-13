import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flashFlag, flashKey } from "$lib/utils/flash.svelte";

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
});

afterEach(() => {
  vi.useRealTimers();
});

describe("flashFlag", () => {
  it("is on for the given time after a trigger", () => {
    const copied = flashFlag(1000);
    expect(copied.on).toBe(false);
    copied.trigger();
    expect(copied.on).toBe(true);
    vi.advanceTimersByTime(999);
    expect(copied.on).toBe(true);
    vi.advanceTimersByTime(1);
    expect(copied.on).toBe(false);
  });

  it("restarts the timer on re-trigger", () => {
    const copied = flashFlag(1000);
    copied.trigger();
    vi.advanceTimersByTime(800);
    copied.trigger();
    vi.advanceTimersByTime(800);
    expect(copied.on).toBe(true);
    vi.advanceTimersByTime(200);
    expect(copied.on).toBe(false);
  });
});

describe("flashKey", () => {
  it("remembers which key flashed last, for the full time", () => {
    const copied = flashKey<"absolute" | "relative">(1000);
    copied.trigger("absolute");
    vi.advanceTimersByTime(600);
    copied.trigger("relative");
    expect(copied.current).toBe("relative");
    vi.advanceTimersByTime(600);
    expect(copied.current).toBe("relative");
    vi.advanceTimersByTime(400);
    expect(copied.current).toBeNull();
  });

  it("clear() ends the flash and its timer", () => {
    const copied = flashKey<string>(1000);
    copied.trigger("a");
    copied.clear();
    expect(copied.current).toBeNull();
    copied.trigger("b");
    vi.advanceTimersByTime(999);
    expect(copied.current).toBe("b");
  });
});
