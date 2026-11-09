<!--
  Cmd+F on the review page: an input, "3 of 41", match case, previous, next.
  It searches `units`, or the mounted DOM when there are none to give (see
  `reviewFind.ts`), paints what is in the DOM, and repaints as that changes:
  rows mount and unmount as the page scrolls, and the highlight worker replaces
  a row's text nodes. It lives outside the scroller, so it never scrolls away.
-->
<script lang="ts">
  import { CaseSensitive, ChevronDown, ChevronUp, X } from "@lucide/svelte";
  import { onDestroy, onMount, tick, untrack } from "svelte";
  import {
    clearPaint,
    firstVisibleOrder,
    indexMatches,
    type MatchIndex,
    matchFallback,
    paintMatches,
    scrollMatchIntoView,
  } from "$lib/review/findPainter";
  import { scrolling } from "$lib/review/nearViewport";
  import { domUnits, type FindMatch, type FindUnit, findMatches, MAX_MATCHES } from "$lib/review/reviewFind";

  let {
    scroller,
    scope,
    units,
    query = $bindable(""),
    class: className = "",
    onReveal,
    onClose,
  }: {
    /** The page's scroller. Its `[data-find-root]` holds what is searched. */
    scroller: HTMLElement | undefined;
    /** What is being searched: the tab and its filters. A change starts over. */
    scope: string;
    /** The text to search, or `null` to read it from the DOM. */
    units: FindUnit[] | null;
    query?: string;
    class?: string;
    /** About to move to `match`: make its row mountable. */
    onReveal: (match: FindMatch) => void;
    onClose: () => void;
  } = $props();

  /** Long enough to skip the keystrokes of a word, short enough not to notice. */
  const TYPING_DEBOUNCE_MS = 120;

  let input = $state<HTMLInputElement>();
  let caseSensitive = $state(false);
  let matches = $state.raw<FindMatch[]>([]);
  let capped = $state(false);
  let position = $state(-1);
  let index: MatchIndex = new Map();

  const searching = $derived(query.length > 0);
  const countLabel = $derived.by(() => {
    if (!searching) return "";
    if (matches.length === 0) return "No results";
    const total = capped ? `${MAX_MATCHES.toLocaleString()}+` : matches.length.toLocaleString();
    return `${(position + 1).toLocaleString()} of ${total}`;
  });

  const root = () => scroller?.querySelector("[data-find-root]") ?? null;

  function paint(): Range | null {
    return paintMatches(root(), matches, index, matches[position] ?? null);
  }

  /** Why the search ran, which decides where the current match goes. */
  type Reason = "typed" | "scope" | "refresh";

  function nextPosition(reason: Reason, list: FindUnit[], previous: FindMatch | undefined): number {
    if (matches.length === 0) return -1;
    const atOrAfter = (order: number, start = 0) =>
      Math.max(
        0,
        matches.findIndex((match) => match.order > order || (match.order === order && match.start >= start)),
      );
    if (reason === "scope" || !previous) {
      const element = root();
      return atOrAfter(element && scroller ? firstVisibleOrder(element, scroller, list) : 0);
    }
    if (reason === "refresh") {
      const same = matches.findIndex(
        (match) =>
          match.unit.path === previous.unit.path &&
          match.unit.id === previous.unit.id &&
          match.start === previous.start,
      );
      if (same >= 0) return same;
    }
    return atOrAfter(previous.order, previous.start);
  }

  function search(reason: Reason) {
    const element = root();
    const list = units ?? (element ? domUnits(element) : []);
    const previous = reason === "scope" ? undefined : matches[position];
    const found = findMatches(list, query, caseSensitive);
    matches = found.matches;
    capped = found.capped;
    index = indexMatches(found.matches);
    position = nextPosition(reason, list, previous);
    paint();
    if (reason === "typed") void reveal();
  }

  let revealFrame = 0;

  /** Move to the current match. It may be in a collapsed file or an unmounted
   *  chunk, so mount it first, then measure once the update is over: a layout
   *  forced in the middle of a row swap clamps the scroll in WebKit. */
  async function reveal() {
    const match = matches[position];
    if (!match) return;
    onReveal(match);
    await tick();
    cancelAnimationFrame(revealFrame);
    revealFrame = requestAnimationFrame(() => {
      const element = root();
      if (!element || !scroller || matches[position] !== match) return;
      const target = paint() ?? (match.unit.nodes ? null : matchFallback(element, match));
      if (target) scrollMatchIntoView(target, scroller);
    });
  }

  function move(step: number) {
    if (matches.length === 0) return;
    position = (position + step + matches.length) % matches.length;
    void reveal();
  }

  export function next() {
    move(1);
  }

  export function previous() {
    move(-1);
  }

  export function focus() {
    input?.focus();
    input?.select();
  }

  // Search again when the query or the text changes. Only typing waits.
  let lastQuery = "";
  let lastCase = false;
  let lastScope: string | null = null;
  let typingTimer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    const nextQuery = query;
    const nextCase = caseSensitive;
    const nextScope = scope;
    void units;
    untrack(() => {
      const typed = nextQuery !== lastQuery || nextCase !== lastCase;
      const moved = lastScope !== null && nextScope !== lastScope;
      lastQuery = nextQuery;
      lastCase = nextCase;
      lastScope = nextScope;
      clearTimeout(typingTimer);
      if (typed && nextQuery) typingTimer = setTimeout(() => search("typed"), TYPING_DEBOUNCE_MS);
      else search(moved ? "scope" : "refresh");
    });
  });

  // Repaint once the page holds still. Repainting a common word as each chunk
  // mounts costs WebKit 125ms frames while scrolling (`test:perf`), and rows
  // scrolled past don't need it.
  let repaintFrame = 0;
  function repaintWhenStill() {
    if (repaintFrame) return;
    repaintFrame = requestAnimationFrame(() => {
      repaintFrame = 0;
      if (scrolling()) repaintWhenStill();
      // Text read from the DOM may have changed; data-backed text changes through `units`.
      else if (units) paint();
      else search("refresh");
    });
  }

  // Only while there is something to paint: it runs on every mounted chunk.
  $effect(() => {
    if (!scroller || !searching) return;
    const observer = new MutationObserver(repaintWhenStill);
    observer.observe(scroller, { childList: true, subtree: true });
    return () => {
      observer.disconnect();
      cancelAnimationFrame(repaintFrame);
      repaintFrame = 0;
    };
  });

  onMount(focus);

  onDestroy(() => {
    clearTimeout(typingTimer);
    cancelAnimationFrame(revealFrame);
    clearPaint();
  });

  function onKeydown(event: KeyboardEvent) {
    // An IME's Enter accepts its text.
    if (event.isComposing || event.keyCode === 229) return;
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
    } else if (event.key === "Enter") {
      // Handled even with a modifier, or ⌘↵ would submit the review from here.
      event.preventDefault();
      move(event.shiftKey ? -1 : 1);
    }
  }
