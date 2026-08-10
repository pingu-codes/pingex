import DOMPurify from "dompurify";
import hljs from "highlight.js/lib/common";
import { Marked, Renderer, type Tokens } from "marked";
import "highlight.js/styles/github-dark.css";

const COPY_BUTTON = `<button type="button" class="code-copy" aria-label="Copy code" title="Copy code"><svg class="icon-copy" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="14" height="14" x="8" y="8" rx="2" ry="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/></svg><svg class="icon-check" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M20 6 9 17l-5-5"/></svg></button>`;

const marked = new Marked({
  gfm: true,
  renderer: {
    code({ text, lang }) {
      const language = lang && hljs.getLanguage(lang) ? lang : null;
      const highlighted = language ? hljs.highlight(text, { language }).value : hljs.highlightAuto(text).value;
      const label = language ? `<div class="code-lang">${language}</div>` : "";
      return `<div class="code-block">${label}${COPY_BUTTON}<pre><code class="hljs">${highlighted}</code></pre></div>`;
    },
    table(token: Tokens.Table) {
      return `<div class="table-wrap">${Renderer.prototype.table.call(this, token)}</div>`;
    },
  },
});

export function renderMarkdown(text: string): string {
  return DOMPurify.sanitize(marked.parse(text, { async: false }));
}

export function highlightAs(code: string, language: string): string {
  const value = hljs.getLanguage(language) ? hljs.highlight(code, { language }).value : hljs.highlightAuto(code).value;
  return DOMPurify.sanitize(value);
}
