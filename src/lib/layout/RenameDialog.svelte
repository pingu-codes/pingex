<script lang="ts">
import type { DialogClose } from "$lib/app/dialogs.svelte";
import DialogShell from "$lib/components/DialogShell.svelte";

let {
  kind,
  current,
  title = `Rename ${kind}`,
  submitLabel = "Rename",
  close,
}: {
  kind: "project" | "thread" | "section" | "folder";
  /** The name to pre-fill and edit. */
  current: string;
  title?: string;
  submitLabel?: string;
  /** Resolves the new name, or nothing when dismissed. */
  close: DialogClose<string>;
} = $props();

// Seeded once: the dialog is mounted per opening.
// svelte-ignore state_referenced_locally
let value = $state(current);

function submit(event: SubmitEvent) {
  event.preventDefault();
  const name = value.trim();
  if (name) close(name);
}
</script>

<DialogShell {title} onClose={() => close()}>
  <form onsubmit={submit} class="mt-4">
    <!-- svelte-ignore a11y_autofocus -->
    <input bind:value autofocus class="input w-full" placeholder="{kind.charAt(0).toUpperCase() + kind.slice(1)} name" />
    <div class="mt-4 flex justify-end gap-2">
      <button type="button" onclick={() => close()} class="btn btn-sm preset-tonal">Cancel</button>
      <button type="submit" class="btn btn-sm preset-filled-primary-500" disabled={!value.trim()}>{submitLabel}</button>
    </div>
  </form>
</DialogShell>
