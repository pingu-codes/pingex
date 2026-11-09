<script lang="ts">
  /**
   * "Move to PR": file a thread under another open pull request of its
   * repository, or under none. Only the grouping changes; the thread keeps
   * the step, checkout and commit it started on.
   */
  import { GitPullRequest, X } from "@lucide/svelte";
  import type { DialogClose } from "$lib/app/dialogs.svelte";
  import DialogShell from "$lib/components/DialogShell.svelte";
  import { query } from "$lib/services/queryCache.svelte";
  import type { PrLink, PrSummary } from "$lib/types";

  export type MoveToPrChoice = { number: number | null };

  let { link, close }: { link: PrLink; close: DialogClose<MoveToPrChoice> } = $props();

  const prsQuery = query("reviewListPrs", () => [link.repoKey]);
  const error = $derived(prsQuery.error);
  const prs = $derived<PrSummary[] | null>(prsQuery.loading ? null : (prsQuery.value ?? []));
</script>

<DialogShell
  title="Move to PR"
  subtitle="It keeps the commit and checkout it started on."
  width={440}
  onClose={() => close()}
>
  {#snippet icon()}
    <GitPullRequest size={17} class="text-primary-500" />
  {/snippet}

  <div class="mt-4 max-h-80 space-y-1 overflow-y-auto">
    {#if error}
      <p class="error-note">{error}</p>
    {:else if prs === null}
      <p class="px-2 py-1 text-xs text-surface-500">Loading pull requests…</p>
    {:else}
      {#each prs as pr (pr.number)}
        <button
          type="button"
          class="menu-row {pr.number === link.prNumber ? "preset-tonal" : ""}"
          aria-current={pr.number === link.prNumber ? "true" : undefined}
          onclick={() => close({ number: pr.number })}
        >
          <span class="font-mono text-[11px] text-surface-500">#{pr.number}</span>
          <span class="min-w-0 flex-1 truncate">{pr.title}</span>
          <span class="shrink-0 text-[10px] text-surface-500">{pr.author}</span>
        </button>
      {/each}
      <button type="button" class="menu-row text-surface-600-400" onclick={() => close({ number: null })}>
        <X size={13} class="shrink-0" />
        No PR
      </button>
    {/if}
  </div>
</DialogShell>
