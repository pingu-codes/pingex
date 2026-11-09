// What Cmd+F searches on the review page, and the search itself.
//
// The Files tab searches the PR's data, because most of its rows are not in
// the DOM (`DiffChunk.svelte`) and collapsed files have none. The other tabs
// are fully mounted, so they search their DOM. Either way the text is a list
// of units in page order; `findPainter.ts` turns a unit's offsets into Ranges.

import type { CommentThread } from "$lib/review/review";
import { type DiffSide, diffRows, entryLocation, groupBy, threadLineKey } from "$lib/review/reviewPage";
import type { Finding, PendingEntry, PrFile } from "$lib/types";
import { renderMarkdown } from "$lib/utils/markdown";

/** One run of text. A match's offsets are into `text`. */
export type FindUnit = {
  /** The element's `data-find` within its file. */
  id: string;
  text: string;
  /** The file it is in, or "" outside the Files tab. */
  path: string;
  /** The diff row it sits under, which must be mounted to reach it. */
  row: string | null;
  /** Read from the DOM (`domUnits`): the nodes `text` joins. */
  nodes?: Text[];
};

export type FindMatch = {
  unit: FindUnit;
  /** The unit's place in the list searched. */
  order: number;
  start: number;
  end: number;
};

/** A piece of text a caller's line annotation shows. */
export type FindText = { id: string; text: string };

/** The unit for a file's path in its header. */
export const PATH_UNIT = "path";

/** Past this the count reads "10,000+" and later text can't be reached. */
export const MAX_MATCHES = 10_000;

const REGEX_SYNTAX = /[\\^$.*+?()[\]{}|/]/g;

/** `query` as literal text. A space matches any whitespace, since rendered
 *  text wraps and code indents with tabs. Not `toLowerCase`, which can change
 *  a string's length (`İ`) and shift every offset after it. */
function matcher(query: string, caseSensitive: boolean): RegExp {
  const source = query.replace(REGEX_SYNTAX, "\\$&").replace(/\s+/g, "\\s+");
  return new RegExp(source, caseSensitive ? "gu" : "giu");
}

export function findMatches(
  units: FindUnit[],
  query: string,
  caseSensitive: boolean,
): { matches: FindMatch[]; capped: boolean } {
  const matches: FindMatch[] = [];
  if (!query) return { matches, capped: false };
  const pattern = matcher(query, caseSensitive);
  for (const [order, unit] of units.entries()) {
    pattern.lastIndex = 0;
    for (let found = pattern.exec(unit.text); found; found = pattern.exec(unit.text)) {
      if (matches.length === MAX_MATCHES) return { matches, capped: true };
      matches.push({ unit, order, start: found.index, end: found.index + found[0].length });
    }
  }
  return { matches, capped: false };
}

// --- the Files tab -----------------------------------------------------------

/** Rendered comment text by markdown source. Bodies are few and repeat on
 *  every search, and rendering them is the costly part. */
const renderedText = new Map<string, string>();
const RENDERED_TEXT_ENTRIES = 2_000;

/** What a reader sees of `body`: `CommentBody`'s element has this same
 *  `textContent`, so offsets line up. Parsed in a `<template>`, which is inert
 *  and fetches no images. */
export function markdownText(body: string): string {
  const cached = renderedText.get(body);
  if (cached !== undefined) return cached;
  const template = document.createElement("template");
  template.innerHTML = renderMarkdown(body);
  const text = template.content.textContent ?? "";
  if (renderedText.size >= RENDERED_TEXT_ENTRIES) renderedText.clear();
  renderedText.set(body, text);
  return text;
}

/** Text for a Finding merged into a human comment, which shows as one line. */
export function mergedFindingLine(agentLabel: string, finding: Finding, sameAs: string | null): string {
  return `${agentLabel} found this too: ${finding.title}. Same as ${sameAs ?? "an existing"} comment.`;
}

