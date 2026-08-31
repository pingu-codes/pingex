import { beforeEach, describe, expect, it } from "vitest";
import { nextFollowing, recallScroll, rememberScroll, resetScrollPositions } from "./scrollPositions";

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

  describe("nextFollowing", () => {
    it("detaches on any move up, even inside the near-bottom band", () => {
      // 1500 is the very bottom (2000 - 500); 1450 is 50px up, well inside 120px.
      expect(nextFollowing(true, 1500, scroller(1450))).toBe(false);
    });

    it("re-attaches when a move down lands near the bottom", () => {
      expect(nextFollowing(false, 300, scroller(1400))).toBe(true);
    });

    it("stays detached when a move down is still far from the bottom", () => {
      expect(nextFollowing(false, 300, scroller(600))).toBe(false);
    });

    it("keeps the previous state when the offset did not move", () => {
      expect(nextFollowing(true, 1500, scroller(1500))).toBe(true);
      expect(nextFollowing(false, 600, scroller(600))).toBe(false);
    });
  });
});
