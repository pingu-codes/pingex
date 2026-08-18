<script lang="ts">
import { ArrowLeft, FileDiff, RefreshCw } from "@lucide/svelte";
import { untrack } from "svelte";
import { gitFileDiff } from "$lib/services/api";
import type { ChangedFile, ChangesSummary, FileDiff as FileDiffData } from "$lib/types";
import { fileIconFor } from "$lib/utils/fileIcons";
import { highlightAs } from "$lib/utils/markdown";

let {
  cwd,
  summary,
  loading = false,
  error = null,
  focusPath = null,
  onRefresh,
}: {
  cwd: string;
  summary: ChangesSummary | null;
  loading?: boolean;
  error?: string | null;
  /** Open straight onto this file; `null` shows the list. */
  focusPath?: string | null;
  onRefresh: () => void;
} = $props();

/** Rows rendered at once; the list grows in pages so 2000 files never mount together. */
const PAGE = 150;
/** Above this many changed lines a diff starts collapsed and is never highlighted. */
const LARGE_LINES = 5000;
const HIGHLIGHT_BYTES = 100_000;
const DEFAULT_BYTES = 256 * 1024;
const MAX_BYTES = 2 * 1024 * 1024;

let selected = $state<string | null>(null);
let visibleCount = $state(PAGE);
let diff = $state<FileDiffData | null>(null);
let diffLoading = $state(false);
let diffError = $state<string | null>(null);
let forceLarge = $state(false);
let requestId = 0;
const cache = new Map<string, FileDiffData>();

const files = $derived(summary?.files ?? []);
const selectedFile = $derived(files.find((file) => file.path === selected) ?? null);
const isLarge = $derived(
  !!selectedFile && (selectedFile.binary || selectedFile.additions + selectedFile.deletions > LARGE_LINES),
);

// Follow the requested focus path whenever it changes.
$effect(() => {
  selected = focusPath ?? null;
});

// New summary (refresh after a turn) invalidates cached patches.
$effect(() => {
  summary;
  cache.clear();
  const current = untrack(() => selected);
  if (current) void load(current, DEFAULT_BYTES);
});

$effect(() => {
  forceLarge = false;
  if (selected) void load(selected, DEFAULT_BYTES);
  else diff = null;
});

async function load(path: string, maxBytes: number) {
  const file = files.find((entry) => entry.path === path);
  if (!file || !summary) return;
  const key = `${summary.base}:${path}:${maxBytes}`;
  const cached = cache.get(key);
  if (cached) {
    diff = cached;
    return;
  }
  const id = ++requestId;
  diffLoading = true;
  diffError = null;
  try {
    const result = await gitFileDiff(cwd, summary.base, path, file.status === "untracked", maxBytes);
    if (id !== requestId) return;
    if (cache.size > 50) cache.delete(cache.keys().next().value as string);
    cache.set(key, result);
    diff = result;
  } catch (e) {
    if (id !== requestId) return;
    diffError = e instanceof Error ? e.message : String(e);
  } finally {
    if (id === requestId) diffLoading = false;
  }
}

function basename(path: string): string {
  return path.split("/").pop() ?? path;
}

const statusLabel: Record<ChangedFile["status"], string> = {
  added: "New",
  untracked: "New",
  modified: "Edited",
  deleted: "Deleted",
  renamed: "Renamed",
};

// Only highlight modest diffs: highlight.js on a megabyte of generated text
// is exactly the lag this panel exists to avoid.
const rendered = $derived.by(() => {
  if (!diff) return "";
  if (diff.patch.length > HIGHLIGHT_BYTES) return escapeHtml(diff.patch);
  return highlightAs(diff.patch, "diff");
});

function escapeHtml(text: string): string {
  return text.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;");
}
</script>

