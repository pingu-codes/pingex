<!--
  The review page's file rail: the changed files by folder, each with its counts
  and viewed mark, the current one highlighted and kept in view.
-->
<script lang="ts">
  import { Check } from "@lucide/svelte";
  import { type FileCounts, fileGroups } from "$lib/review/reviewPage";
  import type { PrFile } from "$lib/types";
  import { basename } from "$lib/utils/path";

  /** Room kept around the highlighted rail row: its folder's label fits. */
  const RAIL_MARGIN = 32;

  let {
    files,
    currentPath,
    countsByPath,
    viewed,
    onJump,
  }: {
    files: PrFile[];
    currentPath: string | null;
    countsByPath: ReadonlyMap<string, FileCounts>;
    viewed: ReadonlySet<string>;
    onJump: (path: string) => void;
  } = $props();

  let nav = $state<HTMLElement>();

  // Keep the highlighted row, and the folder above it, in the rail's view. By
  // hand: `scrollIntoView` would also move the page.
  $effect(() => {
    void currentPath;
    const rail = nav;
    if (!rail) return;
    const frame = requestAnimationFrame(() => {
      const row = rail.querySelector('[aria-current="true"]')?.getBoundingClientRect();
      if (!row) return;
      const box = rail.getBoundingClientRect();
      if (row.top - RAIL_MARGIN < box.top) rail.scrollTop -= box.top - row.top + RAIL_MARGIN;
      else if (row.bottom + RAIL_MARGIN > box.bottom) rail.scrollTop += row.bottom - box.bottom + RAIL_MARGIN;
    });
    return () => cancelAnimationFrame(frame);
  });
</script>

<div class="hidden w-56 shrink-0 @5xl/review:block border-r border-surface-200-800 bg-surface-100-900">
  <nav
    bind:this={nav}
    aria-label="Changed files"
    class="sticky top-0 flex max-h-[100cqh] flex-col gap-0.5 overflow-y-auto px-2 py-3"
  >
    {#each fileGroups(files) as group (group.dir)}
      {#if group.dir}
        <!-- Wraps at a slash when it can: WebKit won't break there on its own. -->
        <div class="eyebrow px-2 pt-2 pb-1.5 wrap-anywhere">{group.dir.replaceAll("/", "/\u200b")}</div>
      {/if}
      {#each group.files as file (file.path)}
        {@const counts = countsByPath.get(file.path)}
        <button
          type="button"
          class="menu-row {currentPath === file.path ? "bg-surface-200-800" : ""}"
          aria-current={currentPath === file.path}
          onclick={() => onJump(file.path)}
        >
          <span class="min-w-0 grow truncate font-mono text-[11px]">{basename(file.path)}</span>
          {#if counts?.findings}
            <span class="status-badge preset-tonal-primary" title="Agent findings">{counts.findings}</span>
          {/if}
          {#if counts?.comments}
            <span class="status-badge preset-tonal" title="Comments">{counts.comments}</span>
          {/if}
          {#if viewed.has(file.path)}
            <Check size={12} class="text-success-500" aria-label="Viewed" />
          {/if}
        </button>
      {/each}
    {/each}
    <div class="mt-2 flex flex-col gap-1.5 border-t border-surface-200-800 p-2 text-surface-500">
      <div class="flex items-center gap-1.5"><kbd class="kbd">]</kbd><kbd class="kbd">[</kbd>Next / previous file</div>
      <div class="flex items-center gap-1.5"><kbd class="kbd">v</kbd>Mark viewed and collapse</div>
    </div>
  </nav>
</div>
