<script lang="ts">
import SettingRow from "$lib/layout/settings/SettingRow.svelte";
import SettingsCard from "$lib/layout/settings/SettingsCard.svelte";
import { commands } from "$lib/services/homeRouting";
import type { AgentSettings } from "$lib/types";
import { errorMessage } from "$lib/utils/errors";

let agentError = $state<string | null>(null);
let agentSettings = $state<AgentSettings>({
  enabled: false,
  sandbox: "workspace-write",
  maxConcurrent: 4,
  timeoutSeconds: 900,
  sandboxOptions: ["read-only", "workspace-write"],
});

$effect(() => {
  commands
    .readAgentSettings()
    .then((settings) => (agentSettings = settings))
    .catch((cause) => (agentError = errorMessage(cause)));
});

/**
 * The whole number typed into an agent limit, or null when it is empty, not
 * an integer, or outside the input's own min/max (which the browser only
 * hints at). A rejected value snaps back to the stored one: an emptied
 * "Time limit" used to save as 0, which the backend raised to a 1-second
 * timeout that killed every agent.
 */
function agentLimit(input: HTMLInputElement, stored: number): number | null {
  const value = Number(input.value);
  const ok =
    input.value.trim() !== "" && Number.isInteger(value) && value >= Number(input.min) && value <= Number(input.max);
  if (ok) return value;
  input.value = String(stored);
  return null;
}

/** Save one changed field, keeping the rest of the agent settings as they are. */
async function saveAgents(patch: Partial<AgentSettings>) {
  agentError = null;
  const next = { ...agentSettings, ...patch };
  // Show the new value straight away; a rejected save puts the stored one back.
  agentSettings = next;
  try {
    agentSettings = await commands.writeAgentSettings(next);
  } catch (cause) {
    agentError = errorMessage(cause);
    agentSettings = await commands.readAgentSettings().catch(() => next);
  }
}
</script>

<SettingsCard title="Pingex agents" error={agentError}>
  {#snippet description()}
    Stored by Pingex, not <span class="font-mono">config.toml</span>. Applies to new threads.
  {/snippet}
  <div class="mt-4 divide-y divide-surface-200-800">
    <SettingRow
      label="Run subagents as separate processes"
      description="Gives the agent Pingex's own spawn tools, so each subagent runs in its own Codex process that you can watch and stop. New threads only."
    >
      <input
        type="checkbox"
        aria-label="Run subagents as separate processes"
        class="checkbox"
        checked={agentSettings.enabled}
        onchange={(event) => saveAgents({ enabled: event.currentTarget.checked })}
      />
    </SettingRow>
    <SettingRow
      label="Sandbox limit"
      description="The most a spawned agent may do. Nobody is watching to approve its commands, so full access is not offered."
    >
      <select
        class="select w-48"
        value={agentSettings.sandbox}
        disabled={!agentSettings.enabled}
        onchange={(event) => saveAgents({ sandbox: event.currentTarget.value })}
      >
        {#each agentSettings.sandboxOptions as option (option)}
          <option value={option}>{option}</option>
        {/each}
      </select>
    </SettingRow>
    <SettingRow label="Maximum at once" description="How many agents may run concurrently. Each is a separate process.">
      <input
        type="number"
        min="1"
        max="16"
        aria-label="Concurrent agents"
        class="input w-24"
        value={agentSettings.maxConcurrent}
        disabled={!agentSettings.enabled}
        onchange={(event) => {
          const value = agentLimit(event.currentTarget, agentSettings.maxConcurrent);
          if (value !== null) void saveAgents({ maxConcurrent: value });
        }}
      />
    </SettingRow>
    <SettingRow
      label="Time limit"
      description="Seconds before an agent is stopped, so a stuck one cannot hold a slot forever."
    >
      <input
        type="number"
        min="30"
        max="7200"
        step="30"
        aria-label="Agent time limit in seconds"
        class="input w-24"
        value={agentSettings.timeoutSeconds}
        disabled={!agentSettings.enabled}
        onchange={(event) => {
          const value = agentLimit(event.currentTarget, agentSettings.timeoutSeconds);
          if (value !== null) void saveAgents({ timeoutSeconds: value });
        }}
      />
    </SettingRow>
  </div>
</SettingsCard>
