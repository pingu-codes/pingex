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
  setMcpEnabled,
  setSkillEnabled,
} from "$lib/services/api";
import { mcpStatus as mcpStatusEvents } from "$lib/services/codexEvents.svelte";
import type { IntegrationsList, McpServerStatus, McpServerSummary, SkillSummary } from "$lib/types";
import { renderMarkdown } from "$lib/utils/markdown";

let {
  focusServer = null,
  focusTool = null,
  nonce = 0,
  onGoToConnections,
}: {
  focusServer?: string | null;
  /** Tool within `focusServer` to expand and scroll to (from a tool call). */
  focusTool?: string | null;
  nonce?: number;
  onGoToConnections?: () => void;
} = $props();

let data = $state<IntegrationsList | null>(null);
let loadError = $state<string | null>(null);
let actionError = $state<string | null>(null);
let filter = $state<IntegrationFilter>("all");

/**
 * Live state from Codex, keyed by server name and joined onto `data.mcpServers`
 * (which is what `config.toml` declares). A server can appear in one and not
 * the other: configured but not started, or provided by a plugin and never
 * written to `config.toml` at all.
 */
let statuses = $state<Record<string, McpServerStatus>>({});
let checking = $state(false);
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
let skillFormError = $state<string | null>(null);
let skillSaving = $state(false);

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
let formError = $state<string | null>(null);
let saving = $state(false);

async function load() {
  loadError = null;
  try {
    data = await listIntegrations();
  } catch (cause) {
    loadError = cause instanceof Error ? cause.message : String(cause);
  }
}

/**
 * Refresh Codex's live view. Servers start asynchronously, so this is also what
 * the "Refresh" button and the startup notification both trigger — there is no
 * one moment when the picture is final.
 */
async function refreshStatus() {
  checking = true;
  try {
    statuses = await listMcpServerStatus();
  } catch (cause) {
    actionError = cause instanceof Error ? cause.message : String(cause);
  } finally {
    checking = false;
  }
}

