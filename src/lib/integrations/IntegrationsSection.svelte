<script lang="ts">
import {
  ChevronRight,
  ExternalLink,
  FileText,
  FolderOpen,
  LogIn,
  Plug,
  Plus,
  Puzzle,
  RefreshCw,
  Sparkles,
  Trash2,
  X,
} from "@lucide/svelte";
import { Collapsible } from "@skeletonlabs/skeleton-svelte";
import { tick } from "svelte";
import type { HTMLInputAttributes } from "svelte/elements";
import Segmented, { type SegmentedOption } from "$lib/components/Segmented.svelte";
import TooltipButton from "$lib/components/TooltipButton.svelte";
import {
  authAction,
  capabilitySummary,
  contributionSummary,
  envKeysValid,
  formatArgs,
  type IntegrationFilter,
  parseArgs,
  resourcesOf,
  resourceTemplatesOf,
  retainFailedInventory,
  rowStatus,
  runtimeStatusLabel,
  serverInfoLines,
  splitFrontmatter,
  statusDotClass,
  statusLabel,
  toolParameters,
  toolsOf,
  validSkillName,
} from "$lib/integrations/integrationsHelpers";
import {
  createSkill,
  deleteSkill,
  listIntegrations,
  listMcpServerStatus,
  mcpOauthLogin,
  openExternalUrl,
  openInZed,
  readSkill,
  removeMcpServer,
  revealInFinder,
  saveMcpServer,
  setIntegrationEnabled,
  setMcpEnabled,
  setSkillEnabled,
} from "$lib/services/api";
import { mcpStatus as mcpStatusEvents, skillsStatus } from "$lib/services/codexEvents.svelte";
import type { IntegrationsList, McpServerStatus, McpServerSummary, SkillSummary } from "$lib/types";
import { submitState } from "$lib/utils/actionState.svelte";
import { errorMessage } from "$lib/utils/errors";
import { renderMarkdown } from "$lib/utils/markdown";
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
    const next = await listIntegrations(projectPath ? [projectPath] : [], forceReload);
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

/** Skills whose SKILL.md is shown, and the text once loaded (null while loading). */
let skillOpen = $state<Record<string, boolean>>({});
let skillText = $state<Record<string, string | null>>({});
let confirmDeleteSkill = $state<string | null>(null);

// Add-skill form.
let addingSkill = $state(false);
let skillFormName = $state("");
let skillFormDescription = $state("");
let skillFormBody = $state("");
const skillForm = submitState();

// The server whose edit form is open ("" means the Add form; null means none).
let editing = $state<string | null>(null);
let confirmRemove = $state<string | null>(null);

// Edit-form fields.
let formName = $state("");
/** `"stdio"` (command + args + env) or `"http"` (url + bearer token env var). */
let formTransport = $state<"stdio" | "http">("stdio");
let formCommand = $state("");
let formArgs = $state("");
let formEnv = $state<{ key: string; value: string }[]>([]);
let formUrl = $state("");
let formBearerEnv = $state("");
const serverForm = submitState();

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

async function changeSetting(kind: "mcp" | "plugin", id: string, value: string) {
  const context = projectPath;
  await actions.run(async () => {
    const next = await setIntegrationEnabled(kind, id, value === "inherit" ? null : value === "enabled", context);
    if (projectPath !== context) return;
    apply(next);
    await refreshStatus();
  });
}

function settingValue(key: string): string {
  const value = data?.settings[key]?.overrideEnabled;
  return value == null ? "inherit" : value ? "enabled" : "disabled";
}

function matches(...values: (string | null | undefined)[]): boolean {
  return values.filter(Boolean).join(" ").toLowerCase().includes(query.trim().toLowerCase());
}

function liveFor(server: McpServerSummary): McpServerStatus | undefined {
  const owner = data?.settings[`mcp:${server.name}`]?.pluginId;
  if (!owner) return statuses[server.name];
  // Plugin integration IDs include the package ID; runtime MCP names do not.
  // A different plugin can win the runtime name collision.
  const live = statuses[server.name.slice(owner.length + 1)];
  return live?.pluginId === owner ? live : undefined;
}

