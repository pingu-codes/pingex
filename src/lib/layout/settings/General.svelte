<script lang="ts">
  import { classifyVersion, LAST_STABLE, STABLE, type VersionTier } from "$lib/app/codexVersion.svelte";
  import { hostFromOption, hostOverride, hostToOption } from "$lib/app/host";
  import { launch } from "$lib/app/launch.svelte";
  import HostSelect from "$lib/components/HostSelect.svelte";
  import ProfileHomes from "$lib/layout/ProfileHomes.svelte";
  import SettingBadge from "$lib/layout/settings/SettingBadge.svelte";
  import SettingsCard from "$lib/layout/settings/SettingsCard.svelte";
  import { readCodexServerInfo } from "$lib/services/api";
  import { commands, loadProfile } from "$lib/services/homeRouting";
  import type { Account, CodexServerInfo, RuntimeSettings } from "$lib/types";
  import { errorMessage } from "$lib/utils/errors";
  import { codexVersionFromUserAgent } from "$lib/utils/userAgent";

  let {
    account = null,
    codexHome = null,
    codexBinary = null,
  }: {
    account?: Account | null;
    codexHome?: string | null;
    codexBinary?: string | null;
  } = $props();

  let runtimeSettings = $state<RuntimeSettings | null>(null);
  let settingsHome = $state("");
  let settingsBinary = $state("");
  let settingsClaudeBinary = $state("");
  let settingsClaudeConfig = $state("");
  // Where each harness runs: "native" or a WSL distribution name. The selects
  // only appear when a distribution exists, so the form is unchanged elsewhere.
  let settingsCodexHost = $state("native");
  let settingsClaudeHost = $state("native");
  let generalError = $state<string | null>(null);
  let generalSaved = $state(false);
  // The directory the form loaded with, so a save can say what changed.
  let loadedClaudeConfig = $state("");
  let claudeConfigMoved = $state(false);
  let serverInfo = $state<CodexServerInfo | null>(null);
  const serverVersion = $derived(codexVersionFromUserAgent(serverInfo?.userAgent));
  const serverTier = $derived(serverVersion ? classifyVersion(serverVersion) : null);
  const TIER_LABEL: Record<VersionTier, string> = {
    supported: "supported",
    older: `older than last stable ${LAST_STABLE} — may not work`,
    newer: `newer than stable ${STABLE} — untested`,
    unstable: "unreleased build — untested",
  };

  $effect(() => {
    generalError = null;
    commands
      .readRuntimeSettings()
      .then((settings) => {
        runtimeSettings = settings;
        settingsHome = settings.overrideCodexHome ?? "";
        settingsBinary = settings.overrideCodexBinary ?? "";
        settingsClaudeBinary = settings.overrideClaudeBinary ?? "";
        settingsClaudeConfig = settings.overrideClaudeConfigDir ?? "";
        loadedClaudeConfig = settingsClaudeConfig;
        settingsCodexHost = hostToOption(settings.overrideCodexHost);
        settingsClaudeHost = hostToOption(settings.overrideClaudeHost);
      })
      .catch((cause) => (generalError = errorMessage(cause)));
    // Best effort: a Codex that will not start is already reported elsewhere.
    readCodexServerInfo()
      .then((info) => (serverInfo = info))
      .catch(() => (serverInfo = null));
  });

  async function saveGeneral(event: SubmitEvent) {
    event.preventDefault();
    generalError = null;
    generalSaved = false;
    try {
      runtimeSettings = await commands.updateRuntimeSettings(
        settingsHome.trim() || null,
        settingsBinary.trim() || null,
        settingsClaudeBinary.trim() || null,
        settingsClaudeConfig.trim() || null,
        hostOverride(hostFromOption(settingsCodexHost)),
        hostOverride(hostFromOption(settingsClaudeHost)),
      );
      claudeConfigMoved = settingsClaudeConfig.trim() !== loadedClaudeConfig.trim();
      loadedClaudeConfig = settingsClaudeConfig.trim();
      // A changed Claude directory registers a Home and makes it this Host's
      // default; reload so the Homes list below and the routing agree.
      if (claudeConfigMoved) await loadProfile(false);
      generalSaved = true;
    } catch (cause) {
      generalError = errorMessage(cause);
    }
  }
</script>

