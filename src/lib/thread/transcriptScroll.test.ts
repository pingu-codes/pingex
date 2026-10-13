import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { recallScroll, rememberScroll, resetScrollPositions } from "$lib/thread/scrollPositions";
import { RECENT_DOWN_SCROLL_MS, TranscriptScroll } from "$lib/thread/transcriptScroll.svelte";

/** A 10 000px transcript in a 500px viewport, scrolled to `scrollTop`. */
function scroller(scrollTop = 0): HTMLElement {
  return { scrollTop, scrollHeight: 10_000, clientHeight: 500 } as HTMLElement;
}

const BOTTOM = 9_500;

function setup(threadId: string | null = "t1", element = scroller()) {
  const transcript = new TranscriptScroll(() => threadId);
  const detach = transcript.attach(element);
  /** The user moved the scroller to `top`. */
  const userScroll = (top: number) => {
    element.scrollTop = top;
    transcript.onScroll();
  };
  return { transcript, element, detach, userScroll };
}

/** A transcript the user has scrolled to the bottom of. */
function atBottom() {
  const setUp = setup("t1");
  setUp.userScroll(BOTTOM);
  return setUp;
}

describe("TranscriptScroll", () => {
  beforeEach(() => {
    resetScrollPositions();
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
  });
  afterEach(() => vi.useRealTimers());

  it("follows new content to the bottom without mistaking that for the user", () => {
    const { transcript, element } = setup();
    transcript.follow();
    vi.advanceTimersByTime(0);
    expect(element.scrollTop).toBe(10_000);
    // The scroll event our own assignment raises leaves following alone.
    transcript.onScroll();
    expect(transcript.following).toBe(true);
  });

  it("detaches when the user scrolls or wheels up, and stops following", () => {
    const { transcript, element, userScroll } = atBottom();
    userScroll(BOTTOM - 10);
    expect(transcript.following).toBe(false);
    transcript.follow();
    vi.advanceTimersByTime(0);
    expect(element.scrollTop).toBe(BOTTOM - 10);

    transcript.follow(true);
    expect(transcript.following).toBe(true);
    transcript.onWheel(new WheelEvent("wheel", { deltaY: -1 }));
    expect(transcript.following).toBe(false);
  });

  it("offers the jump pill only while heading back down from far up", () => {
    const { transcript, userScroll } = atBottom();
    userScroll(1_000);
    expect(transcript.showJumpPill).toBe(false);
    userScroll(1_500);
    expect(transcript.showJumpPill).toBe(true);
    vi.advanceTimersByTime(RECENT_DOWN_SCROLL_MS);
    expect(transcript.showJumpPill).toBe(false);

    userScroll(2_000);
    expect(transcript.showJumpPill).toBe(true);
    // Reading back up through history hides it at once.
    userScroll(1_800);
    expect(transcript.showJumpPill).toBe(false);
  });

  it("remembers where each thread was left and restores it once", async () => {
    const { userScroll } = setup("t1");
    userScroll(1_234);
    expect(recallScroll("t1")).toEqual({ top: 1_234, atBottom: false });

    const reopened = setup("t1");
    reopened.transcript.restore("t1");
    await vi.runAllTimersAsync();
    expect(reopened.element.scrollTop).toBe(1_234);
    expect(reopened.transcript.following).toBe(false);

    reopened.element.scrollTop = 0;
    reopened.transcript.restore("t1");
    await vi.runAllTimersAsync();
    expect(reopened.element.scrollTop).toBe(0);
  });

  it("brings a thread left at the bottom back to the new bottom", async () => {
    rememberScroll("t1", scroller(BOTTOM));
    const { transcript, element } = setup("t1");
    transcript.restore("t1");
    await vi.runAllTimersAsync();
    expect(element.scrollTop).toBe(10_000);
    expect(transcript.following).toBe(true);
  });
});
