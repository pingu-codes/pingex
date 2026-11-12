<!--
  Whether a PR has a local worktree, and which: an icon for a list row, a chip
  with the folder's name for a page, both with the name, path, branch and state
  on hover. Until the read has answered it draws nothing, so it never says "no
  worktree" for a PR it hasn't looked up.
-->
<script lang="ts">
  import { Folder, FolderGit2 } from "@lucide/svelte";
  import TooltipAnchor from "$lib/components/TooltipAnchor.svelte";
  import { worktreeBadge } from "$lib/review/worktreeBadge";
  import type { PrCheckout } from "$lib/types";

  let {
    checkout,
    known,
    variant,
    noneHint = "Check out makes its worktree.",
  }: {
    checkout: PrCheckout | null;
    /** The read has answered: `null` then means there is no worktree. */
    known: boolean;
    variant: "icon" | "chip";
    /** What creates a worktree here, for the tooltip when there is none. */
    noneHint?: string;
  } = $props();

  const facts = $derived(worktreeBadge(checkout, noneHint));
  const iconTone = $derived(
    { here: "text-primary-500", elsewhere: "text-surface-500", none: "text-surface-400-600" }[facts.state],
  );
  const chipTone = $derived(
    {
      here: "preset-tonal-primary",
      elsewhere: "preset-tonal",
      none: "border border-dashed border-surface-300-700 text-surface-500",
    }[facts.state],
  );
</script>

{#snippet glyph(size: number)}
  {#if facts.state === "none"}
    <Folder {size} aria-hidden="true" />
  {:else}
    <FolderGit2 {size} aria-hidden="true" />
  {/if}
{/snippet}

{#if known}
  <TooltipAnchor
    label={facts.label}
    aria-label={facts.label}
    role="img"
    data-worktree-state={facts.state}
    placement="bottom-start"
    contentClass="panel-card flex max-w-[26rem] flex-col gap-0.5 bg-surface-50-950 px-2.5 py-2 text-xs shadow-lg"
    class={variant === "icon" ? `relative inline-flex shrink-0 ${iconTone}` : `status-badge ${chipTone}`}
  >
    {#if variant === "icon"}
      {@render glyph(13)}
      {#if facts.holdsWork}
        <span class="absolute -top-0.5 -right-0.5 size-1.5 rounded-full bg-warning-500" aria-hidden="true"></span>
      {/if}
    {:else}
      {@render glyph(10)}
      <span class="max-w-40 truncate">{facts.name}</span>
      {#if facts.holdsWork}
        <span class="size-1.5 shrink-0 rounded-full bg-warning-500" aria-hidden="true"></span>
      {/if}
    {/if}
    {#snippet content()}
      <span class="font-semibold">{facts.name}</span>
      {#if facts.path}
        <span class="font-mono text-[11px] break-all text-surface-600-400">{facts.path}</span>
      {/if}
      {#each facts.lines as line (line)}
        <span class="text-surface-600-400">{line}</span>
      {/each}
    {/snippet}
  </TooltipAnchor>
{/if}
