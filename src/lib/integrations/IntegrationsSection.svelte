<script lang="ts">
import { Plus, Puzzle, RefreshCw } from "@lucide/svelte";
import { tick } from "svelte";
import { openDialog } from "$lib/app/dialogs.svelte";
import Segmented from "$lib/components/Segmented.svelte";
import AddSkillDialog from "$lib/integrations/AddSkillDialog.svelte";
import {
  type IntegrationFilter,
  type OverrideValue,
  overrideEnabled,
  overrideValue,
  retainFailedInventory,
} from "$lib/integrations/integrationsHelpers";
import McpServerDialog from "$lib/integrations/McpServerDialog.svelte";
import McpServerRow from "$lib/integrations/McpServerRow.svelte";
import OverrideSelect from "$lib/integrations/OverrideSelect.svelte";
import SkillRow from "$lib/integrations/SkillRow.svelte";
import {
  createSkill,
  listMcpServerStatus,
  type McpServerInput,
  type NewSkillInput,
  saveMcpServer,
} from "$lib/services/api";
import { mcpStatus as mcpStatusEvents, skillsStatus } from "$lib/services/codexEvents.svelte";
import { commands } from "$lib/services/homeRouting";
import type { IntegrationSetting, IntegrationsList, McpServerStatus, McpServerSummary, SkillSummary } from "$lib/types";
import { submitState } from "$lib/utils/actionState.svelte";
import { type Resource, resource } from "$lib/utils/resource.svelte";

let {
  projectPath = null,
  focusServer = null,
  focusTool = null,
  nonce = 0,
  onGoToConnections,
}: {
  projectPath?: string | null;
  focusServer?: string | null;
  /** Tool within `focusServer` to expand and scroll to (from a tool call). */
  focusTool?: string | null;
  nonce?: number;
  onGoToConnections?: () => void;
} = $props();

// Only the newest read lands, and a mutation's own result (`apply`)
// supersedes any read still in flight.
const inventory: Resource<IntegrationsList | undefined, [forceReload?: boolean]> = resource(
  async (forceReload = false) => {
    const next = await commands.listIntegrations(projectPath ? [projectPath] : [], forceReload);
    return retainFailedInventory(inventory.value ?? null, next);
  },
);
const data = $derived(inventory.value ?? null);
const loadError = $derived(inventory.error);
const loading = $derived(inventory.loading);
/** Mutations from the rows; they may overlap, and share one error banner. */
const actions = submitState({ exclusive: false });
let actionNotice = $state<string | null>(null);
let filter = $state<IntegrationFilter>("all");
let query = $state("");
const updating = $derived(actions.busy);

/**
 * Live state from Codex, keyed by server name and joined onto `data.mcpServers`
 * (which is what `config.toml` declares). A server can appear in one and not
 * the other: configured but not started, or provided by a plugin and never
 * written to `config.toml` at all.
 */
const liveStatus = resource(() => listMcpServerStatus(), { initial: {} as Record<string, McpServerStatus> });
const statuses = $derived(liveStatus.value);
const checking = $derived(liveStatus.loading);
const actionError = $derived(actions.error ?? liveStatus.error);
/** Servers whose tool list is expanded. */
let expanded = $state<Record<string, boolean>>({});
/** Servers with an OAuth sign-in in flight, cleared by the completion event. */
let signingIn = $state<Record<string, boolean>>({});

const load = (forceReload = false) => inventory.reload(forceReload);

let observedSkillsNonce = skillsStatus.nonce;
$effect(() => {
  const nonce = skillsStatus.nonce;
  if (nonce === observedSkillsNonce) return;
  observedSkillsNonce = nonce;
  void load();
});

async function refresh() {
  await Promise.all([load(true), refreshStatus()]);
}

/**
 * Refresh Codex's live view. Servers start asynchronously, so this is also what
 * the "Refresh" button and the startup notification both trigger — there is no
 * one moment when the picture is final.
 */
async function refreshStatus() {
  // This API returns the home runtime, not a runtime for an arbitrary project.
  if (projectPath) {
    liveStatus.set({});
    return;
  }
  // Every startup or login event asks again; only the newest answer may land,
  // or a slow older one leaves a running server on "Checking…".
  await liveStatus.reload();
}