</script>

<div class="popover-card absolute top-2 z-30 flex items-center gap-1 p-1 {className}" role="search" aria-label="Find">
  <input
    bind:this={input}
    bind:value={query}
    type="text"
    class="field h-7 w-44 text-xs"
    placeholder="Find on this tab"
    aria-label="Find on this tab"
    autocomplete="off"
    autocapitalize="off"
    spellcheck="false"
    onkeydown={onKeydown}
  >
  <span class="min-w-16 text-right text-[11px] text-surface-500 tabular-nums" aria-live="polite">{countLabel}</span>
  <button
    type="button"
    class="btn-icon btn-icon-sm {caseSensitive ? "preset-tonal-primary" : "hover:preset-tonal text-surface-500"}"
    aria-label="Match case"
    title="Match case"
    aria-pressed={caseSensitive}
    onclick={() => (caseSensitive = !caseSensitive)}
  >
    <CaseSensitive size={14} aria-hidden="true" />
  </button>
  <button
    type="button"
    class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500"
    aria-label="Previous match"
    title="Previous match (⇧↵)"
    disabled={matches.length === 0}
    onclick={previous}
  >
    <ChevronUp size={14} aria-hidden="true" />
  </button>
  <button
    type="button"
    class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500"
    aria-label="Next match"
    title="Next match (↵)"
    disabled={matches.length === 0}
    onclick={next}
  >
    <ChevronDown size={14} aria-hidden="true" />
  </button>
  <button
    type="button"
    class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500"
    aria-label="Close find"
    title="Close (Esc)"
    onclick={onClose}
  >
    <X size={14} aria-hidden="true" />
  </button>
</div>
