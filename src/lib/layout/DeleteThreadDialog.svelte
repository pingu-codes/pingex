<script lang="ts">
import type { DialogClose } from "$lib/app/dialogs.svelte";
import DialogShell from "$lib/components/DialogShell.svelte";

let {
  title,
  close,
}: {
  /** Thread title, shown in the confirmation copy. */
  title: string;
  close: DialogClose<true>;
} = $props();
</script>

<DialogShell title="Delete thread" onClose={() => close()}>
  <p class="mt-3 text-sm leading-6 text-surface-600-400">
    Permanently delete <span class="font-medium text-surface-900-100">{title}</span>? This cannot be undone.
  </p>

  {#snippet footer()}
    <button type="button" onclick={() => close()} class="btn btn-sm preset-tonal">Cancel</button>
    <button type="button" onclick={() => close(true)} class="btn btn-sm preset-filled-error-500">Delete</button>
  {/snippet}
</DialogShell>