// Reload whenever the section is (re-)opened, and honour a focus request.
$effect(() => {
  // Reference nonce so re-opening the same section re-runs this effect.
  void nonce;
  void projectPath;
  const server = focusServer;
  const tool = focusTool;
  // The tool list has to exist before it can be scrolled to, so wait for the
  // status call rather than just the config read.
  Promise.all([load(), refreshStatus()]).then(() => {
    if (!server) return;
    filter = "mcp";
    expanded[server] = true;
    // The Collapsible opens on the next update; scroll once its content is in.
    void tick().then(() => {
      const target = tool ? document.getElementById(`mcp-tool-${server}-${tool}`) : null;
      (target ?? document.getElementById(`mcp-row-${server}`))?.scrollIntoView({ block: "nearest" });
    });
  });
});

// Codex reports servers finishing startup and OAuth logins completing; both
// change what this view should show, and neither is something we initiated.
$effect(() => {
  if (mcpStatusEvents.nonce === 0) return;
  const server = mcpStatusEvents.lastLoginServer;
  if (server) delete signingIn[server];
  void refreshStatus();
});

function apply(next: IntegrationsList) {
  inventory.set(retainFailedInventory(data, next));
  actionNotice = next.errors.find((message) => message.startsWith("Saved.")) ?? null;
  // This view already has the refreshed mutation result. Notify other consumers.
  observedSkillsNonce = ++skillsStatus.nonce;
}

async function changeSetting(kind: "mcp" | "plugin", id: string, value: OverrideValue) {
  const context = projectPath;
  await actions.run(async () => {
    const next = await commands.setIntegrationEnabled(kind, id, overrideEnabled(value), context);
    if (projectPath !== context) return;
    apply(next);
    await refreshStatus();
  });
}

/** The stored settings of one integration, keyed `<kind>:<id>`. */
const setting = (kind: "mcp" | "skill" | "plugin", id: string) => data?.settings[`${kind}:${id}`];

/** False when the plugin that provides an integration is turned off. */
const ownerEnabled = (owned: IntegrationSetting | undefined) =>
  !data?.plugins.some((plugin) => plugin.id === owned?.pluginId && !plugin.enabled);

function matches(...values: (string | null | undefined)[]): boolean {
  return values.filter(Boolean).join(" ").toLowerCase().includes(query.trim().toLowerCase());
}

function liveFor(server: McpServerSummary): McpServerStatus | undefined {
  const owner = setting("mcp", server.name)?.pluginId;
  if (!owner) return statuses[server.name];
  // Plugin integration IDs include the package ID; runtime MCP names do not.
  // A different plugin can win the runtime name collision.
  const live = statuses[server.name.slice(owner.length + 1)];
  return live?.pluginId === owner ? live : undefined;
}

async function toggleEnabled(server: McpServerSummary) {
  if (server.scope === "inherited" || server.scope === "plugin") {
    await changeSetting("mcp", server.name, server.enabled ? "disabled" : "enabled");
    return;
  }
  await actions.run(async () => {
    apply(await commands.setMcpEnabled(server.name, !server.enabled));
    // The backend reloaded Codex's config; pick up the resulting state.
    await refreshStatus();
  });
}

/**
 * Start an OAuth sign-in. Codex opens the browser and drives the whole flow;
 * we stay pending until it reports back, since there is nothing to poll.
 */
async function signIn(server: McpServerSummary) {
  signingIn[server.name] = true;
  const started = await actions.run(async () => {
    await commands.mcpOauthLogin(server.name);
    // In browser preview there is no completion event, so reflect it now.
    await refreshStatus();
  });
  if (!started) delete signingIn[server.name];
}

async function toggleSkill(skill: SkillSummary) {
  await actions.run(async () => {
    await commands.setSkillEnabled(skill.name, !skill.enabled, skill.path);
    skillsStatus.nonce += 1;
    // The notification reloads Codex's effective state in both views.
  });
}

async function deleteSkill(skill: SkillSummary) {
  await actions.run(async () => apply(await commands.deleteSkill(skill.path, null)));
}

