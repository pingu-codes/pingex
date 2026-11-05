<script lang="ts">
  /**
   * Ask for a line of text about an agent Finding: why it is wrong ("Wrong,
   * tell the agent why…"), or the repository review rule "Skip this kind of
   * finding" saves after the reviewer confirms. Resolves with the trimmed
   * text, or `null` on Cancel.
   */
  import type { DialogClose } from "$lib/app/dialogs.svelte";
  import DialogShell from "$lib/components/DialogShell.svelte";

  let {
    title,
    body,
    label,
    initial = "",
    confirmLabel,
    close,
  }: {
    title: string;
    body: string;
    /** The text box's label. */
    label: string;
    initial?: string;
    confirmLabel: string;
    close: DialogClose<string>;
  } = $props();

  // svelte-ignore state_referenced_locally -- the starting text only.
  let text = $state(initial);
  const empty = $derived(text.trim().length === 0);
</script>

<DialogShell {title} width={460} onClose={() => close()}>
  <p class="mt-3 text-sm leading-6 text-surface-600-400">{body}</p>
  <label class="mt-3 flex flex-col gap-1 text-xs">
    <span class="font-medium">{label}</span>
    <textarea class="field resize-none text-[13px]" rows="3" bind:value={text}></textarea>
  </label>

  {#snippet footer()}
    <button type="button" class="btn btn-sm hover:preset-tonal" onclick={() => close()}>Cancel</button>
    <button
      type="button"
      class="btn btn-sm preset-filled-primary-500"
      disabled={empty}
      onclick={() => close(text.trim())}
    >
      {confirmLabel}
    </button>
  {/snippet}
</DialogShell>
