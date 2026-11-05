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
  const threadsAt = (side: DiffSide, line: number) =>
    threads.filter((candidate) => (candidate.side ?? "RIGHT") === side && candidate.line === line);
  const outside = $derived(
    threads.filter((candidate) => !lineKeys.has(`${candidate.side ?? "RIGHT"}:${candidate.line}`)),
  );
  const pendingAt = (side: DiffSide, line: number) =>
    pending.filter((entry) => {
      const at = entryLocation(entry);
      return at.side === side && at.line === line;
    });
  const mine = $derived(selection?.path === file.path ? selection : null);
  const composing = $derived(composer?.range.path === file.path ? composer : null);
  const toggleLabel = $derived(`Toggle ${file.path}`);

  // A drag down the line numbers extends the selection row by row.
  let dragging = $state(false);
  function startDrag(event: PointerEvent, side: DiffSide, line: number) {
    if (event.button !== 0 || !canComment) return;
    event.preventDefault();
    dragging = true;
    onSelectLine(side, line, event.shiftKey);
    window.addEventListener("pointerup", () => (dragging = false), { once: true });
  }
  function dragOver(side: DiffSide, line: number) {
    if (dragging) onSelectLine(side, line, true);
  }
  function keySelect(event: MouseEvent, side: DiffSide, line: number) {
    // Pointer clicks already selected on pointerdown; this is Enter or Space.
    if (event.detail === 0) onSelectLine(side, line, event.shiftKey);
  }

  const badgeCount = (count: number, one: string, many: string) => `${count} ${count === 1 ? one : many}`;
</script>

<section class="panel-card overflow-hidden bg-surface-50-950" aria-label={file.path} data-file={file.path}>
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
        <span class="font-mono text-[11px] font-semibold">{file.path}</span>
        <span class="text-success-500">+{file.additions}</span><span class="text-error-500">−{file.deletions}</span>
        {#if counts.findings > 0}
          <span class="status-badge preset-tonal-primary">{badgeCount(counts.findings, "finding", "findings")}</span>
        {/if}
        {#if counts.comments > 0}
          <span class="status-badge preset-tonal">{badgeCount(counts.comments, "comment", "comments")}</span>
        {/if}
        <span class="grow"></span>
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
      </div>
    {/snippet}
  </ActionMenu>

  {#if open}
    <div class="overflow-x-auto border-t border-surface-200-800">
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
          {@const label = row.side === "LEFT" ? `removed line ${row.number}` : `line ${row.number}`}
          <div
            class="group/line ln relative {selected
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
                  aria-label="Select {label}"
                  aria-pressed={selected}
                  disabled={!canComment}
                  onpointerdown={(event) => startDrag(event, row.side, row.number)}
                  onpointerenter={() => dragOver(row.side, row.number)}
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
            {#if canComment}
              <button
                type="button"
                class="absolute top-0.5 left-1 grid size-4 place-items-center rounded bg-primary-500 text-white opacity-0 group-hover/line:opacity-100 focus:opacity-100"
                aria-label="Comment on {label}"
                onclick={() => {
                  onSelectLine(row.side, row.number, false);
                  onCompose("comment");
                }}
              >
                <Plus size={11} aria-hidden="true" />
              </button>
            {/if}
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

          {#each threadsAt(row.side, row.number) as found (found.key)}
            {@render thread(found)}
          {/each}
          {#each pendingAt(row.side, row.number) as entry (entry.id)}
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