// Reload whenever the section is (re-)opened, and honour a focus request.
$effect(() => {
  // Reference nonce so re-opening the same section re-runs this effect.
  void nonce;
  const server = focusServer;
  const tool = focusTool;
  // The tool list has to exist before it can be scrolled to, so wait for the
  // status call rather than just the config read.
  Promise.all([load(), refreshStatus()]).then(() => {
    if (!server) return;
    filter = "mcp";
    expanded[server] = true;
    queueMicrotask(() => {
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
  data = next;
}

async function toggleEnabled(server: McpServerSummary) {
  actionError = null;
  try {
    apply(await setMcpEnabled(server.name, !server.enabled));
    // The backend reloaded Codex's config; pick up the resulting state.
    await refreshStatus();
  } catch (cause) {
    actionError = cause instanceof Error ? cause.message : String(cause);
  }
}

/**
 * Start an OAuth sign-in. Codex opens the browser and drives the whole flow;
 * we stay pending until it reports back, since there is nothing to poll.
 */
async function signIn(server: McpServerSummary) {
  actionError = null;
  signingIn[server.name] = true;
  try {
    await mcpOauthLogin(server.name);
    // In browser preview there is no completion event, so reflect it now.
    await refreshStatus();
  } catch (cause) {
    delete signingIn[server.name];
    actionError = cause instanceof Error ? cause.message : String(cause);
  }
}

async function toggleSkill(skill: SkillSummary) {
  actionError = null;
  try {
    await setSkillEnabled(skill.name, !skill.enabled);
    // Codex owns the enabled state; re-read rather than assuming it took.
    await load();
  } catch (cause) {
    actionError = cause instanceof Error ? cause.message : String(cause);
  }
}

async function toggleSkillView(skill: SkillSummary) {
  const open = !skillOpen[skill.name];
  skillOpen[skill.name] = open;
  if (!open || skillText[skill.name] != null) return;
  skillText[skill.name] = null;
  try {
    skillText[skill.name] = await readSkill(skill.path);
  } catch (cause) {
    delete skillOpen[skill.name];
    actionError = cause instanceof Error ? cause.message : String(cause);
  }
}

async function doDeleteSkill(skill: SkillSummary) {
  actionError = null;
  try {
    apply(await deleteSkill(skill.path));
    confirmDeleteSkill = null;
    delete skillOpen[skill.name];
    delete skillText[skill.name];
  } catch (cause) {
    actionError = cause instanceof Error ? cause.message : String(cause);
  }
}

function openAddSkill() {
  addingSkill = true;
  editing = null;
  skillFormName = "";
  skillFormDescription = "";
  skillFormBody = "";
  skillFormError = null;
}

async function saveSkillForm(event: SubmitEvent) {
  event.preventDefault();
  skillFormError = null;
  const name = skillFormName.trim();
  if (!validSkillName(name)) {
    skillFormError = "Name must be lowercase letters, digits, '-' or '_' (not leading).";
    return;
  }
  if (!skillFormDescription.trim()) {
    skillFormError = "Description is required — it is what Codex matches against.";
    return;
  }
  skillSaving = true;
  try {
    apply(await createSkill({ name, description: skillFormDescription.trim(), body: skillFormBody.trim() || null }));
    addingSkill = false;
    filter = filter === "all" ? "all" : "skills";
    queueMicrotask(() => document.getElementById(`skill-row-${name}`)?.scrollIntoView({ block: "nearest" }));
  } catch (cause) {
    skillFormError = cause instanceof Error ? cause.message : String(cause);
  } finally {
    skillSaving = false;
  }
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
  formError = null;
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
  formError = null;
}

function cancelEdit() {
  editing = null;
  formError = null;
}

function addEnvRow() {
  formEnv = [...formEnv, { key: "", value: "" }];
}

function removeEnvRow(index: number) {
  formEnv = formEnv.filter((_, i) => i !== index);
}

async function saveForm(event: SubmitEvent) {
  event.preventDefault();
  formError = null;
  const name = formName.trim();
  if (!name) {
    formError = "Name is required.";
    return;
  }
  if (formTransport === "stdio" && !formCommand.trim()) {
    formError = "Command is required.";
    return;
  }
  if (formTransport === "http" && !formUrl.trim()) {
    formError = "URL is required.";
    return;
  }
  const keys = formEnv.map((row) => row.key.trim());
  if (formTransport === "stdio" && !envKeysValid(keys)) {
    formError = "Environment variable names must be unique and non-empty.";
    return;
  }
  // Only send env pairs that have a value typed this session. Empty values keep
  // the existing secret (on configure) or are omitted (on add).
  const env: Record<string, string> = {};
  for (const row of formEnv) {
    if (row.key.trim() && row.value) env[row.key.trim()] = row.value;
  }
  saving = true;
  try {
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
  } catch (cause) {
    formError = cause instanceof Error ? cause.message : String(cause);
  } finally {
    saving = false;
  }
}

async function doRemove(name: string) {
  actionError = null;
  try {
    apply(await removeMcpServer(name));
    confirmRemove = null;
    delete expanded[name];
    await refreshStatus();
  } catch (cause) {
    actionError = cause instanceof Error ? cause.message : String(cause);
  }
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
</script>

<div class="text-sm">
  <div class="flex items-center justify-between gap-2">
    <div>
      <div class="text-base font-semibold">Integrations</div>
      <div class="mt-1 text-xs text-surface-600-400">MCP servers, skills, and plugins for this Codex home.</div>
    </div>
    <div class="flex shrink-0 items-center gap-1.5">
      <button type="button" onclick={refreshStatus} disabled={checking} class="btn btn-sm hover:preset-tonal text-xs">
        <RefreshCw size={13} class={checking ? "animate-spin" : ""} /> Refresh
      </button>
      <button type="button" onclick={openAddSkill} class="btn btn-sm hover:preset-tonal text-xs">
        <Plus size={13} /> Add skill
      </button>
      <button type="button" onclick={openAdd} class="btn btn-sm preset-tonal">
        <Plus size={14} /> Add MCP server
      </button>
    </div>
  </div>

  <div class="mt-4 flex flex-wrap gap-1" role="tablist" aria-label="Integration type">
    {#each FILTERS as tab (tab.id)}
      <button
        type="button"
        role="tab"
        aria-selected={filter === tab.id}
        onclick={() => (filter = tab.id)}
        class="rounded-full px-3 py-1 text-xs transition-colors {filter === tab.id
          ? 'preset-filled-primary-500'
          : 'hover:preset-tonal text-surface-600-400'}"
      >
        {tab.label}
      </button>
    {/each}
  </div>

  {#if loadError}
    <div class="card preset-tonal-error mt-4 px-3 py-2 text-xs">{loadError}</div>
  {/if}
  {#if actionError}
    <div class="card preset-tonal-error mt-4 px-3 py-2 text-xs">{actionError}</div>
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
          class="input mt-1 w-full font-mono text-xs" autocomplete="off" autocorrect="off" autocapitalize="off" spellcheck={false}
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
      {#if skillFormError}
        <div class="card preset-tonal-error px-3 py-2 text-xs">{skillFormError}</div>
      {/if}
      <div class="flex items-center gap-2">
        <button type="submit" disabled={skillSaving} class="btn btn-sm preset-filled-primary-500">
          {skillSaving ? "Creating…" : "Create skill"}
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
          class="input mt-1 w-full font-mono text-xs" autocomplete="off" autocorrect="off" autocapitalize="off" spellcheck={false}
        />
        {#if editing !== "" && formName.trim() !== editing}
          <p class="mt-1 text-[11px] text-surface-500">Renaming keeps this server's settings and stored secrets.</p>
        {/if}
      </div>
      <div>
        <span class="text-xs font-medium text-surface-500">Transport</span>
        <div class="mt-1 flex gap-1">
          {#each [{ id: "stdio", label: "stdio (command)" }, { id: "http", label: "HTTP (url)" }] as option (option.id)}
            <button
              type="button"
              onclick={() => (formTransport = option.id as "stdio" | "http")}
              aria-pressed={formTransport === option.id}
              class="btn btn-sm text-xs {formTransport === option.id ? 'preset-filled-surface-200-800' : 'hover:preset-tonal text-surface-500'}"
            >
              {option.label}
            </button>
          {/each}
        </div>
      </div>
      {#if formTransport === "stdio"}
      <div>
        <label for="mcp-command" class="text-xs font-medium text-surface-500">Command</label>
        <input id="mcp-command" bind:value={formCommand} placeholder="npx" class="input mt-1 w-full font-mono text-xs" autocomplete="off" autocorrect="off" autocapitalize="off" spellcheck={false}
 />
      </div>
      <div>
        <label for="mcp-args" class="text-xs font-medium text-surface-500">Arguments</label>
        <input
          id="mcp-args"
          bind:value={formArgs}
          placeholder="-y @modelcontextprotocol/server-github"
          class="input mt-1 w-full font-mono text-xs" autocomplete="off" autocorrect="off" autocapitalize="off" spellcheck={false}
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
              <input bind:value={row.key} placeholder="GITHUB_TOKEN" class="input flex-1 font-mono text-xs" autocomplete="off" autocorrect="off" autocapitalize="off" spellcheck={false}
 />
              <input
                bind:value={row.value}
                type="password"
                placeholder={editing !== "" ? "unchanged" : "secret value"}
                class="input flex-1 font-mono text-xs" autocomplete="off" autocorrect="off" autocapitalize="off" spellcheck={false}
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
          class="input mt-1 w-full font-mono text-xs" autocomplete="off" autocorrect="off" autocapitalize="off" spellcheck={false}
        />
      </div>
      <div>
        <label for="mcp-bearer" class="text-xs font-medium text-surface-500">Bearer token variable</label>
        <input
          id="mcp-bearer"
          bind:value={formBearerEnv}
          placeholder="LINEAR_API_KEY"
          class="input mt-1 w-full font-mono text-xs" autocomplete="off" autocorrect="off" autocapitalize="off" spellcheck={false}
        />
        <p class="mt-1 text-[11px] text-surface-500">
          Name of an environment variable holding the token. Leave blank to sign in with OAuth instead.
        </p>
      </div>
      {/if}
      {#if formError}
        <div class="card preset-tonal-error px-3 py-2 text-xs">{formError}</div>
      {/if}
      <div class="flex items-center gap-2">
        <button type="submit" disabled={saving} class="btn btn-sm preset-filled-primary-500">
          {saving ? "Saving…" : editing === "" ? "Add server" : "Save changes"}
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
        {#if data.mcpServers.length === 0}
          <div class="text-xs text-surface-500">No MCP servers configured.</div>
        {/if}
        {#each data.mcpServers as server (server.name)}
          {@const live = statuses[server.name]}
          {@const status = rowStatus(server, live, checking)}
          {@const tools = toolsOf(live)}
          {@const resources = resourcesOf(live)}
          {@const templates = resourceTemplatesOf(live)}
          {@const info = serverInfoLines(live)}
          {@const contribution = contributionSummary(live)}
          {@const hasDetails = tools.length + resources.length + templates.length + info.length > 0}
          {@const auth = authAction(live)}
          <div id="mcp-row-{server.name}" class="card border border-surface-200-800 bg-surface-50-950 p-3">
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
                <span class="text-[10px] text-surface-500">{statusLabel(status)}</span>
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
              {#if auth === "signIn"}
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
              <button type="button" onclick={() => toggleEnabled(server)} class="btn btn-sm hover:preset-tonal text-xs">
                {server.enabled ? "Disable" : "Enable"}
              </button>
              <button type="button" onclick={() => openConfigure(server)} class="btn btn-sm hover:preset-tonal text-xs">Edit</button>
              {#if hasDetails}
                <button
                  type="button"
                  onclick={() => (expanded[server.name] = !expanded[server.name])}
                  aria-expanded={Boolean(expanded[server.name])}
                  class="btn btn-sm hover:preset-tonal text-xs"
                >
                  <ChevronRight size={12} class="transition-transform {expanded[server.name] ? 'rotate-90' : ''}" />
                  {contribution || "Details"}
                </button>
              {/if}
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
            </div>

            {#if expanded[server.name] && hasDetails}
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
          </div>
        {/each}
      {/if}

      {#if showSkills}
        {#if filter === "skills" && data.skills.length === 0}
          <div class="flex items-center gap-2 text-xs text-surface-500">
            <span>Codex reported no skills for this home.</span>
            <button type="button" onclick={openAddSkill} class="btn btn-sm preset-tonal text-xs"><Plus size={12} /> Add skill</button>
          </div>
        {/if}
        {#each data.skills as skill (skill.name)}
          <div id="skill-row-{skill.name}" class="card border border-surface-200-800 bg-surface-50-950 p-3">
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
              <button type="button" onclick={() => toggleSkill(skill)} class="btn btn-sm hover:preset-tonal text-xs">
                {skill.enabled ? "Disable" : "Enable"}
              </button>
              <button
                type="button"
                onclick={() => toggleSkillView(skill)}
                aria-expanded={Boolean(skillOpen[skill.name])}
                class="btn btn-sm hover:preset-tonal text-xs"
              >
                <ChevronRight size={12} class="transition-transform {skillOpen[skill.name] ? 'rotate-90' : ''}" />
                View SKILL.md
              </button>
              <TooltipButton label="Reveal in Finder" type="button" onclick={() => revealInFinder(skill.path)} aria-label="Reveal {skill.name} in Finder" class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500">
                <FolderOpen size={13} />
              </TooltipButton>
              <TooltipButton label="Open in Zed" type="button" onclick={() => openInZed(skill.path)} aria-label="Open {skill.name} in Zed" class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500">
                <FileText size={13} />
              </TooltipButton>
              <span class="min-w-0 flex-1 truncate font-mono text-[10px] text-surface-500">{skill.path}</span>
              {#if skill.scope === "user"}
                {#if confirmDeleteSkill === skill.name}
                  <button type="button" onclick={() => doDeleteSkill(skill)} class="btn btn-sm preset-filled-error-500 text-xs">Confirm delete</button>
                  <button type="button" onclick={() => (confirmDeleteSkill = null)} class="btn btn-sm hover:preset-tonal text-xs">Cancel</button>
                {:else}
                  <TooltipButton
                    label={`Delete ${skill.name}`}
                    type="button"
                    onclick={() => (confirmDeleteSkill = skill.name)}
                    aria-label="Delete {skill.name}"
                    class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500"
                  >
                    <Trash2 size={13} />
                  </TooltipButton>
                {/if}
              {/if}
            </div>
            {#if skillOpen[skill.name]}
              <div class="mt-2.5 border-t border-surface-200-800 pt-2.5">
                {#if skillText[skill.name] == null}
                  <div class="text-[11px] text-surface-500">Loading SKILL.md…</div>
                {:else}
                  {@const parsed = splitFrontmatter(skillText[skill.name] ?? "")}
                  <div class="max-h-80 overflow-auto rounded bg-surface-100-900 p-3 text-xs leading-5">
                    {#if parsed.meta.length > 0}
                      <dl class="mb-2 grid grid-cols-[auto_1fr] gap-x-3 gap-y-0.5 border-b border-surface-200-800 pb-2 text-[11px]">
                        {#each parsed.meta as entry (entry.key)}
                          <dt class="font-mono text-surface-500">{entry.key}</dt>
                          <dd class="min-w-0 break-words text-surface-600-400">{entry.value}</dd>
                        {/each}
                      </dl>
                    {/if}
                    <div class="prose-skill">{@html renderMarkdown(parsed.body)}</div>
                  </div>
                {/if}
              </div>
            {/if}
          </div>
        {/each}
      {/if}

      {#if showPlugins}
        <div class="card flex items-center gap-3 border border-dashed border-surface-200-800 p-3 text-xs text-surface-500">
          <Puzzle size={16} class="shrink-0" />
          <span>Plugins are not yet supported by this Codex build.</span>
        </div>
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

<style>
  .prose-skill :global(h1),
  .prose-skill :global(h2),
  .prose-skill :global(h3) {
    margin: 0.6rem 0 0.25rem;
    font-weight: 600;
  }
  .prose-skill :global(h1) { font-size: 0.9rem; }
  .prose-skill :global(h2) { font-size: 0.8rem; }
  .prose-skill :global(p) { margin: 0.35rem 0; }
  .prose-skill :global(ul),
  .prose-skill :global(ol) {
    margin: 0.35rem 0;
    padding-left: 1.1rem;
  }
  .prose-skill :global(ul) { list-style: disc; }
  .prose-skill :global(ol) { list-style: decimal; }
  .prose-skill :global(pre) {
    overflow-x: auto;
    border-radius: 0.5rem;
    background: #0d1117;
    padding: 0.5rem 0.7rem;
    font-size: 11px;
  }
  .prose-skill :global(code) { font-size: 11px; }
  .prose-skill :global(hr) { margin: 0.5rem 0; opacity: 0.3; }
</style>
