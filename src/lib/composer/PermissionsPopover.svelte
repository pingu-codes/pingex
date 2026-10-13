<script lang="ts">
import { Check, ChevronDown, Shield } from "@lucide/svelte";
import { CLAUDE_MODES } from "$lib/composer/claudeModes";
import { type HarnessChoice, permissionPresetsFor, presetFor } from "$lib/composer/composerPrefs.svelte";
import { positionComposerPopover } from "$lib/composer/positionComposerPopover";
import { can } from "$lib/utils/capabilities";

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

const permissionModes = $derived(can(harness, "permissionModes"));
const presets = $derived(permissionModes ? CLAUDE_MODES : permissionPresetsFor(harness));
const selected = $derived(
  permissionModes ? CLAUDE_MODES.find((mode) => mode.id === selectedId) : presetFor(harness, selectedId),
);
const selectedLabel = $derived(selected?.label ?? "Permissions");
</script>

<div class="relative">
  <button
    onclick={(event) => {
      event.stopPropagation();
      onToggle();
    }}
    aria-label={permissionModes ? "Set Claude mode" : "Set permissions level"}
    class="pill {selectedId === null ? 'text-error-500 hover:text-error-500' : ''}"
  >
    <Shield size={12} />
    {selectedLabel}
    <ChevronDown size={11} />
  </button>
  {#if open}
    <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events, a11y_interactive_supports_focus -->
    <div
      class="popover-card absolute bottom-8 left-0 z-50 w-[270px] select-none p-2"
      onclick={(event) => event.stopPropagation()}
      use:positionComposerPopover
      role="dialog"
      aria-label="Permission options"
    >
      <span class="px-1 eyebrow text-[10px]">Permissions</span>
      <div class="mt-1 space-y-0.5">
        {#each presets as preset (preset.id)}
          <button
            onclick={() => onChoose(preset.id)}
            class="menu-row"
          >
            <span class="min-w-0 flex-1">
              <span class="block">{preset.label}</span>
              <span class="block text-[10px] leading-4 text-surface-500">{preset.description}</span>
            </span>
            {#if selected?.id === preset.id}
              <Check size={13} class="shrink-0 text-primary-500" />
            {/if}
          </button>
        {/each}
      </div>
    </div>
  {/if}
</div>
