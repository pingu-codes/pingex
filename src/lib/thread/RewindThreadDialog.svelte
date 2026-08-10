<script lang="ts">
import type { DialogClose } from "$lib/app/dialogs.svelte";
import DialogShell from "$lib/components/DialogShell.svelte";

let {
  turnCount,
  close,
}: {
  /** Turns that will be dropped: the edited one and everything after it. */
  turnCount: number;
  close: DialogClose<true>;
} = $props();
</script>

<DialogShell title="Rewind conversation" onClose={() => close()}>
  <p class="mt-3 text-sm leading-6 text-surface-600-400">
    Resending this message rewinds the conversation to that point, permanently discarding
    <span class="font-medium text-surface-900-100"
      >{turnCount} {turnCount === 1 ? "turn" : "turns"}</span
    > of history. This cannot be undone.
  </p>
  <p class="mt-2 text-sm leading-6 text-surface-600-400">
    Files Codex already changed stay as they are — only the conversation is rewound.
  </p>

  {#snippet footer()}
    <button type="button" onclick={() => close()} class="btn btn-sm preset-tonal">Cancel</button>
    <button type="button" onclick={() => close(true)} class="btn btn-sm preset-filled-error-500">
      Rewind &amp; resend
    </button>
  {/snippet}
</DialogShell>