function pluginServerLabel(server: McpServerSummary): string {
  const setting = data?.settings[`mcp:${server.name}`];
  if (data?.plugins.some((plugin) => plugin.id === setting?.pluginId && !plugin.enabled)) return "Disabled by plugin";
  if (!server.enabled) return "Disabled";
  // plugin/read exposes names, not the package's per-server defaults or policy.
  return setting?.overrideEnabled === true ? "Enabled preference" : "Plugin default";
}

async function toggleEnabled(server: McpServerSummary) {
  if (server.scope === "inherited" || server.scope === "plugin") {
    await changeSetting("mcp", server.name, server.enabled ? "disabled" : "enabled");
    return;
  }
  await actions.run(async () => {
    apply(await setMcpEnabled(server.name, !server.enabled));
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
    await mcpOauthLogin(server.name);
    // In browser preview there is no completion event, so reflect it now.
    await refreshStatus();
  });
  if (!started) delete signingIn[server.name];
}

async function toggleSkill(skill: SkillSummary) {
  await actions.run(async () => {
    await setSkillEnabled(skill.name, !skill.enabled, skill.path);
    skillsStatus.nonce += 1;
    // The notification reloads Codex's effective state in both views.
  });
}

async function setSkillView(skill: SkillSummary, open: boolean) {
  skillOpen[skill.path] = open;
  if (!open || skillText[skill.path] != null) return;
  skillText[skill.path] = null;
  try {
    skillText[skill.path] = await readSkill(skill.path);
  } catch (cause) {
    delete skillOpen[skill.path];
    actions.error = errorMessage(cause);
  }
}

async function doDeleteSkill(skill: SkillSummary) {
  await actions.run(async () => {
    apply(await deleteSkill(skill.path));
    confirmDeleteSkill = null;
    delete skillOpen[skill.path];
    delete skillText[skill.path];
  });
}

function openAddSkill() {
  addingSkill = true;
  editing = null;
  skillFormName = "";
  skillFormDescription = "";
  skillFormBody = "";
  skillForm.error = null;
}

async function saveSkillForm(event: SubmitEvent) {
  event.preventDefault();
  skillForm.error = null;
  const name = skillFormName.trim();
  if (!validSkillName(name)) {
    skillForm.error = "Name must be lowercase letters, digits, '-' or '_' (not leading).";
    return;
  }
  if (!skillFormDescription.trim()) {
    skillForm.error = "Description is required — it is what Codex matches against.";
    return;
  }
  await skillForm.run(async () => {
    apply(await createSkill({ name, description: skillFormDescription.trim(), body: skillFormBody.trim() || null }));
    addingSkill = false;
    filter = filter === "all" ? "all" : "skills";
    // Rows are keyed by path, not name.
    const created = data?.skills.find((row) => row.name === name);
    if (created)
      queueMicrotask(() =>
        document.getElementById(`skill-row-${encodeURIComponent(created.path)}`)?.scrollIntoView({ block: "nearest" }),
      );
  });
}

function openAdd() {
  addingSkill = false;
  editing = "";
  formName = "";
  formTransport = "stdio";
  formCommand = "";
  formArgs = "";
  formEnv = [];
  formUrl = "";
  formBearerEnv = "";
  serverForm.error = null;
}

function openConfigure(server: McpServerSummary) {
  editing = server.name;
  formName = server.name;
  // An entry with neither command nor url is malformed; editing it as stdio is
  // the repair path, since that is the transport the command field writes.
  formTransport = server.transport === "http" ? "http" : "stdio";
  formCommand = server.command ?? "";
  formArgs = formatArgs(server.args);
  // Existing secret values are never sent to the UI: show key names with empty,
  // write-only value fields. Leaving them blank preserves the stored secrets.
  formEnv = server.envKeys.map((key) => ({ key, value: "" }));
  formUrl = server.url ?? "";
  formBearerEnv = server.bearerTokenEnvVar ?? "";
  serverForm.error = null;
}

function cancelEdit() {
  editing = null;
  serverForm.error = null;
}

function addEnvRow() {
  formEnv = [...formEnv, { key: "", value: "" }];
}

function removeEnvRow(index: number) {
  formEnv = formEnv.filter((_, i) => i !== index);
}