{#if error}
  <p class="text-xs text-error-500">{error}</p>
{:else if !summary && loading}
  <p class="text-xs text-surface-500">Reading changes…</p>
{:else if !summary}
  <p class="text-xs text-surface-500">Not a Git repository.</p>
{:else if selectedFile}
  <div class="flex h-full min-h-0 flex-col gap-2">
    <div class="flex items-center gap-2">
      <button
        onclick={() => (selected = null)}
        class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500"
        aria-label="Back to changed files"
      >
        <ArrowLeft size={14} />
      </button>
      <span class="min-w-0 flex-1 truncate font-mono text-xs" title={selectedFile.path}>{selectedFile.path}</span>
      <span class="shrink-0 text-[10px] tabular-nums text-success-500">+{selectedFile.additions}</span>
      <span class="shrink-0 text-[10px] tabular-nums text-error-500">−{selectedFile.deletions}</span>
    </div>
    {#if selectedFile.binary}
      <p class="text-xs text-surface-500">Binary file — no text diff.</p>
    {:else if isLarge && !forceLarge}
      <div class="rounded-xl border border-surface-200-800 p-3 text-xs text-surface-500">
        <p>Large change: {(selectedFile.additions + selectedFile.deletions).toLocaleString()} lines.</p>
        <button onclick={() => (forceLarge = true)} class="btn btn-sm preset-tonal mt-2">Show first {DEFAULT_BYTES / 1024} KB</button>
      </div>
    {:else if diffLoading && !diff}
      <p class="text-xs text-surface-500">Loading diff…</p>
    {:else if diffError}
      <p class="text-xs text-error-500">{diffError}</p>
    {:else if diff}
      <div class="min-h-0 flex-1 overflow-hidden rounded-xl border border-surface-200-800">
        <pre class="diff-block h-full overflow-auto px-3 py-2.5 font-mono text-[11px] leading-5"><code class="hljs">{@html rendered}</code></pre>
      </div>
      {#if diff.truncated}
        <div class="flex items-center gap-2 text-[11px] text-surface-500">
          <span class="flex-1">Showing first {Math.round(diff.bytes / 1024)} KB.</span>
          {#if diff.bytes < MAX_BYTES}
            <button onclick={() => selected && load(selected, Math.min(diff!.bytes * 4, MAX_BYTES))} class="btn btn-sm preset-tonal">Show more</button>
          {:else}
            <span>Diff too large — open the file in your editor.</span>
          {/if}
        </div>
      {/if}
    {/if}
  </div>
{:else}
  <div class="flex items-center gap-2 pb-2 text-[11px] text-surface-500">
    <span class="flex-1 truncate">
      {summary.totalFiles} file{summary.totalFiles === 1 ? "" : "s"}
      {#if summary.baseBranch}· vs {summary.baseBranch}{/if}
    </span>
    <span class="tabular-nums text-success-500">+{summary.additions}</span>
    <span class="tabular-nums text-error-500">−{summary.deletions}</span>
    <button onclick={onRefresh} class="btn-icon btn-icon-sm hover:preset-tonal" aria-label="Refresh changes" disabled={loading}>
      <RefreshCw size={12} class={loading ? "animate-spin" : ""} />
    </button>
  </div>
  {#if files.length === 0}
    <p class="text-xs text-surface-500">No changes.</p>
  {:else}
    <div class="space-y-0.5">
      {#each files.slice(0, visibleCount) as file (file.path)}
        {@const icon = fileIconFor(basename(file.path))}
        <button
          onclick={() => (selected = file.path)}
          title={file.path}
          class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-xs hover:preset-tonal"
        >
          <icon.icon size={13} class="shrink-0 {icon.class}" />
          <span class="min-w-0 flex-1 truncate">{file.path}</span>
          {#if file.binary}
            <span class="shrink-0 text-[10px] text-surface-500">bin</span>
          {:else}
            <span class="shrink-0 text-[10px] tabular-nums text-success-500">+{file.additions}</span>
            <span class="shrink-0 text-[10px] tabular-nums text-error-500">−{file.deletions}</span>
          {/if}
          <span class="w-12 shrink-0 text-right text-[10px] uppercase tracking-wide text-surface-500">{statusLabel[file.status]}</span>
        </button>
      {/each}
    </div>
    {#if files.length > visibleCount}
      <button onclick={() => (visibleCount += PAGE)} class="mt-1 w-full rounded px-2 py-1.5 text-left text-xs text-surface-500 hover:preset-tonal">
        Show {Math.min(PAGE, files.length - visibleCount)} more of {files.length - visibleCount}
      </button>
    {/if}
    {#if summary.truncated}
      <p class="mt-2 flex items-center gap-1 text-[11px] text-warning-500">
        <FileDiff size={12} /> Listing capped at {files.length} of {summary.totalFiles} files.
      </p>
    {/if}
  {/if}
{/if}

<style>
  .diff-block {
    background: #0d1117;
    color: #e6edf3;
  }
  .diff-block :global(.hljs) {
    background: transparent;
    padding: 0;
  }
</style>
