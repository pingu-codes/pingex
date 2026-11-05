<!-- A trash icon that asks once more inline before deleting. -->
<script lang="ts">
  import { Trash2 } from "@lucide/svelte";
  import TooltipButton from "$lib/components/TooltipButton.svelte";

  let {
    label,
    confirmLabel,
    onConfirm,
  }: {
    /** Tooltip and accessible name of the icon, e.g. "Remove github". */
    label: string;
    /** The confirming button, e.g. "Confirm remove". */
    confirmLabel: string;
    onConfirm: () => Promise<unknown>;
  } = $props();

  let confirming = $state(false);

  async function confirm() {
    await onConfirm();
    confirming = false;
  }
</script>

{#if confirming}
  <button type="button" onclick={confirm} class="btn btn-sm preset-filled-error-500 text-xs">{confirmLabel}</button>
  <button type="button" onclick={() => (confirming = false)} class="btn btn-sm hover:preset-tonal text-xs">
    Cancel
  </button>
{:else}
  <TooltipButton
    {label}
    type="button"
    onclick={() => (confirming = true)}
    aria-label={label}
    class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500"
  >
    <Trash2 size={13} />
  </TooltipButton>
{/if}
