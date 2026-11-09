<script lang="ts">
  // Where a review thread sits and the lines it is about, above the thread
  // away from the diff. The location opens the file in the diff.
  import type { CommentThread } from "$lib/review/review";
  import { contextRows, threadLocation } from "$lib/review/threadContext";

  let { thread, onJump }: { thread: CommentThread; onJump: (path: string) => void } = $props();

  const rows = $derived(contextRows(thread));
</script>

<div class="flex flex-col overflow-hidden rounded-container border border-surface-200-800">
  <div class="flex items-center gap-2 bg-surface-100-900 px-2 py-1">
    <button
      type="button"
      class="min-w-0 truncate font-mono text-[11px] text-surface-500 hover:underline"
      onclick={() => onJump(thread.path)}
    >
      {threadLocation(thread)}
    </button>
    {#if thread.outdated}
      <span class="status-badge preset-tonal-warning">Outdated</span>
    {/if}
  </div>
  {#if rows}
    <div class="overflow-x-auto" role="figure" aria-label="Lines {threadLocation(thread)}">
      {#each rows as row (row.key)}
        <div
          class="ln {row.line.kind === "add" ? "ln-add" : row.line.kind === "del" ? "ln-del" : ""} {row.commented
            ? "shadow-[inset_2px_0_0_var(--color-primary-500)]"
            : ""}"
          data-commented={row.commented}
        >
          <span>{row.line.oldLine ?? ""}</span>
          <span>{row.line.newLine ?? ""}</span>
          <span>{row.line.kind === "add" ? "+" : row.line.kind === "del" ? "−" : ""}</span>
          <span
            >{#if row.html !== null}
              {@html row.html}
            {:else}
              {row.line.content}
            {/if}</span
          >
        </div>
      {/each}
    </div>
  {/if}
</div>
