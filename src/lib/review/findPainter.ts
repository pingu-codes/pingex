// Cmd+F's work in the DOM: matches to Ranges, Ranges to the screen.
//
// Matches paint through the CSS Custom Highlight API. Ranges can cross
// highlight.js's token spans and add no nodes, so they can't disturb a row
// swap, and building them reads no layout. A range doesn't survive its row
// being replaced, so the caller repaints whenever the DOM changes.

import type { FindMatch, FindUnit } from "$lib/review/reviewFind";

const MATCH_HIGHLIGHT = "review-find";
const CURRENT_HIGHLIGHT = "review-find-current";
/** Set on the current match's row where the Highlight API is missing. */
const CURRENT_ATTRIBUTE = "data-find-current";
/** Past this only the current match is added: a one-letter query over several
 *  screens of mounted rows, where painting every letter helps nobody. */
const MAX_PAINTED = 5_000;

/** Room the find bar covers at the top of the pane, and a little at the bottom. */
const PANE_TOP_MARGIN = 56;
const PANE_BOTTOM_MARGIN = 40;
/** A diff row's line numbers and marker: a match under them is out of view. */
const GUTTER_WIDTH = 104;

const hasHighlightApi = () => typeof CSS !== "undefined" && "highlights" in CSS && typeof Highlight !== "undefined";

/** Matches by file and unit, for looking up what a mounted element holds. */
export type MatchIndex = Map<string, Map<string, FindMatch[]>>;

export function indexMatches(matches: FindMatch[]): MatchIndex {
  const index: MatchIndex = new Map();
  for (const match of matches) {
    const { path, id } = match.unit;
    let file = index.get(path);
    if (!file) index.set(path, (file = new Map()));
    const own = file.get(id);
    if (own) own.push(match);
    else file.set(id, [match]);
  }
  return index;
}

function textNodes(element: Element): Text[] {
  const nodes: Text[] = [];
  const walker = document.createTreeWalker(element, NodeFilter.SHOW_TEXT);
  for (let node = walker.nextNode(); node; node = walker.nextNode()) nodes.push(node as Text);
  return nodes;
}

/** Offsets into the joined text of `nodes`, as a range's boundaries. */
function boundaries(nodes: Text[], start: number, end: number): StaticRangeInit | null {
  let offset = 0;
  let from: { node: Text; at: number } | null = null;
  for (const node of nodes) {
    const next = offset + node.data.length;
    if (!from && start < next) from = { node, at: start - offset };
    if (from && end <= next) {
      return { startContainer: from.node, startOffset: from.at, endContainer: node, endOffset: end - offset };
    }
    offset = next;
  }
  return null;
}

function liveRange({ startContainer, startOffset, endContainer, endOffset }: StaticRangeInit): Range {
  const range = document.createRange();
  range.setStart(startContainer, startOffset);
  range.setEnd(endContainer, endOffset);
  return range;
}

/** Ranges for the matches now in the DOM under `root`, and the current one's.
 *  An element showing other text than its unit is left unpainted rather than
 *  have the wrong characters marked (a comment open in its edit box has none).
 *
 *  All but the current one's are `StaticRange`s. The browser adjusts every live
 *  Range each time a node is removed, so hundreds of them make each unmounting
 *  chunk slow enough to drop frames in WebKit. A static one on a removed row
 *  just stops painting until the next repaint. */
function collectRanges(
  root: Element,
  matches: FindMatch[],
  index: MatchIndex,
  current: FindMatch | null,
): { ranges: StaticRange[]; currentRange: Range | null } {
  const ranges: StaticRange[] = [];
  let currentRange: Range | null = null;
  const canPaintAll = hasHighlightApi();
  const add = (match: FindMatch, nodes: Text[]) => {
    const isCurrent = match === current;
    if (!isCurrent && (!canPaintAll || ranges.length >= MAX_PAINTED)) return;
    const found = boundaries(nodes, match.start, match.end);
    if (!found) return;
    if (isCurrent) currentRange = liveRange(found);
    else ranges.push(new StaticRange(found));
  };

  if (matches[0]?.unit.nodes) {
    for (const match of matches) add(match, match.unit.nodes as Text[]);
    return { ranges, currentRange };
  }
  for (const section of root.querySelectorAll<HTMLElement>("[data-file]")) {
    const file = index.get(section.dataset.file ?? "");
    if (!file) continue;
    for (const element of section.querySelectorAll<HTMLElement>("[data-find]")) {
      const own = file.get(element.dataset.find ?? "");
      if (!own || element.textContent !== own[0].unit.text) continue;
      const nodes = textNodes(element);
      for (const match of own) add(match, nodes);
    }
  }
  return { ranges, currentRange };
}

