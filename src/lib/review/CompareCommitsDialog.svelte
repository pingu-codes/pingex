<script lang="ts">
  import { GitCompare } from "@lucide/svelte";
  import type { DialogClose } from "$lib/app/dialogs.svelte";
  import DialogShell from "$lib/components/DialogShell.svelte";
  import DiffBlock from "$lib/components/DiffBlock.svelte";
  /** What a git branch changed between two commits: a thread's start and
   *  its branch's tip. Read locally by SHA; nothing is checked out. */
  import { fileChange } from "$lib/review/review";
  import { query } from "$lib/services/queryCache.svelte";
  import type { PrFile } from "$lib/types";
  import { shortSha } from "$lib/utils/format";

  let {
    repoDir,
    base,
    head,
    title = "Compare start to tip",
    close,
  }: { repoDir: string; base: string; head: string; title?: string; close: DialogClose<true> } = $props();

  const diffQuery = query("reviewDiff", () => [repoDir, base, head]);
  const error = $derived(diffQuery.error);
  const files = $derived<PrFile[] | null>(diffQuery.value?.files ?? null);
</script>

<DialogShell {title} subtitle="{shortSha(base)} → {shortSha(head)}" width={760} onClose={() => close()}>
  {#snippet icon()}
    <GitCompare size={17} class="text-primary-500" />
  {/snippet}
  <div class="mt-4 max-h-[60vh] space-y-2 overflow-y-auto">
    {#if error}
      <p class="error-note">{error}</p>
    {:else if files === null}
      <p class="text-xs text-surface-500">Reading the diff…</p>
    {:else if files.length === 0}
      <p class="empty-state">No changes between these commits.</p>
    {:else}
      {#each files as file (file.path)}
        <DiffBlock change={fileChange(file)} autoCollapse={true} />
      {/each}
    {/if}
  </div>
</DialogShell>
