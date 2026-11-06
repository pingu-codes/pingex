// Syntax highlighting for a changed file's diff rows on the review page.
//
// Each hunk is highlighted as two texts: the base side (context and removed
// lines) and the head side (context and added lines). A removed line takes its
// colour from the base text and every other line from the head, so each reads
// as the code it came from, and a comment opened on one side never bleeds into
// the other. State does not carry across the gap between hunks.

import type { PrFile } from "$lib/types";
import { languageForPath } from "$lib/utils/codeLanguage";
import { DIFF_HIGHLIGHT_BYTES, highlightLines } from "$lib/utils/markdown";

/** A file's highlighted rows by `diffRows` key, memoised per file. */
const memo = new WeakMap<PrFile, Map<string, string> | null>();

/**
 * Highlighted HTML for each diff line of `file`, keyed like `diffRows`
 * (`h{hunk}l{line}`), or `null` to show it as plain text: a language
 * highlight.js has no grammar for, a diff with no text, or one too large to
 * highlight without lag.
 */
export function highlightedRows(file: PrFile): Map<string, string> | null {
  if (memo.has(file)) return memo.get(file) ?? null;
  const rows = compute(file);
  memo.set(file, rows);
  return rows;
}

function compute(file: PrFile): Map<string, string> | null {
  if (file.patchTruncated) return null;
  const language = languageForPath(file.path);
  if (language === "plaintext") return null;
  let bytes = 0;
  for (const hunk of file.hunks) for (const line of hunk.lines) bytes += line.content.length + 1;
  if (bytes === 0 || bytes > DIFF_HIGHLIGHT_BYTES) return null;

  const rows = new Map<string, string>();
  file.hunks.forEach((hunk, hunkIndex) => {
    const base = hunk.lines.filter((line) => line.kind !== "add");
    const head = hunk.lines.filter((line) => line.kind !== "del");
    const baseHtml = highlightLines(base.map((line) => line.content).join("\n"), language);
    const headHtml = highlightLines(head.map((line) => line.content).join("\n"), language);
    let baseAt = 0;
    let headAt = 0;
    hunk.lines.forEach((line, lineIndex) => {
      const html = line.kind === "del" ? baseHtml[baseAt] : headHtml[headAt];
      if (line.kind !== "add") baseAt++;
      if (line.kind !== "del") headAt++;
      if (html !== undefined) rows.set(`h${hunkIndex}l${lineIndex}`, html);
    });
  });
  return rows;
}