async function saveForm(event: SubmitEvent) {
  event.preventDefault();
  serverForm.error = null;
  const name = formName.trim();
  if (!name) {
    serverForm.error = "Name is required.";
    return;
  }
  if (formTransport === "stdio" && !formCommand.trim()) {
    serverForm.error = "Command is required.";
    return;
  }
  if (formTransport === "http" && !formUrl.trim()) {
    serverForm.error = "URL is required.";
    return;
  }
  const keys = formEnv.map((row) => row.key.trim());
  if (formTransport === "stdio" && !envKeysValid(keys)) {
    serverForm.error = "Environment variable names must be unique and non-empty.";
    return;
  }
  // Only send env pairs that have a value typed this session. Empty values keep
  // the existing secret (on configure) or are omitted (on add).
  const env: Record<string, string> = {};
  for (const row of formEnv) {
    if (row.key.trim() && row.value) env[row.key.trim()] = row.value;
  }
  await serverForm.run(async () => {
    apply(
      await saveMcpServer(
        formTransport === "stdio"
          ? {
              // "" means the Add form, where there is no prior entry to edit.
              previousName: editing === "" ? null : editing,
              name,
              command: formCommand.trim(),
              args: parseArgs(formArgs),
              env,
              // Rows still on screen define the desired key set; anything the
              // user deleted is pruned from the stored env by its absence.
              envKeys: keys.filter(Boolean),
            }
          : {
              previousName: editing === "" ? null : editing,
              name,
              url: formUrl.trim(),
              bearerTokenEnvVar: formBearerEnv.trim() || null,
            },
      ),
    );
    editing = null;
    await refreshStatus();
  });
}

async function doRemove(name: string) {
  await actions.run(async () => {
    apply(await removeMcpServer(name));
    confirmRemove = null;
    delete expanded[name];
    await refreshStatus();
  });
}

/** Identifiers, commands and URLs: no autocorrection or spellcheck. */
const codeInput = {
  autocomplete: "off",
  autocorrect: "off",
  autocapitalize: "off",
  spellcheck: false,
} as const satisfies HTMLInputAttributes;

