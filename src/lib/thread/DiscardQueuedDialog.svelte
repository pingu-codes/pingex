<script lang="ts">
import type { DialogClose } from "$lib/app/dialogs.svelte";
import DialogShell from "$lib/components/DialogShell.svelte";

let {
  preview,
  close,
}: {
  /** The queued message's text, so the user sees what they are about to lose. */
  preview: string;
  close: DialogClose<true>;
} = $props();
</script>

<DialogShell title="Discard queued message" onClose={() => close()}>
  <p class="mt-3 text-sm leading-6 text-surface-600-400">
    The composer already has text, so this message can't be moved back into it. Removing it from the
    queue discards it for good.
  </p>
  <blockquote class="mt-3 max-h-40 overflow-y-auto whitespace-pre-wrap rounded-lg bg-surface-100-900 px-3 py-2 text-sm text-surface-700-300">
    {preview}
  </blockquote>

  {#snippet footer()}
    <button type="button" onclick={() => close()} class="btn btn-sm preset-tonal">Keep</button>
    <button type="button" onclick={() => close(true)} class="btn btn-sm preset-filled-error-500">Discard</button>
  {/snippet}
</DialogShell>
