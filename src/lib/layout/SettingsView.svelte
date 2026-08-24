<script lang="ts">
import {
  Blocks,
  Bot,
  Code2,
  Database,
  FlaskConical,
  Keyboard,
  Palette,
  RotateCcw,
  Search,
  Settings2,
  Sliders,
  Smartphone,
  X,
} from "@lucide/svelte";
import TooltipButton from "$lib/components/TooltipButton.svelte";
import IntegrationsSection from "$lib/integrations/IntegrationsSection.svelte";
import { appearance, FONT_SIZE_MAX, FONT_SIZE_MIN } from "$lib/layout/appearancePrefs.svelte";
import Connections from "$lib/layout/Connections.svelte";
import { messageLog } from "$lib/layout/messageLogPrefs.svelte";
import { filterSections, SETTINGS_SECTIONS } from "$lib/layout/settingsSections";
import { acceleratorFromEvent } from "$lib/quick/quickChat";
import {
  DEFAULT_QUICK_SHORTCUT,
  getQuickShortcut,
  isTauri,
  readAgentSettings,
  readCodexServerInfo,
  readConfigSettings,
  readHomeOverview,
  readRuntimeSettings,
  revealInFinder,
  setQuickShortcut,
  updateRuntimeSettings,
  writeAgentSettings,
  writeConfigSetting,
} from "$lib/services/api";
import type { Account, AgentSettings, CodexServerInfo, ConfigSetting, HomeOverview, RuntimeSettings } from "$lib/types";
import { codexVersionFromUserAgent } from "$lib/types";
import { dragRegion } from "$lib/utils/dragRegion";

let {
  account = null,
  codexHome = null,
  codexBinary = null,
  initialSection = null,
  focusServer = null,
  focusTool = null,
  navNonce = 0,
  onClose,
}: {
  account?: Account | null;
  codexHome?: string | null;
  codexBinary?: string | null;
  /** Section requested via the shared settingsNav channel (e.g. from an MCP tool call). */
  initialSection?: string | null;
  focusServer?: string | null;
  /** Tool within `focusServer` to scroll to, when the caller knows which one. */
  focusTool?: string | null;
  navNonce?: number;
  onClose: () => void;
} = $props();

// Lucide icon per section id.
const sectionIcons: Record<string, typeof Settings2> = {
  general: Settings2,
  appearance: Palette,
  agent: Bot,
  modelFeatures: Sliders,
  coding: Code2,
  integrations: Blocks,
  connections: Smartphone,
  keyboard: Keyboard,
  data: Database,
  advanced: FlaskConical,
};

let query = $state("");
let activeSection = $state("general");
// Jump to the requested section whenever a settingsNav request fires.
$effect(() => {
  void navNonce;
  if (initialSection) activeSection = initialSection;
});
const filtered = $derived(filterSections(SETTINGS_SECTIONS, query));
// Keep the active section valid as the filter narrows the list.
const currentSection = $derived(
  filtered.find((section) => section.id === activeSection)?.id ?? filtered[0]?.id ?? null,
);

// --- General (runtime overrides) ---
let runtimeSettings = $state<RuntimeSettings | null>(null);
let settingsHome = $state("");
let settingsBinary = $state("");
let generalError = $state<string | null>(null);
let generalSaved = $state(false);
let serverInfo = $state<CodexServerInfo | null>(null);
const serverVersion = $derived(codexVersionFromUserAgent(serverInfo?.userAgent));

// --- config.toml settings (Agent / Model features / Coding) ---
let configSettings = $state<ConfigSetting[]>([]);
let configError = $state<string | null>(null);
let agentError = $state<string | null>(null);
let agentSettings = $state<AgentSettings>({
  enabled: false,
  sandbox: "workspace-write",
  maxConcurrent: 4,
  timeoutSeconds: 900,
  sandboxOptions: ["read-only", "workspace-write"],
});

// --- Integrations (read-only) ---
let overview = $state<HomeOverview | null>(null);

// Quick-chat global shortcut recorder.
let quickShortcut = $state<string>(DEFAULT_QUICK_SHORTCUT);
let recording = $state(false);
let shortcutError = $state<string | null>(null);

async function applyShortcut(accelerator: string) {
  const previous = quickShortcut;
  shortcutError = null;
  try {
    quickShortcut = await setQuickShortcut(accelerator);
  } catch (cause) {
    quickShortcut = previous;
    shortcutError = cause instanceof Error ? cause.message : String(cause);
  }
}