async function addSkill() {
  const name = await openDialog(AddSkillDialog, {
    submit: async (input: NewSkillInput) => apply(await createSkill(input)),
  });
  if (!name) return;
  filter = filter === "all" ? "all" : "skills";
  // Rows are keyed by path, not name.
  const created = data?.skills.find((row) => row.name === name);
  if (created)
    queueMicrotask(() =>
      document.getElementById(`skill-row-${encodeURIComponent(created.path)}`)?.scrollIntoView({ block: "nearest" }),
    );
}

/** Add a server, or edit `server`. */
function editServer(server: McpServerSummary | null = null) {
  void openDialog(McpServerDialog, {
    server,
    submit: async (input: McpServerInput) => {
      apply(await saveMcpServer(input));
      await refreshStatus();
    },
  });
}

async function removeServer(name: string) {
  await actions.run(async () => {
    apply(await commands.removeMcpServer(name));
    delete expanded[name];
    await refreshStatus();
  });
}

const FILTERS: { id: IntegrationFilter; label: string }[] = [
  { id: "all", label: "All" },
  { id: "mcp", label: "MCP" },
  { id: "skills", label: "Skills" },
  { id: "plugins", label: "Plugins" },
  { id: "connections", label: "Connections" },
];

const showMcp = $derived(filter === "all" || filter === "mcp");
const showSkills = $derived(filter === "all" || filter === "skills");
const showPlugins = $derived(filter === "all" || filter === "plugins");
const servers = $derived(
  (data?.mcpServers ?? []).filter((row) =>
    matches(
      row.name,
      liveFor(row)?.serverInfo?.title,
      liveFor(row)?.serverInfo?.description,
      setting("mcp", row.name)?.pluginId,
    ),
  ),
);
const skills = $derived(
  (data?.skills ?? []).filter((row) =>
    matches(row.name, row.displayName, row.description, row.shortDescription, setting("skill", row.path)?.pluginId),
  ),
);
const plugins = $derived((data?.plugins ?? []).filter((row) => matches(row.id, row.name, row.description)));
const matchCount = $derived(
  (showMcp ? servers.length : 0) + (showSkills ? skills.length : 0) + (showPlugins ? plugins.length : 0),
);
</script>

