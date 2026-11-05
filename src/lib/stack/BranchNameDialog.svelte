<!-- Name a git branch: a new one on the stack, or a new name for one.
     Resolves the name, or null. -->
<script lang="ts">
  import type { DialogClose } from "$lib/app/dialogs.svelte";
  import DialogShell from "$lib/components/DialogShell.svelte";

  let {
    title,
    subtitle = null,
    initial = "",
    confirmLabel,
    close,
  }: {
    title: string;
    subtitle?: string | null;
    initial?: string;
    confirmLabel: string;
    close: DialogClose<string>;
  } = $props();

  // svelte-ignore state_referenced_locally -- starts from the current name.
  let value = $state(initial);
  const name = $derived(value.trim());
  const ready = $derived(name.length > 0 && name !== initial && !/\s/.test(name) && !name.startsWith("-"));
</script>

<DialogShell {title} {subtitle} onClose={() => close()}>
  <form
    id="branch-name-form"
    onsubmit={(event) => {
      event.preventDefault();
      if (ready) close(name);
    }}
  >
    <label class="flex flex-col gap-1 text-xs">
      <span class="font-medium">Git branch name</span>
      <!-- svelte-ignore a11y_autofocus -->
      <input class="field font-mono text-xs" bind:value data-autofocus>
    </label>
  </form>
  {#snippet footer()}
    <button type="button" class="btn btn-sm hover:preset-tonal" onclick={() => close()}>Cancel</button>
    <button type="submit" form="branch-name-form" class="btn btn-sm preset-filled-primary-500" disabled={!ready}>
      {confirmLabel}
    </button>
  {/snippet}
</DialogShell>
