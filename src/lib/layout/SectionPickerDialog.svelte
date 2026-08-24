<script lang="ts">
import { Bookmark, Plus, X } from "@lucide/svelte";
import type { DialogClose } from "$lib/app/dialogs.svelte";
import DialogShell from "$lib/components/DialogShell.svelte";
import type { ThreadSection } from "$lib/types";

/** What the picker resolves with: an existing section, a new one to create,
 *  or "take the thread out of its section". */
export type SectionChoice =
  | { kind: "existing"; section: ThreadSection }
  | { kind: "new"; name: string; color: string | null }
  | { kind: "none" };

let {
  sections,
  current = null,
  close,
}: {
  sections: ThreadSection[];
  /** The section the thread is in now, if any. */
  current?: string | null;
  close: DialogClose<SectionChoice>;
} = $props();

const SWATCHES = ["#ef4444", "#f59e0b", "#22c55e", "#3b82f6", "#a855f7", "#ec4899"];

let creating = $state(false);
let name = $state("");
let color = $state<string | null>(null);

function submitNew(event: SubmitEvent) {
  event.preventDefault();
  const trimmed = name.trim();
  if (trimmed) close({ kind: "new", name: trimmed, color });
}
</script>

<DialogShell title="Move to section" subtitle="Sections group threads across every project." width={420} onClose={() => close()}>
  {#snippet icon()}<Bookmark size={17} class="text-primary-500" />{/snippet}

  <div class="mt-4 space-y-1">
    {#each sections as section (section.id)}
      <button
        class="flex w-full items-center gap-3 rounded-lg border border-surface-200-800 px-3 py-2 text-left text-xs hover:preset-tonal {section.id === current ? 'preset-tonal' : ''}"
        onclick={() => close({ kind: "existing", section })}
        aria-current={section.id === current ? "true" : undefined}
      >
        <span class="size-2.5 shrink-0 rounded-full" style="background: {section.color ?? 'var(--color-surface-400)'}"></span>
        <span class="min-w-0 flex-1 truncate font-medium">{section.name}</span>
        {#if section.id === current}<span class="text-[10px] text-surface-500">current</span>{/if}
      </button>
    {/each}
    {#if current}
      <button
        class="flex w-full items-center gap-3 rounded-lg border border-dashed border-surface-300-700 px-3 py-2 text-left text-xs text-surface-600-400 hover:preset-tonal"
        onclick={() => close({ kind: "none" })}
      >
        <X size={13} class="shrink-0" />
        Remove from section
      </button>
    {/if}
  </div>

  {#if creating}
    <form onsubmit={submitNew} class="mt-3 space-y-3 rounded-lg border border-surface-200-800 p-3">
      <!-- svelte-ignore a11y_autofocus -->
      <input bind:value={name} autofocus class="input w-full" placeholder="Section name" aria-label="Section name" />
      <div class="flex items-center gap-2" role="radiogroup" aria-label="Section colour">
        {#each SWATCHES as swatch (swatch)}
          <button
            type="button"
            role="radio"
            aria-checked={color === swatch}
            aria-label={swatch}
            onclick={() => (color = color === swatch ? null : swatch)}
            class="size-5 rounded-full border-2 transition {color === swatch ? 'border-surface-900-100 scale-110' : 'border-transparent'}"
            style="background: {swatch}"
          ></button>
        {/each}
      </div>
      <div class="flex justify-end gap-2">
        <button type="button" onclick={() => (creating = false)} class="btn btn-sm preset-tonal">Cancel</button>
        <button type="submit" class="btn btn-sm preset-filled-primary-500" disabled={!name.trim()}>Create and move</button>
      </div>
    </form>
  {:else}
    <button
      type="button"
      onclick={() => (creating = true)}
      class="mt-3 flex w-full items-center gap-2 rounded-lg px-3 py-2 text-left text-xs font-medium text-primary-500 hover:preset-tonal"
    >
      <Plus size={13} />
      New section…
    </button>
  {/if}
</DialogShell>
