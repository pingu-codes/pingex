<script lang="ts">
import type { ThreadItem } from "$lib/types";
import { renderMarkdown } from "$lib/utils/markdown";

let { items, live = false }: { items: ThreadItem[]; live?: boolean } = $props();

const summaries = $derived(items.map((item) => (item.summary ?? []).filter(Boolean).join("\n\n")).filter(Boolean));
const latest = $derived(summaries.at(-1) ?? "");
</script>

{#if live}
  <div class="text-xs">
    <div class="working-shimmer mb-1 font-medium">Working…</div>
    {#if latest}
      <div class="prose-reasoning leading-5 text-surface-500">
        {@html renderMarkdown(latest)}
      </div>
    {/if}
  </div>
{:else if summaries.length > 0}
  <div class="prose-reasoning text-xs leading-5 text-surface-500">
    {@html renderMarkdown(summaries.join("\n\n"))}
  </div>
{/if}

<style>
  .prose-reasoning :global(p) {
    margin: 0.25rem 0;
  }
  .working-shimmer {
    background: linear-gradient(
      90deg,
      color-mix(in oklab, currentColor 40%, transparent) 25%,
      currentColor 50%,
      color-mix(in oklab, currentColor 40%, transparent) 75%
    );
    background-size: 200% 100%;
    -webkit-background-clip: text;
    background-clip: text;
    color: var(--color-surface-500, #888);
    -webkit-text-fill-color: transparent;
    animation: shimmer 1.6s linear infinite;
    width: fit-content;
  }
  @keyframes shimmer {
    from {
      background-position: 200% 0;
    }
    to {
      background-position: -200% 0;
    }
  }
</style>
