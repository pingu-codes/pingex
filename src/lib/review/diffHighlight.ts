// Syntax highlighting for a changed file's diff rows on the review page.
//
// Each hunk is highlighted as two texts: the base side (context and removed
// lines) and the head side (context and added lines). A removed line takes its
// colour from the base text and every other line from the head, so each reads
// as the code it came from, and a comment opened on one side never bleeds into
// the other. State does not carry across the gap between hunks.
//
// No DOM here: `highlight.worker.ts` runs this off the main thread, and
// `diffHighlighter.svelte.ts` hands rows to the page as they come back.

import type { PrFile } from "$lib/types";
import { languageForPath } from "$lib/utils/codeLanguage";
import { DIFF_HIGHLIGHT_BYTES, highlightLines } from "$lib/utils/highlightLines";

/** What highlighting a file reads: what the worker is sent. */
export type HighlightSource = Pick<PrFile, "path" | "hunks" | "patchTruncated">;

/** A file's highlighted rows by `diffRows` key, memoised per file. */
const memo = new WeakMap<HighlightSource, Map<string, string> | null>();

/**
 * Highlighted HTML for each diff line of `file`, keyed like `diffRows`
 * (`h{hunk}l{line}`), or `null` to show it as plain text: a language
 * highlight.js has no grammar for, a diff with no text, or one too large to
 * highlight without lag. Synchronous; the page asks `highlightFor` instead.
 */
export function highlightedRows(file: HighlightSource): Map<string, string> | null {
  if (memo.has(file)) return memo.get(file) ?? null;
  const rows = computeRows(file);
  memo.set(file, rows);
  return rows;
}

/** `highlightedRows` without the memo, for the worker. */
export function computeRows(file: HighlightSource): Map<string, string> | null {
  if (file.patchTruncated) return null;
  const language = languageForPath(file.path);
  if (language === "plaintext") return null;
  let bytes = 0;
  for (const hunk of file.hunks) for (const line of hunk.lines) bytes += line.content.length + 1;
  if (bytes === 0 || bytes > DIFF_HIGHLIGHT_BYTES) return null;

  const rows = new Map<string, string>();
  for (const [hunkIndex, hunk] of file.hunks.entries()) {
    const base = hunk.lines.filter((line) => line.kind !== "add");
    const head = hunk.lines.filter((line) => line.kind !== "del");
    const baseHtml = highlightLines(base.map((line) => line.content).join("\n"), language);
    const headHtml = highlightLines(head.map((line) => line.content).join("\n"), language);
    // Markup the check refused: show the whole file plain.
    if (!baseHtml || !headHtml) return null;
    let baseAt = 0;
    let headAt = 0;
    for (const [lineIndex, line] of hunk.lines.entries()) {
      const html = line.kind === "del" ? baseHtml[baseAt] : headHtml[headAt];
      if (line.kind !== "add") baseAt++;
      if (line.kind !== "del") headAt++;
      if (html !== undefined) rows.set(`h${hunkIndex}l${lineIndex}`, html);
    }
  }
  return rows;
}
