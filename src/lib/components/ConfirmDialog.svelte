<script lang="ts">
/**
 * A yes/no confirmation: `await openDialog(ConfirmDialog, { ... })` resolves
 * `true` on confirm and `null` on Cancel or any other dismissal.
 */
import type { Component } from "svelte";
import type { DialogClose } from "$lib/app/dialogs.svelte";
import DialogShell from "$lib/components/DialogShell.svelte";

let {
  title,
  body,
  detail,
  confirmLabel,
  cancelLabel = "Cancel",
  tone = "error",
  icon: Icon,
  width,
  close,
}: {
  title: string;
  /** The explanation; one paragraph per entry. */
  body: string | string[];
  /** What the action touches, below the copy: a text excerpt, or a list (e.g. paths). */
  detail?: string | string[];
  confirmLabel: string;
  cancelLabel?: string;
  tone?: "error" | "warning";
  icon?: Component<{ size?: number; class?: string }>;
  width?: number;
  close: DialogClose<true>;
} = $props();

const paragraphs = $derived(typeof body === "string" ? [body] : body);
const toneClass = $derived(
  tone === "warning"
    ? { icon: "text-warning-500", button: "preset-filled-warning-500" }
    : { icon: "text-error-500", button: "preset-filled-error-500" },
);
</script>

<DialogShell {title} {width} onClose={() => close()}>
  {#snippet icon()}{#if Icon}<Icon size={17} class={toneClass.icon} />{/if}{/snippet}
  {#each paragraphs as paragraph, index (index)}
    <p class="{index === 0 ? 'mt-3' : 'mt-2'} text-sm leading-6 text-surface-600-400">{paragraph}</p>
  {/each}
  {#if typeof detail === "string"}
    <blockquote class="mt-3 max-h-40 overflow-y-auto whitespace-pre-wrap rounded-lg bg-surface-100-900 px-3 py-2 text-sm text-surface-700-300">{detail}</blockquote>
  {:else if detail}
    <ul class="mt-2 max-h-40 overflow-y-auto rounded-md border border-surface-200-800 bg-surface-50-950 p-2 font-mono text-[11px]">
      {#each detail as item (item)}
        <li class="truncate" title={item}>{item}</li>
      {/each}
    </ul>
  {/if}

  {#snippet footer()}
    <button type="button" onclick={() => close()} class="btn btn-sm preset-tonal">{cancelLabel}</button>
    <button type="button" onclick={() => close(true)} class="btn btn-sm {toneClass.button}">{confirmLabel}</button>
  {/snippet}
</DialogShell>
