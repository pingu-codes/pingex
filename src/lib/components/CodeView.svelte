<script lang="ts">
/**
 * A read-only file view: one highlighted `<pre>`, a line-number gutter beside
 * it, and a band behind the cited line.
 *
 * The gutter is a *separate* column, and that is deliberate. `hljs.highlight`
 * emits spans that legitimately cross newlines — a block comment, a template
 * literal, a Rust raw string — so splitting its HTML on newlines to wrap each
 * line leaves unbalanced tags that the parser silently re-nests, wrecking the
 * colouring from that point down. Highlighting line by line is no better (an
 * opened comment no longer carries to the next line), and highlight.js v11
 * dropped the `continuation` argument that would have made it work. So: never
 * split the highlighted HTML. Numbers live in their own column, where they have
 * no spans to break, and alignment comes from one fixed line-height shared by
 * both columns.
 *
 * The gutter is one text node rather than an element per line: a 512 KB file of
 * short lines is 20k+ lines, and that many elements is slow to build and slow
 * to scroll for something that is only ever read.
 */
import { highlightAs } from "$lib/utils/markdown";

let {
  text,
  language,
  highlightLine = null,
}: {
  text: string;
  /** A grammar highlight.js has registered; see `languageForPath`. */
  language: string;
  /** 1-based line to scroll to and tint, if any. */
  highlightLine?: number | null;
} = $props();

/**
 * Above this, highlighting costs more than the colour is worth — the same call
 * ChangesPanel makes for large diffs.
 */
const MAX_HIGHLIGHT_BYTES = 256 * 1024;

const lineCount = $derived.by(() => {
  let count = 1;
  for (let i = 0; i < text.length; i++) if (text[i] === "\n") count++;
  // A trailing newline ends the last line; it does not start another one.
  return text.endsWith("\n") && count > 1 ? count - 1 : count;
});

const gutter = $derived(Array.from({ length: lineCount }, (_unused, index) => index + 1).join("\n"));

const rendered = $derived(text.length > MAX_HIGHLIGHT_BYTES ? escapeHtml(text) : highlightAs(text, language));

/** Only draw the band for a line the file actually has. */
const band = $derived(highlightLine && highlightLine <= lineCount ? highlightLine : null);
/** Both columns carry `py-2.5`, so one offset places the band in either. */
const bandTop = $derived(`calc(0.625rem + ${(band ?? 1) - 1} * var(--code-line))`);

let bandEl = $state<HTMLElement | null>(null);
let scrolled = false;

// Scroll the band itself rather than computing an offset, so it can never end
// up somewhere the tint is not.
$effect(() => {
  if (scrolled || !bandEl) return;
  scrolled = true;
  bandEl.scrollIntoView({ block: "center" });
});

function escapeHtml(value: string): string {
  return value.replace(/[&<>]/g, (char) => (char === "&" ? "&amp;" : char === "<" ? "&lt;" : "&gt;"));
}
</script>

<div class="code-view max-h-[70vh] overflow-auto rounded-xl border border-surface-200-800">
  <!--
    The band sits inside this `min-w-max` row, not the scroll container: an
    `inset-x-0` band on the container would be viewport-wide and would clip as
    soon as the code scrolls sideways.
  -->
  <div class="relative flex min-w-max font-mono text-[11px]">
    {#if band}
      <div
        bind:this={bandEl}
        data-testid="line-band"
        class="pointer-events-none absolute inset-x-0 bg-primary-500/15"
        style="top: {bandTop}; height: var(--code-line)"
      ></div>
    {/if}
    <!-- Sticky so the numbers stay put when the code scrolls sideways; its own
         opaque background would hide the band, so it carries a second one. -->
    <div
      aria-hidden="true"
      class="sticky left-0 z-10 shrink-0 select-none border-r border-surface-200-800 bg-surface-100-900"
    >
      {#if band}
        <div
          data-testid="gutter-band"
          class="pointer-events-none absolute inset-x-0 bg-primary-500/15"
          style="top: {bandTop}; height: var(--code-line)"
        ></div>
      {/if}
      <pre
        data-testid="gutter"
        class="relative whitespace-pre px-2 py-2.5 text-right tabular-nums leading-[var(--code-line)] text-surface-500">{gutter}</pre>
    </div>
    <!-- `whitespace-pre` is load-bearing: one source line must be exactly one
         line box, or the gutter and the band drift out of alignment. -->
    <pre class="whitespace-pre px-3 py-2.5 leading-[var(--code-line)]"><code class="hljs">{@html rendered}</code></pre>
  </div>
</div>

<style>
  .code-view {
    --code-line: 18px;
    background: #0d1117;
    color: #e6edf3;
  }
  .code-view :global(.hljs) {
    background: transparent;
    padding: 0;
  }
</style>
