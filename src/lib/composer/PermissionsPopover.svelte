<script lang="ts">
import { Check, ChevronDown, Shield } from "@lucide/svelte";
import { type HarnessChoice, permissionPresetsFor } from "$lib/composer/composerPrefs.svelte";

let {
  open,
  selectedId,
  harness = null,
  onToggle,
  onChoose,
}: {
  open: boolean;
  selectedId: string | null;
  harness?: HarnessChoice | null;
  onToggle: () => void;
  onChoose: (id: string) => void;
} = $props();

const presets = $derived(permissionPresetsFor(harness));
const selectedLabel = $derived(presets.find((preset) => preset.id === selectedId)?.label ?? "Permissions");
</script>

<div class="relative">
  <button
    onclick={(event) => {
      event.stopPropagation();
      onToggle();
    }}
    aria-label="Set permissions level"
    class="inline-flex items-center gap-1.5 rounded-full px-2 py-1 text-[11px] transition hover:bg-surface-200-800
      {selectedId === null ? 'text-error-500' : 'text-surface-500 hover:text-surface-800-200'}"
  >
    <Shield size={12} />
    {selectedLabel}
    <ChevronDown size={11} />
  </button>
  {#if open}
    <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events, a11y_interactive_supports_focus -->
    <div
      class="card absolute bottom-8 left-0 z-50 w-[270px] select-none border border-surface-200-800 bg-surface-50-950 p-2 shadow-xl"
      onclick={(event) => event.stopPropagation()}
      role="dialog"
      aria-label="Permission options"
    >
      <span class="px-1 text-[10px] font-semibold uppercase tracking-[0.08em] text-surface-500">Permissions</span>
      <div class="mt-1 space-y-0.5">
        {#each presets as preset (preset.id)}
          <button
            onclick={() => onChoose(preset.id)}
            class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-xs hover:preset-tonal"
          >
            <span class="min-w-0 flex-1">
              <span class="block">{preset.label}</span>
              <span class="block text-[10px] leading-4 text-surface-500">{preset.description}</span>
            </span>
            {#if selectedId === preset.id}
              <Check size={13} class="shrink-0 text-primary-500" />
            {/if}
          </button>
        {/each}
      </div>
    </div>
  {/if}
</div>