/** The text `FindingCard` shows, with the `data-find` of each piece. */
export function findingTexts(
  finding: Finding,
  shown: { agentLabel: string; added: boolean; addedBody: string | null; sameAs: string | null },
): FindText[] {
  const id = (piece: string) => `finding:${finding.id}:${piece}`;
  if (finding.state === "merged") {
    return [{ id: id("merged"), text: mergedFindingLine(shown.agentLabel, finding, shown.sameAs) }];
  }
  const texts: FindText[] = [];
  if (finding.title) texts.push({ id: id("title"), text: finding.title });
  texts.push({ id: id("body"), text: shown.added && shown.addedBody ? shown.addedBody : finding.body });
  if (finding.evidence) texts.push({ id: id("evidence"), text: finding.evidence });
  return texts;
}

/** A file's text in page order, as `FileDiff.svelte` lays it out: its path,
 *  then each row with what sits under that row, then threads off the diff. */
export function fileFindUnits(
  file: PrFile,
  threads: CommentThread[],
  pending: PendingEntry[],
  annotations?: (side: DiffSide, line: number) => FindText[],
): FindUnit[] {
  const { path } = file;
  const units: FindUnit[] = [{ id: PATH_UNIT, text: path, path, row: null }];
  const threadsAt = groupBy(threads, threadLineKey);
  const pendingAt = groupBy(pending, (entry) => {
    const at = entryLocation(entry);
    return `${at.side}:${at.line}`;
  });
  const threadUnits = (found: CommentThread[], row: string | null) => {
    for (const thread of found) {
      for (const comment of thread.comments) {
        units.push({ id: `comment:${comment.id}`, text: markdownText(comment.body), path, row });
      }
    }
  };
  const drawn = new Set<string>();
  for (const row of diffRows(file)) {
    if (row.kind === "hunk") {
      units.push({ id: row.key, text: row.header, path, row: row.key });
      continue;
    }
    units.push({ id: row.key, text: row.line.content, path, row: row.key });
    const line = `${row.side}:${row.number}`;
    drawn.add(line);
    threadUnits(threadsAt.get(line) ?? [], row.key);
    for (const entry of pendingAt.get(line) ?? []) {
      units.push({ id: `pending:${entry.id}`, text: markdownText(entry.body), path, row: row.key });
      if (entry.kind === "suggestion") {
        units.push({ id: `pending:${entry.id}:replacement`, text: entry.replacement, path, row: row.key });
      }
    }
    for (const { id, text } of annotations?.(row.side, row.number) ?? []) {
      units.push({ id, text, path, row: row.key });
    }
  }
  threadUnits(
    threads.filter((thread) => !drawn.has(threadLineKey(thread))),
    null,
  );
  return units;
}

// --- the other tabs ----------------------------------------------------------

/** Elements whose text runs on with the text around them. */
const INLINE_TAGS = new Set(
  "A ABBR B BDI BDO CITE CODE DATA DEL DFN EM FONT I INS KBD MARK Q S SAMP SMALL SPAN STRONG SUB SUP TIME U VAR".split(
    " ",
  ),
);
const NOT_READ = "button, svg, textarea, input, select, script, style, .sr-only, [aria-hidden='true']";

/** The element a text node's run belongs to. A diff row's columns are spans,
 *  but read apart: a line number doesn't run on into the code. */
function blockOf(node: Text, root: Element): Element | null {
  let element = node.parentElement;
  while (
    element &&
    element !== root &&
    INLINE_TAGS.has(element.tagName) &&
    !element.parentElement?.classList.contains("ln")
  ) {
    element = element.parentElement;
  }
  return element;
}

/** The text under `root`, one unit per block, so a match can cross a bold word
 *  or a highlighted token but not run from one paragraph into the next. */
export function domUnits(root: Element): FindUnit[] {
  const units: FindUnit[] = [];
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  let block: Element | null = null;
  let unit: FindUnit | null = null;
  for (let node = walker.nextNode() as Text | null; node; node = walker.nextNode() as Text | null) {
    if (node.parentElement?.closest(NOT_READ)) continue;
    const owner = blockOf(node, root);
    if (unit?.nodes && owner === block) {
      unit.nodes.push(node);
      unit.text += node.data;
    } else if (node.data.trim()) {
      block = owner;
      unit = { id: `dom:${units.length}`, text: node.data, path: "", row: null, nodes: [node] };
      units.push(unit);
    }
  }
  return units;
}
