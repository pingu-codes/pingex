<script lang="ts">
import type { DialogClose } from "$lib/app/dialogs.svelte";
import DialogShell from "$lib/components/DialogShell.svelte";

let {
  current,
  next,
  close,
}: {
  /** The objective the thread already holds. */
  current: string;
  /** The objective that would replace it. */
  next: string;
  close: DialogClose<true>;
} = $props();
</script>

<DialogShell title="Replace goal" onClose={() => close()}>
  <p class="mt-3 text-sm leading-6 text-surface-600-400">
    This thread already has a goal:
    <span class="font-medium text-surface-900-100">{current}</span>
  </p>
  <p class="mt-2 text-sm leading-6 text-surface-600-400">
    Replace it with <span class="font-medium text-surface-900-100">{next}</span>?
  </p>

  {#snippet footer()}
    <button type="button" onclick={() => close()} class="btn btn-sm preset-tonal">Cancel</button>
    <button type="button" onclick={() => close(true)} class="btn btn-sm preset-filled-warning-500">Replace goal</button>
  {/snippet}
</DialogShell>
