// highlight.js output split into lines, with no DOM: the review diff
// highlights in a Web Worker, where DOMPurify (and `markdown.ts`) can't run.

import hljs from "highlight.js/lib/common";

/** Above this a diff is only escaped: highlight.js on a megabyte of generated text lags the UI. */
export const DIFF_HIGHLIGHT_BYTES = 100_000;

/** The only markup highlight.js writes: a classed span and its close. */
const HLJS_TAG = /<span class="[\w -]*">|<\/span>/g;

/**
 * `code` highlighted as `language` (a grammar highlight.js has), one balanced
 * HTML fragment per line, or `null` if the output holds anything but
 * highlight.js's own spans.
 *
 * highlight.js escapes the text it highlights, so its output is safe for
 * `{@html}` without DOMPurify, which on a whole file costs more than the
 * highlighting. The check keeps that true should a grammar ever emit other
 * markup.
 */
export function highlightLines(code: string, language: string): string[] | null {
  const html = hljs.highlight(code, { language, ignoreIllegals: true }).value;
  return onlyHljsMarkup(html) ? splitHighlighted(html) : null;
}

/** Whether `html` holds no markup but highlight.js's classed spans. */
export function onlyHljsMarkup(html: string): boolean {
  return !html.replace(HLJS_TAG, "").includes("<");
}

/**
 * Split highlight.js HTML on newlines, closing and reopening open spans.
 *
 * highlight.js spans cross newlines (a block comment, a template literal), so
 * a plain split on `\n` leaves unbalanced tags (see `CodeView.svelte`). Here
 * every span still open at a newline is closed there and reopened on the next
 * line, so each fragment stands alone and keeps the colour it is inside.
 */
export function splitHighlighted(html: string): string[] {
  const lines: string[] = [];
  const open: string[] = [];
  let line = "";
  let last = 0;
  for (const match of html.matchAll(/<span[^>]*>|<\/span>|\n/g)) {
    line += html.slice(last, match.index);
    last = match.index + match[0].length;
    const token = match[0];
    if (token === "\n") {
      lines.push(line + "</span>".repeat(open.length));
      line = open.join("");
    } else {
      if (token === "</span>") open.pop();
      else open.push(token);
      line += token;
    }
  }
  lines.push(line + html.slice(last));
  return lines;
}
