import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { debounce } from "$lib/utils/debounce";

describe("debounce", () => {
  // Only fake the timer functions; jsdom's requestAnimationFrame is read-only.
  beforeEach(() => vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] }));
  afterEach(() => vi.useRealTimers());

  it("runs only the last call after the wait elapses", () => {
    const spy = vi.fn();
    const run = debounce(spy, 200);
    run("a");
    run("b");
    run("c");
    expect(spy).not.toHaveBeenCalled();
    vi.advanceTimersByTime(200);
    expect(spy).toHaveBeenCalledTimes(1);
    expect(spy).toHaveBeenCalledWith("c");
  });

  it("cancel() drops a pending call", () => {
    const spy = vi.fn();
    const run = debounce(spy, 200);
    run("a");
    run.cancel();
    vi.advanceTimersByTime(500);
    expect(spy).not.toHaveBeenCalled();
  });
});
