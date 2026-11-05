<script lang="ts">
  /** What a git branch changed between two commits: a thread's start and
   *  its branch's tip. Read locally by SHA; nothing is checked out. */
  import { GitCompare } from "@lucide/svelte";
  import type { DialogClose } from "$lib/app/dialogs.svelte";
  import DialogShell from "$lib/components/DialogShell.svelte";
  import DiffBlock from "$lib/components/DiffBlock.svelte";
  import { commands } from "$lib/services/homeRouting";
  import type { PrFile } from "$lib/types";
  import { errorMessage } from "$lib/utils/errors";

  let {
    repoDir,
    base,
    head,
    title = "Compare start to tip",
    close,
  }: { repoDir: string; base: string; head: string; title?: string; close: DialogClose<true> } = $props();

  let files = $state<PrFile[] | null>(null);
  let error = $state<string | null>(null);

  $effect(() => {
    commands
      .reviewDiff(repoDir, base, head)
      .then((diff) => (files = diff.files))
      .catch((cause) => (error = errorMessage(cause)));
  });
</script>

<DialogShell {title} subtitle="{base.slice(0, 7)} → {head.slice(0, 7)}" width={760} onClose={() => close()}>
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
        <DiffBlock change={{ path: file.path, kind: { type: file.status }, diff: file.patch }} autoCollapse={true} />
      {/each}
    {/if}
  </div>
</DialogShell>
