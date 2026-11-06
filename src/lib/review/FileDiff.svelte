<!--
  One changed file on the review page: a header that toggles it, marks it
  viewed and opens the file menu, then its diff lines. Hover a line for +;
  click or drag down the line numbers (⇧-click extends) to select a range,
  which floats Comment / Suggest change / Ask agent. Review threads, pending
  entries and the composer sit under the line they anchor to, and
  `afterLine` lets a caller add its own cards there (agent Findings).
-->
<script lang="ts" module>
  /** Where a caller's per-line content goes. */
  export type LineSlot = { path: string; side: DiffSide; line: number };
  export type Composer = { range: LineRange; mode: "comment" | "suggestion" };
</script>

<script lang="ts">
  import { ChevronDown, ChevronRight, Ellipsis, Plus, Sparkles, X } from "@lucide/svelte";
  import type { Snippet } from "svelte";
  import ActionMenu from "$lib/components/ActionMenu.svelte";
  import type { MenuEntry } from "$lib/components/actionMenu";
  import LineComposer from "$lib/review/LineComposer.svelte";
  import type { CommentThread } from "$lib/review/review";
  import {
    type DiffSide,
    diffRows,
    entryLabel,
    entryLocation,
    type FileCounts,
    groupBy,
    inRange,
    type LineRange,
    rangeLabel,
    rangeText,
  } from "$lib/review/reviewPage";
  import type { PendingEntry, PrFile } from "$lib/types";

  let {
    file,
    open,
    viewed,
    counts,
    threads,
    pending,
    selection,
    composer,
    canComment = true,
    menuEntries,
    onMenu,
    onToggle,
    onViewed,
    onSelectLine,
    onCompose,
    onAskAgent,
    onAddEntry,
    onCancelComposer,
    onRemoveEntry,
    thread,
    afterLine,
  }: {
    file: PrFile;
    open: boolean;
    viewed: boolean;
    counts: FileCounts;
    /** This file's threads the current filter shows. */
    threads: CommentThread[];
    /** Pending entries in this file. */
    pending: PendingEntry[];
    selection: LineRange | null;
    composer: Composer | null;
    canComment?: boolean;
    menuEntries: MenuEntry[];
    onMenu: (id: string) => void;
    /** The header toggle; ⌥-click collapses every other file. */
    onToggle: (event: MouseEvent) => void;
    onViewed: (viewed: boolean) => void;
    onSelectLine: (side: DiffSide, line: number, extend: boolean) => void;
    onCompose: (mode: Composer["mode"]) => void;
    onAskAgent: () => void;
    onAddEntry: (body: string, replacement: string | null) => void;
    onCancelComposer: () => void;
    onRemoveEntry: (id: string) => void;
    thread: Snippet<[CommentThread]>;
    afterLine?: Snippet<[LineSlot]>;
  } = $props();

  const rows = $derived(diffRows(file));
  const lineKeys = $derived(new Set(rows.flatMap((row) => (row.kind === "line" ? [`${row.side}:${row.number}`] : []))));
  const threadKey = (candidate: CommentThread) => `${candidate.side ?? "RIGHT"}:${candidate.line}`;
  const threadsByLine = $derived(groupBy(threads, threadKey));
  const pendingByLine = $derived(
    groupBy(pending, (entry) => {
      const at = entryLocation(entry);
      return `${at.side}:${at.line}`;
    }),
  );
  const outside = $derived(threads.filter((candidate) => !lineKeys.has(threadKey(candidate))));
  const mine = $derived(selection?.path === file.path ? selection : null);
  const composing = $derived(composer?.range.path === file.path ? composer : null);
  const toggleLabel = $derived(`Toggle ${file.path}`);

  // A drag down the line numbers extends the selection row by row.
  let dragging = false;
  function startDrag(event: PointerEvent, side: DiffSide, line: number) {
    if (event.button !== 0 || !canComment) return;
    event.preventDefault();
    dragging = true;
    onSelectLine(side, line, event.shiftKey);
    window.addEventListener("pointerup", () => (dragging = false), { once: true });
  }

  // One listener for the whole diff, not one per line: it tracks the row
  // under the pointer for the hover + and extends a drag.
  type Hovered = { side: DiffSide; line: number; top: number };
  let hovered = $state<Hovered | null>(null);
  function pointerOver(event: PointerEvent) {
    const row = (event.target as Element).closest<HTMLElement>(".ln[data-line]");
    if (!row) return;
    const side = row.dataset.side as DiffSide;
    const line = Number(row.dataset.line);
    if (hovered?.side === side && hovered.line === line) return;
    hovered = { side, line, top: row.offsetTop };
    if (dragging) onSelectLine(side, line, true);
  }
  const lineLabel = (side: DiffSide, line: number) => (side === "LEFT" ? `removed line ${line}` : `line ${line}`);
  function keySelect(event: MouseEvent, side: DiffSide, line: number) {
    // Pointer clicks already selected on pointerdown; this is Enter or Space.
    if (event.detail === 0) onSelectLine(side, line, event.shiftKey);
  }

  const badgeCount = (count: number, one: string, many: string) => `${count} ${count === 1 ? one : many}`;
