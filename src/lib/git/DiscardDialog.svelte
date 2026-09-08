<script lang="ts">
import { Undo2 } from "@lucide/svelte";
import type { DialogClose } from "$lib/app/dialogs.svelte";
import DialogShell from "$lib/components/DialogShell.svelte";

let {
  paths,
  untrackedPaths,
  close,
}: {
  paths: string[];
  untrackedPaths: string[];
  close: DialogClose<true>;
} = $props();

const total = $derived(paths.length + untrackedPaths.length);
</script>

<DialogShell title="Discard changes" width={440} onClose={() => close()}>
  {#snippet icon()}<Undo2 size={17} class="text-error-500" />{/snippet}
  <p class="mt-3 text-sm leading-6 text-surface-600-400">
    This cannot be undone.
    {#if paths.length > 0}Changes to {paths.length} tracked file{paths.length === 1 ? "" : "s"} will be reverted.{/if}
    {#if untrackedPaths.length > 0}{untrackedPaths.length} untracked file{untrackedPaths.length === 1 ? "" : "s"} will be deleted.{/if}
  </p>
  <ul class="mt-2 max-h-40 overflow-y-auto rounded-md border border-surface-200-800 bg-surface-50-950 p-2 font-mono text-[11px]">
    {#each [...paths, ...untrackedPaths] as path (path)}
      <li class="truncate" title={path}>{path}</li>
    {/each}
  </ul>
  {#snippet footer()}
    <button type="button" onclick={() => close()} class="btn btn-sm preset-tonal">Cancel</button>
    <button type="button" onclick={() => close(true)} class="btn btn-sm preset-filled-error-500">Discard {total} file{total === 1 ? "" : "s"}</button>
  {/snippet}
</DialogShell>