function recordShortcut(event: KeyboardEvent) {
  if (!recording) return;
  event.preventDefault();
  if (event.key === "Escape") {
    recording = false;
    return;
  }
  const accelerator = acceleratorFromEvent(event);
  if (!accelerator) return; // still holding modifiers; wait for the final key
  recording = false;
  applyShortcut(accelerator);
}

async function resetShortcut() {
  recording = false;
  await applyShortcut(DEFAULT_QUICK_SHORTCUT);
}

const KEYBOARD_SHORTCUTS = [
  { keys: "⌘ ↵", action: "Send message" },
  { keys: "⇧ ↵", action: "New line in composer" },
  { keys: "⌘ K", action: "Open file mention search" },
  { keys: "/", action: "Open slash command menu" },
  { keys: "Esc", action: "Interrupt the running turn" },
];

function settingsForSection(section: string): ConfigSetting[] {
  return configSettings.filter((setting) => setting.section === section);
}

$effect(() => {
  generalError = null;
  readRuntimeSettings()
    .then((settings) => {
      runtimeSettings = settings;
      settingsHome = settings.overrideCodexHome ?? "";
      settingsBinary = settings.overrideCodexBinary ?? "";
    })
    .catch((cause) => (generalError = cause instanceof Error ? cause.message : String(cause)));
  // Best effort: a Codex that will not start is already reported elsewhere.
  readCodexServerInfo()
    .then((info) => (serverInfo = info))
    .catch(() => (serverInfo = null));
  readConfigSettings()
    .then((settings) => (configSettings = settings))
    .catch((cause) => (configError = cause instanceof Error ? cause.message : String(cause)));
  readAgentSettings()
    .then((settings) => (agentSettings = settings))
    .catch((cause) => (agentError = cause instanceof Error ? cause.message : String(cause)));
  readHomeOverview()
    .then((data) => (overview = data))
    .catch(() => (overview = null));
  recording = false;
  shortcutError = null;
  getQuickShortcut()
    .then((accelerator) => (quickShortcut = accelerator))
    .catch(() => (quickShortcut = DEFAULT_QUICK_SHORTCUT));
});

// Opening Advanced pulls the buffered traffic captured before this view
// mounted; live messages arrive on the event stream from there.
$effect(() => {
  if (currentSection === "advanced" && messageLog.enabled) void messageLog.refresh();
});

async function saveGeneral(event: SubmitEvent) {
  event.preventDefault();
  generalError = null;
  generalSaved = false;
  try {
    runtimeSettings = await updateRuntimeSettings(settingsHome.trim() || null, settingsBinary.trim() || null);
    generalSaved = true;
  } catch (cause) {
    generalError = cause instanceof Error ? cause.message : String(cause);
  }
}

/** Save one changed field, keeping the rest of the agent settings as they are. */
async function saveAgents(patch: Partial<AgentSettings>) {
  agentError = null;
  const next = { ...agentSettings, ...patch };
  // Show the new value straight away; a rejected save puts the stored one back.
  agentSettings = next;
  try {
    agentSettings = await writeAgentSettings(next);
  } catch (cause) {
    agentError = cause instanceof Error ? cause.message : String(cause);
    agentSettings = await readAgentSettings().catch(() => next);
  }
}

async function setConfig(setting: ConfigSetting, value: string | null, unset: boolean) {
  configError = null;
  try {
    configSettings = await writeConfigSetting(setting.key, value, unset);
  } catch (cause) {
    configError = cause instanceof Error ? cause.message : String(cause);
  }
}

