<script lang="ts">
import { RotateCcw } from "@lucide/svelte";
import TooltipButton from "$lib/components/TooltipButton.svelte";
import SettingBadge from "$lib/layout/settings/SettingBadge.svelte";
import SettingRow from "$lib/layout/settings/SettingRow.svelte";
import SettingsCard from "$lib/layout/settings/SettingsCard.svelte";
import { commands } from "$lib/services/homeRouting";
import type { ConfigSetting } from "$lib/types";
import { errorMessage } from "$lib/utils/errors";

// The `config.toml` keys of one settings section (Agent, Model features or
// Coding), each with its source and a reset to the default.
let { section, title }: { section: string; title: string } = $props();

let configSettings = $state<ConfigSetting[]>([]);
let configError = $state<string | null>(null);
const settings = $derived(configSettings.filter((setting) => setting.section === section));

$effect(() => {
  commands
    .readConfigSettings()
    .then((loaded) => (configSettings = loaded))
    .catch((cause) => (configError = errorMessage(cause)));
});

async function setConfig(setting: ConfigSetting, value: string | null, unset: boolean) {
  configError = null;
  try {
    configSettings = await commands.writeConfigSetting(setting.key, value, unset);
  } catch (cause) {
    configError = errorMessage(cause);
  }
}
</script>

<SettingsCard {title} error={configError}>
  {#snippet description()}
    Stored in <span class="font-mono">config.toml</span>. Changes apply to the next thread.
  {/snippet}
  <div class="mt-4 divide-y divide-surface-200-800">
    {#each settings as setting (setting.key)}
      <SettingRow label={setting.label} data-testid="config-control" data-key={setting.key}>
        {#snippet badges()}
          <SettingBadge kind={setting.source === "config" ? "config" : "default"} />
          {#if setting.restartRequired}<SettingBadge kind="restart" />{/if}
        {/snippet}
        {#snippet description()}
          {#if setting.source === "default"}
            Inherited default{setting.value ? `: ${setting.value}` : ""}
          {:else}
            Set in config.toml{setting.default ? ` · default ${setting.default}` : ""}
          {/if}
        {/snippet}
        {#if setting.kind === "enum"}
          <select
            aria-label={setting.label}
            class="select text-sm"
            value={setting.value ?? ""}
            onchange={(event) => setConfig(setting, event.currentTarget.value, false)}
          >
            {#each setting.options as option (option)}
              <option value={option}>{option}</option>
            {/each}
          </select>
        {:else if setting.kind === "bool"}
          <input
            type="checkbox"
            aria-label={setting.label}
            class="checkbox"
            checked={setting.value === "true"}
            onchange={(event) => setConfig(setting, event.currentTarget.checked ? "true" : "false", false)}
          />
        {:else}
          <input
            aria-label={setting.label}
            class="input w-44 font-mono text-xs"
            value={setting.value ?? ""}
            placeholder={setting.default ?? "inherit"}
            onchange={(event) => {
              const next = event.currentTarget.value.trim();
              setConfig(setting, next || null, next.length === 0);
            }}
          />
        {/if}
        {#if setting.source === "config"}
          <TooltipButton
            label="Reset to default"
            type="button"
            aria-label="Reset {setting.label} to default"
            onclick={() => setConfig(setting, null, true)}
            class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500"
          >
            <RotateCcw size={14} />
          </TooltipButton>
        {/if}
      </SettingRow>
    {:else}
      <p class="py-3 text-xs text-surface-500">No editable settings in this section.</p>
    {/each}
  </div>
</SettingsCard>
