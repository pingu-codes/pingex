<!-- Change the git branch a PR merges into. Resolves the new base, or null. -->
<script lang="ts">
  import type { DialogClose } from "$lib/app/dialogs.svelte";
  import DialogShell from "$lib/components/DialogShell.svelte";

  let { number, base, close }: { number: number; base: string; close: DialogClose<string> } = $props();

  // svelte-ignore state_referenced_locally -- starts from the current base.
  let value = $state(base);
  const ready = $derived(value.trim().length > 0 && value.trim() !== base);
</script>

<DialogShell title="Retarget #{number}" subtitle="Now into {base}" onClose={() => close()}>
  <form
    id="retarget-form"
    onsubmit={(event) => {
      event.preventDefault();
      if (ready) close(value.trim());
    }}
  >
    <label class="flex flex-col gap-1 text-xs">
      <span class="font-medium">New base git branch</span>
      <input class="field font-mono text-xs" bind:value>
    </label>
  </form>
  {#snippet footer()}
    <button type="button" class="btn btn-sm hover:preset-tonal" onclick={() => close()}>Cancel</button>
    <button type="submit" form="retarget-form" class="btn btn-sm preset-filled-primary-500" disabled={!ready}>
      Retarget
    </button>
  {/snippet}
</DialogShell>