<div class="text-sm">
  <div class="flex items-center justify-between gap-2">
    <div>
      <div class="text-base font-semibold">Integrations</div>
      <div class="mt-1 text-xs text-surface-600-400">{projectPath ? "Integration settings for this project. Inherit follows the parent configuration." : "MCP servers, skills, and plugins for this Codex home."}</div>
    </div>
    <div class="flex shrink-0 items-center gap-1.5">
      <button type="button" onclick={refresh} disabled={checking || loading || updating} class="btn btn-sm hover:preset-tonal text-xs">
        <RefreshCw size={13} class={checking ? "animate-spin" : ""} /> Refresh
      </button>
      {#if !projectPath}
      <button type="button" onclick={addSkill} class="btn btn-sm hover:preset-tonal text-xs">
        <Plus size={13} /> Add skill
      </button>
      <button type="button" onclick={() => editServer()} class="btn btn-sm preset-tonal">
        <Plus size={14} /> Add MCP server
      </button>
      {/if}
    </div>
  </div>

  <div class="mt-4 flex items-center gap-2">
    <input type="search" aria-label="Search integrations" placeholder="Search integrations…" class="input w-full" bind:value={query} />
    {#if query}<button type="button" class="btn btn-sm" onclick={() => (query = "")}>Clear search</button>{/if}
  </div>
  <Segmented
    label="Integration type"
    value={filter}
    onChange={(next) => (filter = next)}
    class="mt-4"
    options={FILTERS.filter((entry) => !projectPath || entry.id !== "connections").map((entry) => ({
      value: entry.id,
      label: entry.label,
    }))}
  />

  {#if loadError}
    <div class="error-note mt-4">{loadError}</div>
  {/if}
  {#if actionError}
    <div class="error-note mt-4">{actionError}</div>
  {/if}
  {#if actionNotice}<p class="mt-3 text-xs text-surface-500">{actionNotice}</p>{/if}
  {#each data?.errors ?? [] as error}
    <div class="card preset-tonal-warning mt-4 px-3 py-2 text-xs">{error}</div>
  {/each}
  {#if data && query.trim() && matchCount === 0 && filter !== "connections"}
    <p class="mt-4 text-xs text-surface-500">No matching integrations.</p>
  {/if}

  {#if !data && !loadError}
    <div class="mt-6 text-xs text-surface-500">Loading integrations…</div>
  {:else if data}
    <div class="mt-4 space-y-2">
      {#if showMcp}
        {#if data.mcpServers.length === 0 && !query.trim()}
          <div class="text-xs text-surface-500">No MCP servers configured.</div>
        {/if}
        {#each servers as server (server.name)}
          {@const owned = setting("mcp", server.name)}
          <McpServerRow
            {server}
            live={liveFor(server)}
            setting={owned}
            ownerEnabled={ownerEnabled(owned)}
            {projectPath}
            {checking}
            busy={updating}
            open={Boolean(expanded[server.name])}
            signingIn={Boolean(signingIn[server.name])}
            onOpenChange={(open) => (expanded[server.name] = open)}
            onToggle={() => toggleEnabled(server)}
            onEdit={() => editServer(server)}
            onRemove={() => removeServer(server.name)}
            onSignIn={() => signIn(server)}
            onSetting={(value) => changeSetting("mcp", server.name, value)}
          />
        {/each}
      {/if}

      {#if showSkills}
        {#if projectPath}
          <p class="text-xs text-surface-500">Codex does not apply project skill enablement settings. Manage skill enablement in Settings.</p>
        {/if}
        {#if filter === "skills" && data.skills.length === 0 && !query.trim()}
          <div class="flex items-center gap-2 text-xs text-surface-500">
            <span>Codex reported no skills for this home.</span>
            {#if !projectPath}<button type="button" onclick={addSkill} class="btn btn-sm preset-tonal text-xs"><Plus size={12} /> Add skill</button>{/if}
          </div>
        {/if}
        {#each skills as skill (skill.path)}
          {@const owned = setting("skill", skill.path)}
          <SkillRow
            {skill}
            setting={owned}
            ownerEnabled={ownerEnabled(owned)}
            {projectPath}
            onToggle={() => toggleSkill(skill)}
            onDelete={() => deleteSkill(skill)}
            onError={(message) => (actions.error = message)}
          />
        {/each}
      {/if}

      {#if showPlugins}
        {#each plugins as plugin (plugin.id)}
          <div class="card border border-surface-200-800 p-3">
            <div class="flex items-center gap-2"><Puzzle size={16} /><span class="font-medium">{plugin.name}</span><span class="text-xs text-surface-500">{plugin.enabled ? "Enabled" : "Disabled"}</span></div>
            <p class="mt-1 text-xs text-surface-500">{plugin.id}</p>
            {#if plugin.description}<p class="mt-1 text-xs">{plugin.description}</p>{/if}
            {#if projectPath}
              <OverrideSelect
                label="Setting for {plugin.id}"
                value={overrideValue(setting("plugin", plugin.id))}
                inheritLabel="Inherit ({setting('plugin', plugin.id)?.inheritedEnabled ? 'enabled' : 'disabled'})"
                disabled={updating}
                class="mt-2"
                onChange={(value) => changeSetting("plugin", plugin.id, value)}
              />
            {:else}
              <button type="button" class="btn btn-sm mt-2" disabled={updating} onclick={() => changeSetting("plugin", plugin.id, plugin.enabled ? "disabled" : "enabled")}>{plugin.enabled ? "Disable" : "Enable"}</button>
            {/if}
          </div>
        {/each}
        {#if !data.pluginsSupported}
        <div class="card flex items-center gap-3 border border-dashed border-surface-200-800 p-3 text-xs text-surface-500">
          <Puzzle size={16} class="shrink-0" />
          <span>Plugins are not yet supported by this Codex build.</span>
        </div>
        {:else if data.plugins.length === 0 && !query.trim()}
          <p class="text-xs text-surface-500">No installed plugins.</p>
        {/if}
      {/if}

      {#if filter === "connections"}
        <div class="card border border-surface-200-800 bg-surface-50-950 p-3 text-xs text-surface-600-400">
          <p>Account and device connections are managed in the General section.</p>
          {#if onGoToConnections}
            <button type="button" onclick={onGoToConnections} class="btn btn-sm preset-tonal mt-2 text-xs">Go to General</button>
          {/if}
        </div>
      {/if}
    </div>
  {/if}
</div>
