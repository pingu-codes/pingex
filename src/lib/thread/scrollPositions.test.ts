import { beforeEach, describe, expect, it } from "vitest";
import { recallScroll, rememberScroll, resetScrollPositions } from "./scrollPositions";

function scroller(scrollTop: number, scrollHeight = 2000, clientHeight = 500): HTMLElement {
  return { scrollTop, scrollHeight, clientHeight } as HTMLElement;
}

describe("scrollPositions", () => {
  beforeEach(() => resetScrollPositions());

  it("returns null for a thread never scrolled", () => {
    expect(recallScroll("t1")).toBeNull();
  });

  it("remembers the offset and whether it was near the bottom", () => {
    rememberScroll("t1", scroller(300));
    expect(recallScroll("t1")).toEqual({ top: 300, atBottom: false });
    rememberScroll("t1", scroller(1400));
    expect(recallScroll("t1")).toEqual({ top: 1400, atBottom: true });
  });

  it("keeps threads apart", () => {
    rememberScroll("a", scroller(10));
    rememberScroll("b", scroller(20));
    expect(recallScroll("a")?.top).toBe(10);
    expect(recallScroll("b")?.top).toBe(20);
  });

  it("reset forgets everything", () => {
    rememberScroll("a", scroller(10));
    resetScrollPositions();
    expect(recallScroll("a")).toBeNull();
  });
});
