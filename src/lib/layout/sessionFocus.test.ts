import { beforeEach, describe, expect, it } from "vitest";
import { isTouched, resetTouched, touchedThreads, touchThread } from "./sessionFocus.svelte";

describe("sessionFocus", () => {
  beforeEach(() => resetTouched());

  it("starts empty and records touched threads", () => {
    expect(touchedThreads.size).toBe(0);
    touchThread("a");
    touchThread("a");
    expect(isTouched("a")).toBe(true);
    expect(isTouched("b")).toBe(false);
    expect(touchedThreads.size).toBe(1);
  });

  it("ignores empty ids", () => {
    touchThread(null);
    touchThread(undefined);
    touchThread("");
    expect(touchedThreads.size).toBe(0);
  });
});
