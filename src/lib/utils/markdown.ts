import DOMPurify from "dompurify";
import hljs from "highlight.js/lib/common";
import { Marked, Renderer, type Tokens } from "marked";
import "highlight.js/styles/github-dark.css";

const COPY_BUTTON = `<button type="button" class="code-copy" aria-label="Copy code" title="Copy code"><svg class="icon-copy" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="14" height="14" x="8" y="8" rx="2" ry="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/></svg><svg class="icon-check" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M20 6 9 17l-5-5"/></svg></button>`;

/**
 * Highlighted code by language and source. A streaming message is re-rendered
 * on every delta, and highlighting (`highlightAuto` above all, which tries
 * every language) dominated that: every finished block was redone each time.
 * Finished blocks never change, so they come from here; only the block still
 * growing is highlighted again. Bounded, oldest first.
 */
const highlightCache = new Map<string, string>();
const HIGHLIGHT_CACHE_ENTRIES = 200;

function highlightBlock(text: string, language: string | null): string {
  const key = `${language ?? ""}\u0000${text}`;
  const cached = highlightCache.get(key);
  if (cached !== undefined) return cached;
  const value = language ? hljs.highlight(text, { language }).value : hljs.highlightAuto(text).value;
  if (highlightCache.size >= HIGHLIGHT_CACHE_ENTRIES) {
    highlightCache.delete(highlightCache.keys().next().value as string);
  }
  highlightCache.set(key, value);
  return value;
}

const marked = new Marked({
  gfm: true,
  renderer: {
    code({ text, lang, raw }) {
      const language = lang && hljs.getLanguage(lang) ? lang : null;
      // A fence with no closing line is a block still streaming in. Guessing
      // its language on every delta is the expensive part, so an unlabeled
      // one shows plain until it closes, then is highlighted once.
      const open = /^\s*(`{3,}|~{3,})/.test(raw) && !/\n\s*(`{3,}|~{3,})\s*$/.test(raw);
      const highlighted = !language && open ? escapeHtml(text) : highlightBlock(text, language);
      const label = language ? `<div class="code-lang">${language}</div>` : "";
      return `<div class="code-block code-dark">${label}${COPY_BUTTON}<pre><code class="hljs">${highlighted}</code></pre></div>`;
    },
    table(token: Tokens.Table) {
      return `<div class="table-wrap">${Renderer.prototype.table.call(this, token)}</div>`;
    },
  },
});

/** Escapes text for `{@html}`, for code shown without highlighting. */
export function escapeHtml(text: string): string {
  return text.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;");
}

export function renderMarkdown(text: string): string {
  return DOMPurify.sanitize(marked.parse(text, { async: false }));
}

export function highlightAs(code: string, language: string): string {
  const value = hljs.getLanguage(language) ? hljs.highlight(code, { language }).value : hljs.highlightAuto(code).value;
  return DOMPurify.sanitize(value);
}

/** Above this a diff is only escaped: highlight.js on a megabyte of generated text lags the UI. */
const DIFF_HIGHLIGHT_BYTES = 100_000;

/** A patch highlighted as `diff`, or escaped as plain text when longer than `maxBytes`. */
export function highlightDiff(patch: string, maxBytes = DIFF_HIGHLIGHT_BYTES): string {
  return patch.length > maxBytes ? escapeHtml(patch) : highlightAs(patch, "diff");
}
