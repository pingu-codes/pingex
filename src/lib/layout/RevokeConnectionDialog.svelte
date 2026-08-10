<script lang="ts">
import { TriangleAlert } from "@lucide/svelte";
import type { DialogClose } from "$lib/app/dialogs.svelte";
import DialogShell from "$lib/components/DialogShell.svelte";
import { canRevoke } from "$lib/layout/connectionState";
import type { RemoteConnection } from "$lib/types";

let {
  connection,
  close,
}: {
  connection: RemoteConnection;
  close: DialogClose<true>;
} = $props();

let typed = $state("");

const confirmable = $derived(canRevoke(typed, connection.name));

function confirm() {
  if (confirmable) close(true);
}
</script>

<DialogShell title="Revoke access" titleClass="text-error-500" width={440} onClose={() => close()}>
  {#snippet icon()}<TriangleAlert size={17} />{/snippet}

  <p class="mt-3 text-sm leading-6 text-surface-600-400">
    This permanently revokes
    <span class="font-medium text-surface-900-100">{connection.name}</span>'s
    credential. The device must be paired again to reconnect. This cannot be undone.
  </p>
  <label for="revoke-confirm" class="mt-4 block text-xs font-medium text-surface-500">
    Type <span class="font-mono text-surface-900-100">{connection.name}</span> to confirm
  </label>
  <input id="revoke-confirm" bind:value={typed} autocomplete="off" class="input mt-1 w-full text-sm" placeholder={connection.name} />

  {#snippet footer()}
    <button type="button" onclick={() => close()} class="btn btn-sm preset-tonal">Cancel</button>
    <button type="button" onclick={confirm} disabled={!confirmable} class="btn btn-sm preset-filled-error-500 disabled:opacity-40">
      Revoke access
    </button>
  {/snippet}
</DialogShell>
