<script lang="ts">
import { Search, Settings2, X } from "@lucide/svelte";
import TooltipButton from "$lib/components/TooltipButton.svelte";
import IntegrationsSection from "$lib/integrations/IntegrationsSection.svelte";
import Connections from "$lib/layout/Connections.svelte";
import Advanced from "$lib/layout/settings/Advanced.svelte";
import Appearance from "$lib/layout/settings/Appearance.svelte";
import ConfigToml from "$lib/layout/settings/ConfigToml.svelte";
import General from "$lib/layout/settings/General.svelte";
import Keyboard from "$lib/layout/settings/Keyboard.svelte";
import SettingRow from "$lib/layout/settings/SettingRow.svelte";
import SettingsCard from "$lib/layout/settings/SettingsCard.svelte";
import { filterSections, SETTINGS_SECTIONS, type SettingsSectionId } from "$lib/layout/settingsSections";
import { commands } from "$lib/services/homeRouting";
import { settingsNav } from "$lib/services/settingsNav.svelte";
import { isTauri } from "$lib/services/tauri";
import type { Account } from "$lib/types";
import UsageSummary from "$lib/usage/UsageSummary.svelte";
import { dragRegion } from "$lib/utils/dragRegion";

let {
  account = null,
  codexHome = null,
  codexBinary = null,
  onClose,
}: {
  account?: Account | null;
  codexHome?: string | null;
  codexBinary?: string | null;
  onClose: () => void;
} = $props();

let query = $state("");
let activeSection = $state<SettingsSectionId>("general");
// Jump to the requested section whenever a settingsNav request fires.
$effect(() => {
  void settingsNav.nonce;
  activeSection = settingsNav.section;
});
const filtered = $derived(filterSections(SETTINGS_SECTIONS, query));
// Keep the active section valid as the filter narrows the list.
const current = $derived(filtered.find((section) => section.id === activeSection) ?? filtered[0] ?? null);
const currentSection = $derived(current?.id ?? null);

async function reveal(path: string | null | undefined) {
  if (!path) return;
  try {
    await commands.revealInFinder(path);
  } catch {
    // Reveal is best-effort.
  }
}
</script>

<svelte:window
  onkeydown={(event) => {
    if (event.key !== "Escape") return;
    // Let a child that already claimed Escape win (shortcut recording cancels,
    // an inline rename cancels) rather than closing the whole panel.
    if (event.defaultPrevented) return;
    event.preventDefault();
    onClose();
  }}
/>

<div class="flex h-screen min-h-[560px] flex-col bg-surface-50-950 text-surface-950-50">
  <header
    class="flex h-14 shrink-0 items-center justify-between border-b border-surface-200-800 px-5 select-none {isTauri() ? 'pl-20' : ''}"
    data-tauri-drag-region
    use:dragRegion
  >
    <div class="pointer-events-none flex items-center gap-2 text-sm font-semibold">
      <Settings2 size={16} class="text-surface-500" />
      Settings
    </div>
    <TooltipButton label="Close settings" onclick={onClose} aria-label="Close settings" class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500">
      <X size={16} />
    </TooltipButton>
  </header>

  <div class="flex min-h-0 flex-1">
    <!-- Left nav -->
    <nav class="flex w-60 shrink-0 flex-col border-r border-surface-200-800 p-3">
      <div class="relative mb-3">
        <Search size={14} class="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-surface-500" />
        <input
          bind:value={query}
          placeholder="Search settings"
          aria-label="Search settings"
          class="input w-full pl-8 text-sm"
        />
      </div>
      <div class="flex flex-col gap-0.5 overflow-y-auto" role="tablist">
        {#each filtered as section (section.id)}
          {@const Icon = section.icon}
          <button
            role="tab"
            aria-selected={currentSection === section.id}
            data-testid="settings-nav-item"
            onclick={() => (activeSection = section.id)}
            class="flex items-center gap-2.5 rounded-lg px-2.5 py-2 text-left text-sm transition {currentSection ===
            section.id
              ? 'preset-tonal-primary font-medium'
              : 'hover:preset-tonal text-surface-700-300'}"
          >
            <Icon size={15} class="shrink-0" />
            {section.label}
          </button>
        {/each}
        {#if filtered.length === 0}
          <p class="px-2.5 py-4 text-xs text-surface-500">No settings match "{query}".</p>
        {/if}
      </div>
    </nav>

    <!-- Right content -->
    <div class="min-w-0 flex-1 overflow-y-auto">
      <div class="mx-auto max-w-2xl space-y-6 p-6">
        {#if currentSection === "general"}
          <General {account} {codexHome} {codexBinary} />
        {:else if currentSection === "appearance"}
          <Appearance />
        {:else if current && (currentSection === "agent" || currentSection === "modelFeatures" || currentSection === "coding")}
          <ConfigToml section={current.id} title={current.label} />
        {:else if currentSection === "integrations"}
          <IntegrationsSection
            focusServer={settingsNav.focusServer}
            focusTool={settingsNav.focusTool}
            nonce={settingsNav.nonce}
            onGoToConnections={() => (activeSection = "connections")}
          />
        {:else if currentSection === "connections"}
          <Connections active />
        {:else if currentSection === "keyboard"}
          <Keyboard />
        {:else if currentSection === "usage"}
          <SettingsCard title="Usage" description="Token spend across every project in this Home, by category, model and thread.">
            <div class="mt-4"><UsageSummary scope={{ kind: "global" }} /></div>
          </SettingsCard>
        {:else if currentSection === "data"}
          <SettingsCard title="Data controls" description="Local metadata and storage locations.">
            <div class="mt-1">
              <SettingRow label="Codex home & database">
                {#snippet description()}
                  <span class="block truncate font-mono">{codexHome ?? "~/.codex"}</span>
                {/snippet}
                <button type="button" onclick={() => reveal(codexHome)} class="btn btn-sm preset-tonal">Reveal</button>
              </SettingRow>
            </div>
            <p class="text-[11px] leading-4 text-surface-500">
              Per-project message drafts and the frontend database live under this folder. Removing a project from the
              sidebar clears its draft.
            </p>
          </SettingsCard>
        {:else if currentSection === "advanced"}
          <Advanced />
        {/if}
      </div>
    </div>
  </div>
</div>
