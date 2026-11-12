<script lang="ts">
  import type { SideExchange } from "$lib/types";
  import { renderMarkdown } from "$lib/utils/markdown";

  // A side question answered as text: each question with its answer, refusal
  // or stop.
  let { exchanges }: { exchanges: SideExchange[] } = $props();
</script>

<div class="space-y-3 text-xs" aria-live="polite">
  {#each exchanges as entry (entry.id)}
    <div class="ml-auto max-w-[90%] rounded-xl bg-primary-500/10 px-3 py-2 whitespace-pre-wrap">{entry.question}</div>
    {#if entry.refusal}
      <p class="text-warning-500">{entry.refusal.content}</p>
    {/if}
    {#if entry.outcome === "interrupted"}
      <p class="text-surface-500">Side question stopped.</p>
    {:else if entry.response}
      <div class:text-error-500={entry.outcome === "error"} class="markdown-body text-xs">
        {@html renderMarkdown(entry.response)}
      </div>
    {/if}
  {/each}
</div>
