<script lang="ts">
  import { Check, ChevronDown, Shield } from "@lucide/svelte";
  import ComposerPopover from "$lib/composer/ComposerPopover.svelte";
  import { type HarnessChoice, PERMISSION_PRESETS, presetFor } from "$lib/composer/composerPrefs";
  import { modesFor } from "$lib/composer/harnessModes";
  import { can, harnessName } from "$lib/utils/capabilities";

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
  const modes = $derived(modesFor(harness));
  const presets = $derived(permissionModes ? modes : PERMISSION_PRESETS);
  const selected = $derived(permissionModes ? modes.find((mode) => mode.id === selectedId) : presetFor(selectedId));
  const selectedLabel = $derived(selected?.label ?? "Permissions");
</script>

<ComposerPopover
  {open}
  {onToggle}
  triggerLabel={permissionModes ? `Set ${harnessName(harness)} mode` : "Set permissions level"}
  triggerClass={selectedId === null ? "text-error-500 hover:text-error-500" : ""}
  label="Permission options"
  contentClass="w-[270px]"
>
  {#snippet trigger()}
    <Shield size={12} />
    {selectedLabel}
    <ChevronDown size={11} />
  {/snippet}
  <span class="px-1 eyebrow text-[10px]">Permissions</span>
  <div class="mt-1 space-y-0.5">
    {#each presets as preset (preset.id)}
      <button onclick={() => onChoose(preset.id)} class="menu-row">
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
</ComposerPopover>