async function reveal(path: string | null | undefined) {
  if (!path) return;
  try {
    await revealInFinder(path);
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
    if (event.defaultPrevented || recording) return;
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
          {@const Icon = sectionIcons[section.id] ?? Settings2}
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
          <section class="card border border-surface-200-800 bg-surface-50-950 p-5">
            <h2 class="text-sm font-semibold">Runtime</h2>
            <p class="mt-1 text-xs text-surface-600-400">Which Codex home and binary this app talks to.</p>
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
                </div>
              </div>
            {/if}
            <form onsubmit={saveGeneral} class="mt-4 space-y-4 text-sm">
              <div>
                <div class="flex items-center gap-2">
                  <label for="settings-codex-home" class="text-xs font-medium text-surface-500">CODEX_HOME</label>
                  {@render restartBadge()}
                </div>
                <input
                  id="settings-codex-home"
                  bind:value={settingsHome}
                  placeholder={runtimeSettings?.codexHome ?? codexHome ?? "~/.codex"}
                  class="input mt-1 w-full font-mono text-xs"
                />
              </div>
              <div>
                <div class="flex items-center gap-2">
                  <label for="settings-codex-binary" class="text-xs font-medium text-surface-500">Codex binary</label>
                  {@render restartBadge()}
                </div>
                <input
                  id="settings-codex-binary"
                  bind:value={settingsBinary}
                  placeholder={runtimeSettings?.codexBinary ?? codexBinary ?? "codex"}
                  class="input mt-1 w-full font-mono text-xs"
                />
              </div>
              <div class="flex items-center gap-3">
                <button type="submit" class="btn btn-sm preset-filled-primary-500">Save</button>
                {#if generalSaved}
                  <span class="text-xs {runtimeSettings?.restartRequired ? 'text-warning-500' : 'text-success-500'}">
                    {runtimeSettings?.restartRequired ? "Saved — restart Pingex to apply." : "Saved."}
                  </span>
                {/if}
              </div>
              <p class="text-[11px] leading-4 text-surface-500">
                Leave a field blank to use the CLI/environment value; a binary that cannot be run is
                rejected on save. Overrides are stored in
                <span class="font-mono">{runtimeSettings?.settingsPath ?? "settings.json"}</span>.
              </p>
              {#if generalError}
                <div class="card preset-tonal-error px-3 py-2 text-xs">{generalError}</div>
              {/if}
            </form>
          </section>
        {:else if currentSection === "appearance"}
          <section class="card border border-surface-200-800 bg-surface-50-950 p-5">
            <h2 class="text-sm font-semibold">Appearance</h2>
            <p class="mt-1 text-xs text-surface-600-400">Local to this app; not stored in Codex config.</p>
            <div class="mt-4 space-y-5 text-sm">
              <div class="flex items-center justify-between gap-4">
                <div>
                  <div class="font-medium">Density</div>
                  <div class="text-xs text-surface-500">Spacing of lists and cards.</div>
                </div>
                <div class="flex items-center gap-2">
                  {@render localBadge()}
                  <select
                    aria-label="Density"
                    class="select text-sm"
                    value={appearance.prefs.density}
                    onchange={(event) => appearance.set({ density: event.currentTarget.value as "comfortable" | "compact" })}
                  >
                    <option value="comfortable">Comfortable</option>
                    <option value="compact">Compact</option>
                  </select>
                </div>
              </div>
              <div class="flex items-center justify-between gap-4">
                <div>
                  <div class="font-medium">Font size</div>
                  <div class="text-xs text-surface-500">Base text size ({appearance.prefs.fontSize}px).</div>
                </div>
                <div class="flex items-center gap-2">
                  {@render localBadge()}
                  <input
                    type="range"
                    aria-label="Font size"
                    min={FONT_SIZE_MIN}
                    max={FONT_SIZE_MAX}
                    value={appearance.prefs.fontSize}
                    oninput={(event) => appearance.set({ fontSize: Number(event.currentTarget.value) })}
                  />
                </div>
              </div>
            </div>
          </section>
        {:else if currentSection === "agent" || currentSection === "modelFeatures" || currentSection === "coding"}
          {#if currentSection === "agent"}
            <section class="card mb-4 border border-surface-200-800 bg-surface-50-950 p-5">
              <h2 class="text-sm font-semibold">Pingex agents</h2>
              <p class="mt-1 text-xs text-surface-600-400">
                Stored by Pingex, not <span class="font-mono">config.toml</span>. Applies to new threads.
              </p>
              {#if agentError}
                <div class="card preset-tonal-error mt-3 px-3 py-2 text-xs">{agentError}</div>
              {/if}
              <div class="mt-4 divide-y divide-surface-200-800">
                <div class="flex items-center justify-between gap-3 py-3">
                  <div class="min-w-0">
                    <div class="text-sm">Run subagents as separate processes</div>
                    <p class="text-[11px] leading-4 text-surface-500">
                      Gives the agent Pingex's own spawn tools, so each subagent runs in its own Codex
                      process that you can watch and stop. New threads only.
                    </p>
                  </div>
                  <input
                    type="checkbox"
                    class="checkbox shrink-0"
                    checked={agentSettings.enabled}
                    onchange={(event) => saveAgents({ enabled: event.currentTarget.checked })}
                  />
                </div>
                <div class="flex items-center justify-between gap-3 py-3">
                  <div class="min-w-0">
                    <div class="text-sm">Sandbox limit</div>
                    <p class="text-[11px] leading-4 text-surface-500">
                      The most a spawned agent may do. Nobody is watching to approve its commands, so
                      full access is not offered.
                    </p>
                  </div>
                  <select
                    class="select w-48 shrink-0"
                    value={agentSettings.sandbox}
                    disabled={!agentSettings.enabled}
                    onchange={(event) => saveAgents({ sandbox: event.currentTarget.value })}
                  >
                    {#each agentSettings.sandboxOptions as option (option)}
                      <option value={option}>{option}</option>
                    {/each}
                  </select>
                </div>
                <div class="flex items-center justify-between gap-3 py-3">
                  <div class="min-w-0">
                    <div class="text-sm">Maximum at once</div>
                    <p class="text-[11px] leading-4 text-surface-500">
                      How many agents may run concurrently. Each is a separate process.
                    </p>
                  </div>
                  <input
                    type="number"
                    min="1"
                    max="16"
                    class="input w-24 shrink-0"
                    value={agentSettings.maxConcurrent}
                    disabled={!agentSettings.enabled}
                    onchange={(event) => saveAgents({ maxConcurrent: Number(event.currentTarget.value) })}
                  />
                </div>
                <div class="flex items-center justify-between gap-3 py-3">
                  <div class="min-w-0">
                    <div class="text-sm">Time limit</div>
                    <p class="text-[11px] leading-4 text-surface-500">
                      Seconds before an agent is stopped, so a stuck one cannot hold a slot forever.
                    </p>
                  </div>
                  <input
                    type="number"
                    min="30"
                    max="7200"
                    step="30"
                    class="input w-24 shrink-0"
                    value={agentSettings.timeoutSeconds}
                    disabled={!agentSettings.enabled}
                    onchange={(event) => saveAgents({ timeoutSeconds: Number(event.currentTarget.value) })}
                  />
                </div>
              </div>
            </section>
          {/if}
          <section class="card border border-surface-200-800 bg-surface-50-950 p-5">
            <h2 class="text-sm font-semibold">{filtered.find((section) => section.id === currentSection)?.label}</h2>
            <p class="mt-1 text-xs text-surface-600-400">
              Stored in <span class="font-mono">config.toml</span>. Changes apply to the next thread.
            </p>
            {#if configError}
              <div class="card preset-tonal-error mt-3 px-3 py-2 text-xs">{configError}</div>
            {/if}
            <div class="mt-4 divide-y divide-surface-200-800">
              {#each settingsForSection(currentSection) as setting (setting.key)}
                {@render configControl(setting)}
              {:else}
                <p class="py-3 text-xs text-surface-500">No editable settings in this section.</p>
              {/each}
            </div>
          </section>
        {:else if currentSection === "integrations"}
          <IntegrationsSection
            {focusServer}
            {focusTool}
            nonce={navNonce}
            onGoToConnections={() => (activeSection = "connections")}
          />
        {:else if currentSection === "connections"}
          <Connections active={currentSection === "connections"} />
        {:else if currentSection === "keyboard"}
          <section class="card border border-surface-200-800 bg-surface-50-950 p-5">
            <h2 class="text-sm font-semibold">Keyboard shortcuts</h2>
            <div class="mt-3 flex items-center justify-between gap-3">
              <div class="min-w-0">
                <div class="text-sm">Quick chat</div>
                <p class="text-[11px] leading-4 text-surface-500">Open the floating composer from anywhere.</p>
              </div>
              <div class="flex shrink-0 items-center gap-2">
                <button
                  type="button"
                  onclick={() => {
                    recording = !recording;
                    shortcutError = null;
                  }}
                  onkeydown={recordShortcut}
                  class="input min-w-[150px] rounded-md px-3 py-1.5 text-center font-mono text-xs {recording
                    ? 'border-primary-500 text-primary-500'
                    : ''}"
                >
                  {recording ? "Press keys…" : quickShortcut}
                </button>
                <TooltipButton
                  label="Reset to default"
                  type="button"
                  onclick={resetShortcut}
                  aria-label="Reset quick chat shortcut to default"
                  class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500"
                >
                  <RotateCcw size={14} />
                </TooltipButton>
              </div>
            </div>
            {#if shortcutError}
              <div class="card preset-tonal-error mt-3 px-3 py-2 text-xs">{shortcutError}</div>
            {/if}
            <div class="mt-4 divide-y divide-surface-200-800 text-sm">
              {#each KEYBOARD_SHORTCUTS as shortcut (shortcut.action)}
                <div class="flex items-center justify-between py-2">
                  <span>{shortcut.action}</span>
                  <kbd class="kbd text-xs">{shortcut.keys}</kbd>
                </div>
              {/each}
            </div>
          </section>
        {:else if currentSection === "data"}
          <section class="card border border-surface-200-800 bg-surface-50-950 p-5">
            <h2 class="text-sm font-semibold">Data controls</h2>
            <p class="mt-1 text-xs text-surface-600-400">Local metadata and storage locations.</p>
            <div class="mt-4 space-y-3 text-sm">
              <div class="flex items-center justify-between gap-3">
                <div>
                  <div class="font-medium">Codex home &amp; database</div>
                  <div class="truncate font-mono text-[11px] text-surface-500">
                    {runtimeSettings?.codexHome ?? codexHome ?? "~/.codex"}
                  </div>
                </div>
                <button
                  type="button"
                  onclick={() => reveal(runtimeSettings?.codexHome ?? codexHome)}
                  class="btn btn-sm preset-tonal shrink-0"
                >
                  Reveal
                </button>
              </div>
              <p class="text-[11px] leading-4 text-surface-500">
                Per-project message drafts and the frontend database live under this folder. Removing a project from the
                sidebar clears its draft.
              </p>
            </div>
          </section>
        {:else if currentSection === "advanced"}
          <section class="card border border-surface-200-800 bg-surface-50-950 p-5">
            <h2 class="text-sm font-semibold">Advanced</h2>
            <p class="mt-1 text-xs text-surface-600-400">Local to this app; not stored in Codex config.</p>
            <div class="mt-4 flex items-start justify-between gap-4 text-sm">
              <div class="min-w-0">
                <div class="font-medium">Message log</div>
                <p class="mt-0.5 text-[11px] leading-4 text-surface-500">
                  Record the messages exchanged with the agent. Off by default; the log is kept in memory only and is
                  discarded when you switch it off or quit. Once enabled, open it from a thread's overview menu to
                  view it alongside the conversation.
                </p>
              </div>
              <div class="flex shrink-0 items-center gap-2">
                {@render localBadge()}
                <button
                  type="button"
                  role="switch"
                  aria-checked={messageLog.enabled}
                  aria-label="Message log"
                  data-testid="message-log-switch"
                  onclick={() => messageLog.setEnabled(!messageLog.enabled)}
                  class="relative h-5 w-9 shrink-0 rounded-full transition {messageLog.enabled
                    ? 'bg-primary-500'
                    : 'bg-surface-300-700'}"
                >
                  <span
                    class="absolute top-0.5 size-4 rounded-full bg-white transition-all {messageLog.enabled
                      ? 'left-[1.125rem]'
                      : 'left-0.5'}"
                  ></span>
                </button>
              </div>
            </div>
          </section>
        {/if}
      </div>
    </div>
  </div>
</div>

{#snippet restartBadge()}
  <span class="chip preset-tonal-warning text-[10px]">Restart required</span>
{/snippet}

{#snippet localBadge()}
  <span class="chip preset-tonal text-[10px] text-surface-500">Local</span>
{/snippet}

{#snippet sourceBadge(setting: ConfigSetting)}
  {#if setting.source === "config"}
    <span class="chip preset-tonal-primary text-[10px]">config.toml</span>
  {:else}
    <span class="chip preset-tonal text-[10px] text-surface-500">Default</span>
  {/if}
{/snippet}

{#snippet configControl(setting: ConfigSetting)}
  <div class="flex items-center justify-between gap-4 py-3" data-testid="config-control" data-key={setting.key}>
    <div class="min-w-0">
      <div class="flex items-center gap-2">
        <span class="text-sm font-medium">{setting.label}</span>
        {@render sourceBadge(setting)}
        {#if setting.restartRequired}{@render restartBadge()}{/if}
      </div>
      <div class="mt-0.5 text-[11px] text-surface-500">
        {#if setting.source === "default"}
          Inherited default{setting.value ? `: ${setting.value}` : ""}
        {:else}
          Set in config.toml{setting.default ? ` · default ${setting.default}` : ""}
        {/if}
      </div>
    </div>
    <div class="flex shrink-0 items-center gap-2">
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
    </div>
  </div>
{/snippet}