const TRANSPORTS: SegmentedOption<"stdio" | "http">[] = [
  { value: "stdio", label: "stdio (command)" },
  { value: "http", label: "HTTP (url)" },
];

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
      data?.settings[`mcp:${row.name}`]?.pluginId,
    ),
  ),
);
const skills = $derived(
  (data?.skills ?? []).filter((row) =>
    matches(
      row.name,
      row.displayName,
      row.description,
      row.shortDescription,
      data?.settings[`skill:${row.path}`]?.pluginId,
    ),
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
      <button type="button" onclick={openAddSkill} class="btn btn-sm hover:preset-tonal text-xs">
        <Plus size={13} /> Add skill
      </button>
      <button type="button" onclick={openAdd} class="btn btn-sm preset-tonal">
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

  {#if addingSkill}
    <form onsubmit={saveSkillForm} class="card mt-4 space-y-3 border border-surface-200-800 bg-surface-100-900 p-4">
      <div class="text-xs font-semibold text-surface-500">Add skill</div>
      <div>
        <label for="skill-name" class="text-xs font-medium text-surface-500">Name</label>
        <input
          id="skill-name"
          bind:value={skillFormName}
          placeholder="release-notes"
          class="input mt-1 w-full font-mono text-xs" {...codeInput}
        />
        <p class="mt-1 text-[11px] text-surface-500">Becomes the folder name under <span class="font-mono">~/.codex/skills/</span>.</p>
      </div>
      <div>
        <label for="skill-description" class="text-xs font-medium text-surface-500">Description</label>
        <input
          id="skill-description"
          bind:value={skillFormDescription}
          placeholder="Write release notes from merged PRs. Use when asked to draft a changelog."
          class="input mt-1 w-full text-xs"
        />
        <p class="mt-1 text-[11px] text-surface-500">Codex decides when to use the skill from this sentence, so say what it does and when.</p>
      </div>
      <div>
        <label for="skill-body" class="text-xs font-medium text-surface-500">Instructions (optional)</label>
        <textarea
          id="skill-body"
          bind:value={skillFormBody}
          rows={5}
          placeholder="## Instructions&#10;&#10;1. ..."
          class="textarea mt-1 w-full font-mono text-xs"
        ></textarea>
      </div>
      {#if skillForm.error}
        <div class="error-note">{skillForm.error}</div>
      {/if}
      <div class="flex items-center gap-2">
        <button type="submit" disabled={skillForm.busy} class="btn btn-sm preset-filled-primary-500">
          {skillForm.busy ? "Creating…" : "Create skill"}
        </button>
        <button type="button" onclick={() => (addingSkill = false)} class="btn btn-sm hover:preset-tonal text-surface-500">Cancel</button>
      </div>
    </form>
  {/if}

  {#if editing !== null}
    <form onsubmit={saveForm} class="card mt-4 space-y-3 border border-surface-200-800 bg-surface-100-900 p-4">
      <div class="text-xs font-semibold text-surface-500">
        {editing === "" ? "Add MCP server" : `Edit ${editing}`}
      </div>
      <div>
        <label for="mcp-name" class="text-xs font-medium text-surface-500">Name</label>
        <input
          id="mcp-name"
          bind:value={formName}
          placeholder="github"
          class="input mt-1 w-full font-mono text-xs" {...codeInput}
        />
        {#if editing !== "" && formName.trim() !== editing}
          <p class="mt-1 text-[11px] text-surface-500">Renaming keeps this server's settings and stored secrets.</p>
        {/if}
      </div>
      <div>
        <span class="text-xs font-medium text-surface-500">Transport</span>
        <Segmented label="Transport" value={formTransport} onChange={(next) => (formTransport = next)} class="mt-1" options={TRANSPORTS} />
      </div>
      {#if formTransport === "stdio"}
      <div>
        <label for="mcp-command" class="text-xs font-medium text-surface-500">Command</label>
        <input id="mcp-command" bind:value={formCommand} placeholder="npx" class="input mt-1 w-full font-mono text-xs" {...codeInput}
 />
      </div>
      <div>
        <label for="mcp-args" class="text-xs font-medium text-surface-500">Arguments</label>
        <input
          id="mcp-args"
          bind:value={formArgs}
          placeholder="-y @modelcontextprotocol/server-github"
          class="input mt-1 w-full font-mono text-xs" {...codeInput}
        />
      </div>
      <div>
        <div class="flex items-center justify-between">
          <span class="text-xs font-medium text-surface-500">Environment variables</span>
          <button type="button" onclick={addEnvRow} class="btn btn-sm hover:preset-tonal text-xs text-surface-500">
            <Plus size={12} /> Add
          </button>
        </div>
        {#if editing !== "" && formEnv.length > 0}
          <p class="mt-1 text-[11px] text-surface-500">Leave a value blank to keep the stored secret. Secret values are write-only.</p>
        {/if}
        <div class="mt-1.5 space-y-1.5">
          {#each formEnv as row, index (index)}
            <div class="flex items-center gap-1.5">
              <input bind:value={row.key} placeholder="GITHUB_TOKEN" class="input flex-1 font-mono text-xs" {...codeInput}
 />
              <input
                bind:value={row.value}
                type="password"
                placeholder={editing !== "" ? "unchanged" : "secret value"}
                class="input flex-1 font-mono text-xs" {...codeInput}
              />
              <TooltipButton
                label="Remove variable"
                type="button"
                onclick={() => removeEnvRow(index)}
                aria-label="Remove variable"
                class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500"
              >
                <X size={14} />
              </TooltipButton>
            </div>
          {/each}
        </div>
      </div>
      {:else}
      <div>
        <label for="mcp-url" class="text-xs font-medium text-surface-500">URL</label>
        <input
          id="mcp-url"
          bind:value={formUrl}
          placeholder="https://mcp.example.com/sse"
          class="input mt-1 w-full font-mono text-xs" {...codeInput}
        />
      </div>
      <div>
        <label for="mcp-bearer" class="text-xs font-medium text-surface-500">Bearer token variable</label>
        <input
          id="mcp-bearer"
          bind:value={formBearerEnv}
          placeholder="LINEAR_API_KEY"
          class="input mt-1 w-full font-mono text-xs" {...codeInput}
        />
        <p class="mt-1 text-[11px] text-surface-500">
          Name of an environment variable holding the token. Leave blank to sign in with OAuth instead.
        </p>
      </div>
      {/if}
      {#if serverForm.error}
        <div class="error-note">{serverForm.error}</div>
      {/if}
      <div class="flex items-center gap-2">
        <button type="submit" disabled={serverForm.busy} class="btn btn-sm preset-filled-primary-500">
          {serverForm.busy ? "Saving…" : editing === "" ? "Add server" : "Save changes"}
        </button>
        <button type="button" onclick={cancelEdit} class="btn btn-sm hover:preset-tonal text-surface-500">Cancel</button>
      </div>
    </form>
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
          {@const live = liveFor(server)}
          {@const status = rowStatus(server, live, checking)}
          {@const tools = toolsOf(live)}
          {@const resources = resourcesOf(live)}
          {@const templates = resourceTemplatesOf(live)}
          {@const info = serverInfoLines(live)}
          {@const contribution = contributionSummary(live)}
          {@const hasDetails = tools.length + resources.length + templates.length + info.length > 0}
          {@const auth = authAction(live)}
          <div id="mcp-row-{server.name}" class="card border border-surface-200-800 bg-surface-50-950 p-3">
            <Collapsible
              open={Boolean(expanded[server.name])}
              onOpenChange={(details) => (expanded[server.name] = details.open)}
              class="items-stretch gap-0"
            >
              <div class="flex items-center gap-3">
                <Plug size={16} class="shrink-0 text-surface-500" />
                <div class="min-w-0 flex-1">
                  <div class="flex items-center gap-2">
                    <span class="truncate font-medium">{live?.serverInfo?.title || server.name}</span>
                    <span class="shrink-0 rounded-full bg-surface-200-800 px-1.5 py-0.5 text-[10px] text-surface-600-400">{server.scope}</span>
                  </div>
                  <div class="mt-0.5 truncate text-[11px] text-surface-500">{capabilitySummary(server, live)}</div>
                </div>
                <div class="flex shrink-0 items-center gap-1.5">
                  <span class="size-1.5 rounded-full {statusDotClass(status)}"></span>
                  <span class="text-[10px] text-surface-500">{server.scope === "plugin" && !live ? pluginServerLabel(server) : projectPath ? server.enabled ? "Enabled" : "Disabled" : statusLabel(status)}</span>
                  {#if runtimeStatusLabel(live) && runtimeStatusLabel(live) !== statusLabel(status)}
                    <span class="text-[10px] text-surface-500" title="Codex runtime status">· {runtimeStatusLabel(live)}</span>
                  {/if}
                </div>
              </div>

              {#if live?.error}
                <div class="mt-2 flex items-start gap-1.5 text-[11px] text-error-500">
                  <X size={12} class="mt-0.5 shrink-0" />
                  <span>{live.error}</span>
                </div>
              {/if}
              {#if auth === "signIn"}
                <div class="mt-2 text-[11px] text-warning-600-400">This server needs you to sign in before its tools are available.</div>
              {:else if auth === "env"}
                <div class="mt-2 text-[11px] text-surface-500">
                  Authenticated from <span class="font-mono">{server.bearerTokenEnvVar ?? "a bearer token"}</span> in your environment.
                </div>
              {/if}

              <div class="mt-2.5 flex flex-wrap items-center gap-1.5">
                {#if auth === "signIn" && !projectPath}
                  <button
                    type="button"
                    onclick={() => signIn(server)}
                    disabled={signingIn[server.name]}
                    class="btn btn-sm preset-filled-primary-500 text-xs"
                  >
                    <LogIn size={12} />
                    {signingIn[server.name] ? "Waiting for browser…" : "Sign in"}
                  </button>
                {:else if auth === "signedIn"}
                  <span class="text-[11px] text-success-500">Signed in</span>
                {/if}
                {#if projectPath}
                  <select aria-label={`Setting for ${server.name}`} class="select text-xs" disabled={updating} value={settingValue(`mcp:${server.name}`)} onchange={(event) => changeSetting("mcp", server.name, event.currentTarget.value)}>
                    <option value="inherit">{server.scope === "plugin" ? "Inherit" : `Inherit (${data.settings[`mcp:${server.name}`]?.inheritedEnabled ? "enabled" : "disabled"})`}</option>
                    <option value="enabled">Enabled</option><option value="disabled">Disabled</option>
                  </select>
                {:else}
                <button type="button" onclick={() => toggleEnabled(server)} class="btn btn-sm hover:preset-tonal text-xs">
                  {server.enabled ? "Disable" : "Enable"}
                </button>
                {#if server.scope === "user" || server.scope === "global"}<button type="button" onclick={() => openConfigure(server)} class="btn btn-sm hover:preset-tonal text-xs">Edit</button>{/if}
                {/if}
                {#if hasDetails}
                  <Collapsible.Trigger class="group btn btn-sm hover:preset-tonal text-xs">
                    <ChevronRight size={12} class="transition-transform group-data-[state=open]:rotate-90" />
                    {contribution || "Details"}
                  </Collapsible.Trigger>
                {/if}
                {#if data.settings[`mcp:${server.name}`]?.pluginId}
                  {@const owner = data.settings[`mcp:${server.name}`].pluginId}
                  <span class="text-xs text-surface-500">Plugin: {owner}{data.plugins.some((plugin) => plugin.id === owner && !plugin.enabled) ? " (disabled)" : ""}</span>
                {/if}
                {#if !projectPath && (server.scope === "user" || server.scope === "global")}
                {#if confirmRemove === server.name}
                  <button type="button" onclick={() => doRemove(server.name)} class="btn btn-sm preset-filled-error-500 text-xs">Confirm remove</button>
                  <button type="button" onclick={() => (confirmRemove = null)} class="btn btn-sm hover:preset-tonal text-xs">Cancel</button>
                {:else}
                  <TooltipButton
                    label={`Remove ${server.name}`}
                    type="button"
                    onclick={() => (confirmRemove = server.name)}
                    aria-label="Remove {server.name}"
                    class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500"
                  >
                    <Trash2 size={13} />
                  </TooltipButton>
                {/if}
                {/if}
              </div>

              {#if hasDetails}
                <Collapsible.Content>
                {#if expanded[server.name]}
                <div class="mt-2.5 space-y-3 border-t border-surface-200-800 pt-2.5">
                {#if info.length > 0}
                  <dl class="grid grid-cols-[auto_1fr] gap-x-3 gap-y-0.5 text-[11px]">
                    {#each info as line (line.label)}
                      <dt class="text-surface-500">{line.label}</dt>
                      <dd class="min-w-0 break-words text-surface-600-400">
                        {#if line.href}
                          <button type="button" onclick={() => openExternalUrl(line.href ?? "")} class="inline-flex items-center gap-1 text-primary-500 hover:underline">
                            {line.value} <ExternalLink size={10} />
                          </button>
                        {:else}
                          {line.value}
                        {/if}
                      </dd>
                    {/each}
                  </dl>
                {/if}
                {#if tools.length > 0}
                <div>
                <div class="mb-1 text-[10px] font-semibold uppercase tracking-wide text-surface-500">Tools</div>
                <ul class="space-y-2">
                  {#each tools as tool (tool.name)}
                    {@const parameters = toolParameters(tool)}
                    <li id="mcp-tool-{server.name}-{tool.name}">
                      <div class="font-mono text-[11px] font-medium">{tool.name}</div>
                      {#if tool.description}
                        <p class="mt-0.5 text-[11px] leading-snug text-surface-600-400">{tool.description}</p>
                      {/if}
                      {#if parameters.length > 0}
                        <dl class="mt-1 space-y-0.5">
                          {#each parameters as parameter (parameter.name)}
                            <div class="flex gap-1.5 text-[10px]">
                              <dt class="shrink-0 font-mono text-surface-600-400">
                                {parameter.name}{parameter.required ? "" : "?"}
                                <span class="text-surface-400">: {parameter.type}</span>
                              </dt>
                              {#if parameter.hint}
                                <dd class="min-w-0 flex-1 truncate text-surface-500">{parameter.hint}</dd>
                              {/if}
                            </div>
                          {/each}
                        </dl>
                      {/if}
                    </li>
                  {/each}
                </ul>
                </div>
                {/if}
                {#if resources.length > 0}
                  <div>
                    <div class="mb-1 text-[10px] font-semibold uppercase tracking-wide text-surface-500">Resources</div>
                    <ul class="space-y-1.5">
                      {#each resources as resource (resource.uri)}
                        <li>
                          <div class="flex items-baseline gap-2">
                            <span class="font-mono text-[11px] font-medium break-all">{resource.uri}</span>
                            {#if resource.mimeType}<span class="shrink-0 text-[10px] text-surface-400">{resource.mimeType}</span>{/if}
                          </div>
                          {#if resource.title || resource.name || resource.description}
                            <p class="mt-0.5 text-[11px] leading-snug text-surface-600-400">
                              {[resource.title || resource.name, resource.description].filter(Boolean).join(" — ")}
                            </p>
                          {/if}
                        </li>
                      {/each}
                    </ul>
                  </div>
                {/if}
                {#if templates.length > 0}
                  <div>
                    <div class="mb-1 text-[10px] font-semibold uppercase tracking-wide text-surface-500">Resource templates</div>
                    <ul class="space-y-1.5">
                      {#each templates as template (template.uriTemplate)}
                        <li>
                          <div class="flex items-baseline gap-2">
                            <span class="font-mono text-[11px] font-medium break-all">{template.uriTemplate}</span>
                            {#if template.mimeType}<span class="shrink-0 text-[10px] text-surface-400">{template.mimeType}</span>{/if}
                          </div>
                          {#if template.title || template.name || template.description}
                            <p class="mt-0.5 text-[11px] leading-snug text-surface-600-400">
                              {[template.title || template.name, template.description].filter(Boolean).join(" — ")}
                            </p>
                          {/if}
                        </li>
                      {/each}
                    </ul>
                  </div>
                {/if}
                </div>
                {/if}
                </Collapsible.Content>
              {/if}
            </Collapsible>
          </div>
        {/each}
      {/if}

      {#if showSkills}
        {#if projectPath}
          <p class="text-xs text-surface-500">Codex does not apply project skill enablement settings. Manage skill enablement in Settings.</p>
        {/if}
        {#if filter === "skills" && data.skills.length === 0 && !query.trim()}
          <div class="flex items-center gap-2 text-xs text-surface-500">
            <span>Codex reported no skills for this home.</span>
            {#if !projectPath}<button type="button" onclick={openAddSkill} class="btn btn-sm preset-tonal text-xs"><Plus size={12} /> Add skill</button>{/if}
          </div>
        {/if}
        {#each skills as skill (skill.path)}
          <div id="skill-row-{encodeURIComponent(skill.path)}" class="card border border-surface-200-800 bg-surface-50-950 p-3">
            <Collapsible
              open={Boolean(skillOpen[skill.path])}
              onOpenChange={(details) => setSkillView(skill, details.open)}
              class="items-stretch gap-0"
            >
              <div class="flex items-center gap-3">
                <Sparkles size={16} class="shrink-0 {skill.enabled ? 'text-tertiary-500' : 'text-surface-500'}" />
                <div class="min-w-0 flex-1">
                  <div class="flex items-center gap-2">
                    <span class="truncate font-medium">{skill.displayName || skill.name}</span>
                    <span class="shrink-0 rounded-full bg-surface-200-800 px-1.5 py-0.5 text-[10px] text-surface-600-400">{skill.scope}</span>
                  </div>
                  {#if skill.displayName}
                    <div class="mt-0.5 truncate font-mono text-[10px] text-surface-400">{skill.name}</div>
                  {/if}
                </div>
                <div class="flex shrink-0 items-center gap-1.5">
                  <span class="size-1.5 rounded-full {skill.enabled ? 'bg-success-500' : 'bg-surface-400'}"></span>
                  <span class="text-[10px] text-surface-500">{skill.enabled ? "Enabled" : "Disabled"}</span>
                </div>
              </div>
              {#if skill.shortDescription || skill.description}
                <p class="mt-1.5 line-clamp-3 text-[11px] leading-snug text-surface-600-400">
                  {skill.shortDescription || skill.description}
                </p>
              {/if}
              <div class="mt-2.5 flex flex-wrap items-center gap-1.5">
                {#if !projectPath}
                <button type="button" onclick={() => toggleSkill(skill)} class="btn btn-sm hover:preset-tonal text-xs">
                  {skill.enabled ? "Disable" : "Enable"}
                </button>
                {/if}
                {#if data.settings[`skill:${skill.path}`]?.pluginId}
                  <span class="text-xs text-surface-500">Plugin: {data.settings[`skill:${skill.path}`].pluginId}</span>
                {/if}
                <Collapsible.Trigger class="group btn btn-sm hover:preset-tonal text-xs">
                  <ChevronRight size={12} class="transition-transform group-data-[state=open]:rotate-90" />
                  View SKILL.md
                </Collapsible.Trigger>
                <TooltipButton label="Reveal in Finder" type="button" onclick={() => revealInFinder(skill.path)} aria-label="Reveal {skill.name} in Finder" class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500">
                  <FolderOpen size={13} />
                </TooltipButton>
                <TooltipButton label="Open in Zed" type="button" onclick={() => openInZed(skill.path)} aria-label="Open {skill.name} in Zed" class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500">
                  <FileText size={13} />
                </TooltipButton>
                <span class="min-w-0 flex-1 truncate font-mono text-[10px] text-surface-500">{skill.path}</span>
                {#if skill.scope === "user" && !projectPath}
                  {#if confirmDeleteSkill === skill.path}
                    <button type="button" onclick={() => doDeleteSkill(skill)} class="btn btn-sm preset-filled-error-500 text-xs">Confirm delete</button>
                    <button type="button" onclick={() => (confirmDeleteSkill = null)} class="btn btn-sm hover:preset-tonal text-xs">Cancel</button>
                  {:else}
                    <TooltipButton
                      label={`Delete ${skill.name}`}
                      type="button"
                      onclick={() => (confirmDeleteSkill = skill.path)}
                      aria-label="Delete {skill.name}"
                      class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500"
                    >
                      <Trash2 size={13} />
                    </TooltipButton>
                  {/if}
                {/if}
              </div>
              <Collapsible.Content>
              {#if skillOpen[skill.path]}
                <div class="mt-2.5 border-t border-surface-200-800 pt-2.5">
                  {#if skillText[skill.path] == null}
                    <div class="text-[11px] text-surface-500">Loading SKILL.md…</div>
                  {:else}
                    {@const parsed = splitFrontmatter(skillText[skill.path] ?? "")}
                    <div class="max-h-80 overflow-auto rounded bg-surface-100-900 p-3 text-xs leading-5">
                      {#if parsed.meta.length > 0}
                        <dl class="mb-2 grid grid-cols-[auto_1fr] gap-x-3 gap-y-0.5 border-b border-surface-200-800 pb-2 text-[11px]">
                          {#each parsed.meta as entry (entry.key)}
                            <dt class="font-mono text-surface-500">{entry.key}</dt>
                            <dd class="min-w-0 break-words text-surface-600-400">{entry.value}</dd>
                          {/each}
                        </dl>
                      {/if}
                      <div class="prose-md prose-md-compact">{@html renderMarkdown(parsed.body)}</div>
                    </div>
                  {/if}
                </div>
              {/if}
              </Collapsible.Content>
            </Collapsible>
          </div>
        {/each}
      {/if}

      {#if showPlugins}
        {#each plugins as plugin (plugin.id)}
          <div class="card border border-surface-200-800 p-3">
            <div class="flex items-center gap-2"><Puzzle size={16} /><span class="font-medium">{plugin.name}</span><span class="text-xs text-surface-500">{plugin.enabled ? "Enabled" : "Disabled"}</span></div>
            <p class="mt-1 text-xs text-surface-500">{plugin.id}</p>
            {#if plugin.description}<p class="mt-1 text-xs">{plugin.description}</p>{/if}
            {#if projectPath}
              <select aria-label={`Setting for ${plugin.id}`} class="select mt-2 text-xs" disabled={updating} value={settingValue(`plugin:${plugin.id}`)} onchange={(event) => changeSetting("plugin", plugin.id, event.currentTarget.value)}>
                <option value="inherit">Inherit ({data.settings[`plugin:${plugin.id}`]?.inheritedEnabled ? "enabled" : "disabled"})</option>
                <option value="enabled">Enabled</option><option value="disabled">Disabled</option>
              </select>
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