</script>

<section class="panel-card shrink-0 overflow-hidden bg-surface-50-950" aria-label={file.path} data-file={file.path}>
  <ActionMenu entries={menuEntries} label="File {file.path}" onSelect={onMenu} buttonLabel="File actions">
    {#snippet row({ rowProps, buttonProps })}
      <div {...rowProps} class="file-head outline-none focus-visible:ring-1 focus-visible:ring-primary-500">
        <button
          type="button"
          class="btn-icon btn-icon-sm hover:preset-tonal"
          aria-label={toggleLabel}
          aria-expanded={open}
          onclick={onToggle}
        >
          {#if open}
            <ChevronDown size={14} aria-hidden="true" />
          {:else}
            <ChevronRight size={14} aria-hidden="true" />
          {/if}
        </button>
        <span class="min-w-0 truncate font-mono text-[11px] font-semibold" title={file.path}>{file.path}</span>
        <span class="text-success-500">+{file.additions}</span><span class="text-error-500">−{file.deletions}</span>
        {#if counts.findings > 0}
          <span class="status-badge preset-tonal-primary">{badgeCount(counts.findings, "finding", "findings")}</span>
        {/if}
        {#if counts.comments > 0}
          <span class="status-badge preset-tonal">{badgeCount(counts.comments, "comment", "comments")}</span>
        {/if}
        <!-- Viewed and the menu wrap together, never apart. -->
        <span class="ml-auto flex items-center gap-2">
          <label class="inline-flex items-center gap-1.5 text-surface-500">
            <input
              type="checkbox"
              class="checkbox checkbox-sm"
              checked={viewed}
              aria-label="Viewed {file.path}"
              onchange={(event) => onViewed(event.currentTarget.checked)}
            >
            Viewed
          </label>
          <button {...buttonProps} type="button" class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500">
            <Ellipsis size={14} aria-hidden="true" />
          </button>
        </span>
      </div>
    {/snippet}
  </ActionMenu>

  {#if open}
    <!-- Off screen, the diff skips layout and paint and holds its estimated
         height (`.ln` rows are 20px), so long PRs scroll smoothly. -->
    <div
      class="relative overflow-x-auto border-t border-surface-200-800 [content-visibility:auto]"
      style:contain-intrinsic-block-size="auto {rows.length * 20}px"
      role="presentation"
      onpointerover={pointerOver}
      onpointerleave={() => (hovered = null)}
    >
      {#if canComment && hovered}
        {@const at = hovered}
        <button
          type="button"
          tabindex="-1"
          class="absolute left-1 z-[1] grid size-4 place-items-center rounded bg-primary-500 text-white"
          style:top="{at.top + 2}px"
          aria-label="Comment on {lineLabel(at.side, at.line)}"
          onclick={() => {
            onSelectLine(at.side, at.line, false);
            onCompose("comment");
          }}
        >
          <Plus size={11} aria-hidden="true" />
        </button>
      {/if}
      {#if file.patchTruncated}
        <div class="p-4 text-center text-xs text-surface-500">No text diff for this file (binary, or too large).</div>
      {/if}
      {#each rows as row (row.key)}
        {#if row.kind === "hunk"}
          <div class="ln bg-surface-100-900 text-surface-500">
            <span></span><span></span><span></span><span>{row.header}</span>
          </div>
        {:else}
          {@const selected = inRange(mine, file.path, row.side, row.number)}
          <div
            data-side={row.side}
            data-line={row.number}
            class="ln {selected
              ? "ln-sel"
              : row.line.kind === "add"
                ? "ln-add"
                : row.line.kind === "del"
                  ? "ln-del"
                  : ""}"
          >
            {#each [row.line.oldLine, row.line.newLine] as number, column (column)}
              {#if (column === 0) === (row.side === "LEFT")}
                <button
                  type="button"
                  class="text-right {selected ? "font-semibold text-surface-900-100" : ""}"
                  aria-label="Select {lineLabel(row.side, row.number)}"
                  aria-pressed={selected}
                  disabled={!canComment}
                  onpointerdown={(event) => startDrag(event, row.side, row.number)}
                  onclick={(event) => keySelect(event, row.side, row.number)}
                >
                  {number}
                </button>
              {:else}
                <span>{number ?? ""}</span>
              {/if}
            {/each}
            <span>{row.line.kind === "add" ? "+" : row.line.kind === "del" ? "−" : ""}</span>
            <span>{row.line.content}</span>
          </div>

          {#if mine && !composing && mine.side === row.side && mine.end === row.number}
            <div class="relative h-0">
              <div
                role="toolbar"
                aria-label={rangeLabel(mine)}
                class="popover-card absolute -top-1 right-3 z-10 flex gap-0.5 p-1"
              >
                <button
                  type="button"
                  class="btn btn-sm hover:preset-tonal text-[11px]"
                  onclick={() => onCompose("comment")}
                >
                  Comment <kbd class="kbd">C</kbd>
                </button>
                <button
                  type="button"
                  class="btn btn-sm hover:preset-tonal text-[11px]"
                  onclick={() => onCompose("suggestion")}
                >
                  Suggest change
                </button>
                <button
                  type="button"
                  class="btn btn-sm hover:preset-tonal text-[11px] text-primary-500"
                  onclick={onAskAgent}
                >
                  <Sparkles size={12} aria-hidden="true" />Ask agent
                </button>
              </div>
            </div>
          {/if}

          {#each threadsByLine.get(`${row.side}:${row.number}`) ?? [] as found (found.key)}
            {@render thread(found)}
          {/each}
          {#each pendingByLine.get(`${row.side}:${row.number}`) ?? [] as entry (entry.id)}
            {@render pendingCard(entry)}
          {/each}
          {#if composing && composing.range.side === row.side && composing.range.end === row.number}
            {#key `${composing.range.start}-${composing.range.end}-${composing.mode}`}
              <LineComposer
                label={rangeLabel(composing.range)}
                mode={composing.mode}
                initialReplacement={composing.mode === "suggestion" ? rangeText(file, composing.range) : ""}
                onAdd={onAddEntry}
                onCancel={onCancelComposer}
              />
            {/key}
          {/if}
          {@render afterLine?.({ path: file.path, side: row.side, line: row.number })}
        {/if}
      {/each}
      {#if outside.length > 0}
        <div class="eyebrow px-3 pt-2">Outside the diff</div>
        {#each outside as found (found.key)}
          {@render thread(found)}
        {/each}
      {/if}
    </div>
  {/if}
</section>

{#snippet pendingCard(entry: PendingEntry)}
  <div
    class="my-2 mr-3 ml-[104px] flex items-start gap-2 rounded-container border border-dashed border-surface-300-700 p-2.5 text-xs"
  >
    <span class="status-badge preset-tonal">Pending</span>
    <div class="min-w-0 grow">
      <div class="font-mono text-[11px] text-surface-500">{entryLabel(entry)}</div>
      <div class="text-[13px] leading-5 whitespace-pre-wrap">{entry.body}</div>
      {#if entry.kind === "suggestion"}
        <pre
          class="mt-1 rounded bg-surface-100-900 p-1.5 font-mono text-[11px] whitespace-pre-wrap"
        >{entry.replacement}</pre>
      {/if}
    </div>
    <button
      type="button"
      class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500"
      aria-label="Remove pending {entry.kind === "reply" ? "reply" : "comment"}"
      onclick={() => onRemoveEntry(entry.id)}
    >
      <X size={13} aria-hidden="true" />
    </button>
  </div>
{/snippet}