<SettingsCard title="Runtime" description="Which Codex home and binary this app talks to.">
  <div class="mt-4 text-sm">
    <div class="text-xs font-medium text-surface-500">Signed in as</div>
    <div class="mt-1">{account?.label ?? "Not signed in"}</div>
  </div>
  {#if serverInfo?.userAgent}
    <div class="mt-3 text-sm">
      <div class="text-xs font-medium text-surface-500">Running Codex</div>
      <div class="mt-1" title={serverInfo.userAgent}>
        {serverVersion ? `codex ${serverVersion}` : serverInfo.userAgent}
        {#if serverInfo.platformOs}
          <span class="text-surface-500"> · {serverInfo.platformOs}</span>
        {/if}
        {#if serverTier}
          <span class={serverTier === "supported" ? "text-surface-500" : "text-warning-600-400"}>
            · {TIER_LABEL[serverTier]}</span
          >
        {/if}
      </div>
    </div>
  {/if}
  <ProfileHomes />
  <form onsubmit={saveGeneral} class="mt-4 space-y-4 text-sm">
    <div>
      {@render fieldLabel("settings-codex-home", "CODEX_HOME", true)}
      <input
        id="settings-codex-home"
        bind:value={settingsHome}
        placeholder={runtimeSettings?.codexHome ?? codexHome ?? "~/.codex"}
        class="input mt-1 w-full font-mono text-xs"
      >
    </div>
    <div>
      {@render fieldLabel("settings-codex-binary", "Codex binary", true)}
      <input
        id="settings-codex-binary"
        bind:value={settingsBinary}
        placeholder={runtimeSettings?.codexBinary ?? codexBinary ?? "codex"}
        class="input mt-1 w-full font-mono text-xs"
      >
    </div>
    {#if launch.distros.length > 0}
      <div>
        {@render fieldLabel("settings-codex-host", "Codex runs in", true)}
        <HostSelect
          id="settings-codex-host"
          bind:value={settingsCodexHost}
          class="mt-1 w-full text-xs"
          data-testid="settings-codex-host"
        />
        {@render hint(
          "The home, the binary and every project of a WSL home are Linux paths inside that distribution. Homes picked from the launcher keep the host they were picked with.",
        )}
      </div>
    {/if}
    <div>
      {@render fieldLabel("settings-claude-binary", "Claude binary")}
      <input
        id="settings-claude-binary"
        bind:value={settingsClaudeBinary}
        placeholder="claude"
        class="input mt-1 w-full font-mono text-xs"
      >
    </div>
    {#if launch.distros.length > 0}
      <div>
        {@render fieldLabel("settings-claude-host", "Claude runs in")}
        <HostSelect
          id="settings-claude-host"
          bind:value={settingsClaudeHost}
          class="mt-1 w-full text-xs"
          data-testid="settings-claude-host"
        />
      </div>
    {/if}
    <div>
      {@render fieldLabel("settings-claude-config", "Claude config directory")}
      <input
        id="settings-claude-config"
        bind:value={settingsClaudeConfig}
        placeholder="~/.claude"
        class="input mt-1 w-full font-mono text-xs"
      >
      {@render hint(
        "Where Claude Code keeps its login (CLAUDE_CONFIG_DIR). Set this if you log in with a non-default directory — a GUI launch does not see shell exports.",
      )}
    </div>
    <div class="flex items-center gap-3">
      <button type="submit" class="btn btn-sm preset-filled-primary-500">Save</button>
      {#if generalSaved}
        <span class="text-xs {runtimeSettings?.restartRequired ? "text-warning-500" : "text-success-500"}">
          {#if runtimeSettings?.restartRequired}
            Saved — restart Pingex to apply.
          {:else if claudeConfigMoved}
            Saved — new Claude conversations use {loadedClaudeConfig || "~/.claude"}. Existing ones keep their Home.
          {:else}
            Saved.
          {/if}
        </span>
      {/if}
    </div>
    <p class="text-[11px] leading-4 text-surface-500">
      Leave a field blank to use the CLI/environment value; a binary that cannot be run is rejected on save. Overrides
      are stored in
      <span class="font-mono">{runtimeSettings?.settingsPath ?? "settings.json"}</span>.
    </p>
    {#if generalError}
      <div class="error-note">{generalError}</div>
    {/if}
  </form>
</SettingsCard>

{#snippet fieldLabel(id: string, label: string, restart = false)}
  <div class="flex items-center gap-2">
    <label for={id} class="text-xs font-medium text-surface-500">{label}</label>
    {#if restart}
      <SettingBadge kind="restart" />
    {/if}
  </div>
{/snippet}

{#snippet hint(text: string)}
  <p class="mt-1 text-[11px] leading-4 text-surface-500">{text}</p>
{/snippet}