function markCurrentRow(range: Range | null) {
  document.querySelector(`[${CURRENT_ATTRIBUTE}]`)?.removeAttribute(CURRENT_ATTRIBUTE);
  const element = range?.startContainer.parentElement;
  (element?.closest(".ln, .comment-body, pre") ?? element)?.setAttribute(CURRENT_ATTRIBUTE, "");
}

/** Paint `matches`, returning the current match's Range if it is in the DOM. */
export function paintMatches(
  root: Element | null,
  matches: FindMatch[],
  index: MatchIndex,
  current: FindMatch | null,
): Range | null {
  const { ranges, currentRange } = root
    ? collectRanges(root, matches, index, current)
    : { ranges: [], currentRange: null };
  if (!hasHighlightApi()) {
    markCurrentRow(currentRange);
    return currentRange;
  }
  CSS.highlights.set(MATCH_HIGHLIGHT, new Highlight(...ranges));
  if (currentRange) {
    const highlight = new Highlight(currentRange);
    highlight.priority = 1;
    CSS.highlights.set(CURRENT_HIGHLIGHT, highlight);
  } else {
    CSS.highlights.delete(CURRENT_HIGHLIGHT);
  }
  return currentRange;
}

/** The highlights are global, and the page remounts on each PR. */
export function clearPaint() {
  if (hasHighlightApi()) {
    CSS.highlights.delete(MATCH_HIGHLIGHT);
    CSS.highlights.delete(CURRENT_HIGHLIGHT);
  } else {
    markCurrentRow(null);
  }
}

/** Where `match` is when it can't be painted: its row, or its file. */
export function matchFallback(root: Element, match: FindMatch): Element | null {
  const { path, row } = match.unit;
  const section = root.querySelector(`[data-file="${CSS.escape(path)}"]`);
  return (row && section?.querySelector(`[data-find="${CSS.escape(row)}"]`)) || section;
}

/** The first unit at or below the top of `pane`, where a new search starts. */
export function firstVisibleOrder(root: Element, pane: Element, units: FindUnit[]): number {
  const top = pane.getBoundingClientRect().top;
  const below = (element: Element | null | undefined) => !!element && element.getBoundingClientRect().bottom > top;
  if (units[0]?.nodes) {
    return Math.max(
      0,
      units.findIndex((unit) => below(unit.nodes?.[0].parentElement)),
    );
  }
  for (const element of root.querySelectorAll<HTMLElement>("[data-find]")) {
    if (!below(element)) continue;
    const path = element.closest<HTMLElement>("[data-file]")?.dataset.file;
    const id = element.dataset.find;
    return Math.max(
      0,
      units.findIndex((unit) => unit.path === path && unit.id === id),
    );
  }
  return 0;
}

/** Scroll each scrolling ancestor up to `pane` just enough to show `target`:
 *  a file's sideways scroll, a log's own, then the pane. By hand, because
 *  `scrollIntoView` also moves `overflow: hidden` ancestors. At once, because a
 *  smooth scroll would mount every file it passes. Forces layout, so call it
 *  outside a Svelte flush. */
export function scrollMatchIntoView(target: Range | Element, pane: Element) {
  const start = target instanceof Range ? target.startContainer.parentElement : target;
  const rect = () =>
    typeof target.getBoundingClientRect === "function"
      ? target.getBoundingClientRect()
      : (start as Element).getBoundingClientRect();
  for (let element = start; element; element = element.parentElement) {
    const style = getComputedStyle(element);
    if (/auto|scroll/.test(style.overflowX) && element.scrollWidth > element.clientWidth) {
      const box = element.getBoundingClientRect();
      const at = rect();
      if (at.left < box.left + GUTTER_WIDTH || at.right > box.right) {
        element.scrollLeft += at.left - box.left - box.width / 2;
      }
    }
    if (/auto|scroll/.test(style.overflowY) && element.scrollHeight > element.clientHeight) {
      const box = element.getBoundingClientRect();
      const at = rect();
      if (at.top < box.top + PANE_TOP_MARGIN || at.bottom > box.bottom - PANE_BOTTOM_MARGIN) {
        element.scrollTop += at.top - box.top - box.height / 2;
      }
    }
    if (element === pane) break;
  }
}
