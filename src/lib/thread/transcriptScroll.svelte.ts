import { tick } from "svelte";
import { isAtBottom, isFarFromBottom, nextFollowing, recallScroll, rememberScroll } from "$lib/thread/scrollPositions";

/** How long a scroll towards the bottom keeps counting as intent to return. */
export const RECENT_DOWN_SCROLL_MS = 2000;

/**
 * The transcript scroller's follow state: whether it tracks the live bottom as
 * content streams in, where it was left per thread, and when the "Jump to
 * latest" pill earns its place.
 */
export class TranscriptScroll {
  /** Whether the transcript tracks the live bottom. Only user scrolling changes it;
   *  position is a poor proxy for intent while content streams in fast. */
  following = $state(true);
  /** Measured: the scroller is pinned to the very bottom. Gates the jump pill so
   *  a wheel-up that cannot move anything (rubber-band at the end) never shows it. */
  #atBottom = $state(true);
  /** Measured: the latest content is well out of view (see `isFarFromBottom`). */
  #farFromBottom = $state(false);
  /** The user scrolled towards the bottom within the last `RECENT_DOWN_SCROLL_MS`.
   *  Read as intent to return to the latest content; scrolling up (reading back
   *  through history) clears it at once so the pill never covers what is being read. */
  #recentDownScroll = $state(false);
  #recentDownScrollTimer: ReturnType<typeof setTimeout> | null = null;
  #element: HTMLElement | null = null;
  /** Set right before we assign `scrollTop` so the resulting scroll event is not
   *  mistaken for the user's. One-shot, deliberately not reactive. */
  #programmaticScroll = false;
  #lastScrollTop = 0;
  /** Thread whose remembered scroll offset has been applied. */
  #restoredFor: string | null = null;
  readonly #threadId: () => string | null;

  /** `threadId` names the thread on show, whose offset each scroll remembers. */
  constructor(threadId: () => string | null) {
    this.#threadId = threadId;
  }

  get showJumpPill(): boolean {
    return !this.following && !this.#atBottom && this.#farFromBottom && this.#recentDownScroll;
  }

  /** Attachment for the scrolling element. */
  attach = (element: HTMLElement) => {
    this.#element = element;
    return () => {
      if (this.#element === element) this.#element = null;
      this.#clearRecentDownScroll();
    };
  };

  /** Keep up with new content, or with `force` re-attach to the bottom first. */
  follow = (force = false): void => {
    const element = this.#element;
    if (!element) return;
    if (force) this.following = true;
    if (!this.following) {
      // Streamed content grows scrollHeight without a scroll event.
      this.#measureBottom(element);
      return;
    }
    requestAnimationFrame(() => this.#scrollProgrammatically(element, element.scrollHeight));
  };

  onScroll = (): void => {
    const element = this.#element;
    if (!element) return;
    if (this.#programmaticScroll) {
      this.#programmaticScroll = false;
    } else {
      this.following = nextFollowing(this.following, this.#lastScrollTop, element);
      this.#noteScrollDirection(element);
    }
    this.#lastScrollTop = element.scrollTop;
    this.#measureBottom(element);
    const id = this.#threadId();
    if (id) rememberScroll(id, element);
  };

  /** A wheel-up and a programmatic scroll-down can land in the same frame, so the
   *  net scroll event alone may read as "moved down"; detach on the intent itself. */
  onWheel = (event: WheelEvent): void => {
    if (event.deltaY < 0) this.following = false;
  };

  /** Put thread `id`'s transcript back where it was left, once it has rendered.
   *  Only the first call per thread does anything. */
  restore(id: string): void {
    if (this.#restoredFor === id) return;
    this.#restoredFor = id;
    const saved = recallScroll(id);
    if (!saved) return;
    tick().then(() => {
      requestAnimationFrame(() => {
        const element = this.#element;
        if (!element || this.#threadId() !== id) return;
        this.following = saved.atBottom;
        this.#scrollProgrammatically(element, saved.atBottom ? element.scrollHeight : saved.top);
        this.#lastScrollTop = element.scrollTop;
        this.#measureBottom(element);
        this.#clearRecentDownScroll();
      });
    });
  }

  /** Assign `scrollTop` without the resulting scroll event flipping `following`.
   *  When nothing moves no event fires, so drop the guard at once rather than let
   *  it swallow the next real user scroll. */
  #scrollProgrammatically(element: HTMLElement, top: number): void {
    const before = element.scrollTop;
    this.#programmaticScroll = true;
    element.scrollTop = top;
    if (element.scrollTop === before) this.#programmaticScroll = false;
  }

  #measureBottom(element: HTMLElement): void {
    this.#atBottom = isAtBottom(element);
    this.#farFromBottom = isFarFromBottom(element);
  }

  #noteScrollDirection(element: HTMLElement): void {
    if (element.scrollTop > this.#lastScrollTop) {
      this.#recentDownScroll = true;
      if (this.#recentDownScrollTimer) clearTimeout(this.#recentDownScrollTimer);
      this.#recentDownScrollTimer = setTimeout(() => {
        this.#recentDownScroll = false;
        this.#recentDownScrollTimer = null;
      }, RECENT_DOWN_SCROLL_MS);
    } else if (element.scrollTop < this.#lastScrollTop) {
      this.#clearRecentDownScroll();
    }
  }

  #clearRecentDownScroll(): void {
    this.#recentDownScroll = false;
    if (this.#recentDownScrollTimer) clearTimeout(this.#recentDownScrollTimer);
    this.#recentDownScrollTimer = null;
  }
}
