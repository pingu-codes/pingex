<script lang="ts">
import { SvelteMap, SvelteSet } from "svelte/reactivity";
import { hostFromOption } from "$lib/app/host";
import { commands, type HarnessKind } from "$lib/bindings";
import HostSelect from "$lib/components/HostSelect.svelte";
import { type ClaudeStatus, readHomeClaudeStatus } from "$lib/services/api";
import { chooseDefaultHome, loadProfile, profileHomes, removeHome, updateHome } from "$lib/services/homeRouting";
import { isTauri } from "$lib/services/tauri";
import { submitState } from "$lib/utils/actionState.svelte";

let harness = $state<HarnessKind>("codex");
let host = $state("native");
let configDir = $state("~/.codex");
let binary = $state("codex");
let label = $state("");
const action = submitState();
const homes = $derived(profileHomes());

// One probe per Claude Home, filled in as it returns: `claude auth status`
// shells out, so the list paints first and the badges follow. A missing entry
// means the probe has not answered yet; `null` means it failed.
const logins = new SvelteMap<string, ClaudeStatus | null>();
// Ids already probed, so a repeat pass does not fire a second subprocess.
const probed = new SvelteSet<string>();
// Which row is open for editing, and which is one click from removal.
let editing = $state<string | null>(null);
let confirming = $state<string | null>(null);
// The edit form's fields, seeded from the row when it opens.
let editDir = $state("");
let editBinary = $state("");
let editLabel = $state("");

// Re-probe whenever the set of Claude Homes changes — after an add or a
// removal, `loadProfile` replaces the list and the ids that went with it. An
// edit keeps the id, so `saveEdit` re-probes that one itself.
$effect(() => {
  const claudeHomes = homes.filter((entry) => entry.home.harness === "claude").map((entry) => entry.home.id);
  for (const id of [...probed]) {
    if (claudeHomes.includes(id)) continue;
    probed.delete(id);
    logins.delete(id);
  }
  for (const id of claudeHomes) {
    if (probed.has(id)) continue;
    probed.add(id);
    readHomeClaudeStatus(id)
      .then((status) => logins.set(id, status))
      .catch(() => logins.set(id, null));
  }
});

function openEditor(id: string, dir: string, cli: string, name: string) {
  editing = editing === id ? null : id;
  confirming = null;
  action.error = null;
  editDir = dir;
  editBinary = cli;
  editLabel = name;
}

async function saveEdit(id: string) {
  const saved = await action.run(() => updateHome(id, editDir, editBinary, editLabel));
  if (!saved) return;
  editing = null;
  // A new config dir is a different login; drop the old badge and ask again.
  if (probed.has(id)) {
    logins.delete(id);
    readHomeClaudeStatus(id)
      .then((status) => logins.set(id, status))
      .catch(() => logins.set(id, null));
  }
}

async function remove(id: string) {
  const removed = await action.run(() => removeHome(id));
  if (removed) confirming = null;
}

async function add() {
  await action.run(async () => {
    await commands.registerProfileHome(
      harness,
      hostFromOption(host),
      configDir,
      binary,
      label.trim() || (harness === "codex" ? "Codex" : "Claude Code"),
    );
    await loadProfile(false);
    label = "";
  });
}
</script>

{#snippet fields(
  name: string,
  dir: string,
  cli: string,
  onName: (value: string) => void,
  onDir: (value: string) => void,
  onCli: (value: string) => void,
)}
  <label class="label text-xs"
    ><span>Name</span
    ><input
      class="input"
      value={name}
      oninput={(event) => onName(event.currentTarget.value)}
      placeholder="Optional label, for example Work"
    /></label
  >
  <label class="label text-xs"
    ><span>Configuration directory</span
    ><input class="input font-mono" value={dir} oninput={(event) => onDir(event.currentTarget.value)} required /></label
  >
  <label class="label text-xs"
    ><span>CLI executable</span
    ><input class="input font-mono" value={cli} oninput={(event) => onCli(event.currentTarget.value)} required /></label
  >
{/snippet}

<div class="mt-5 space-y-3 border-t border-surface-200-800 pt-4">
  <h3 class="text-sm font-medium">Homes in this Profile</h3>
  <p class="text-xs text-surface-500">
    Each Host has its own Codex and Claude Code defaults. Existing conversations keep their Home.
  </p>
  {#each homes as entry (entry.home.id)}
    {@const login = logins.get(entry.home.id)}
    {@const checking = entry.home.harness === "claude" && !logins.has(entry.home.id)}
    <div class="space-y-2 rounded border border-surface-200-800 p-2 text-xs" data-testid="home-row">
      <div class="flex items-center gap-3">
        <div class="min-w-0 flex-1">
          <div class="flex flex-wrap items-center gap-2">
            <span>{entry.home.label} · {entry.home.host.kind === "wsl" ? `WSL · ${entry.home.host.distro}` : "This computer"}</span>
            {#if entry.home.harness === "claude"}
              {#if checking}
                <span class="chip preset-tonal text-[10px] text-surface-500" data-testid="home-login">Checking…</span>
              {:else if login?.loggedIn === true}
                <span class="chip preset-tonal-success text-[10px]" data-testid="home-login">Logged in</span>
              {:else if login?.loggedIn === false}
                <span class="chip preset-tonal-warning text-[10px]" data-testid="home-login">Not logged in</span>
              {:else}
                <span
                  class="chip preset-tonal text-[10px] text-surface-500"
                  data-testid="home-login"
                  title={login?.message ?? "The login could not be checked."}>Login unknown</span
                >
              {/if}
            {/if}
          </div>
          <div class="truncate font-mono text-surface-500" title={entry.home.configDir}>{entry.home.configDir}</div>
          {#if entry.error}<p class="mt-1 text-error-500">{entry.error}</p>{/if}
        </div>
        {#if entry.home.isDefault}<span class="text-surface-500">Default</span>
        {:else}<button
            class="btn btn-sm preset-tonal"
            disabled={action.busy}
            onclick={() => action.run(() => chooseDefaultHome(entry.home.id))}>Make default</button
          >{/if}
        <button
          class="btn btn-sm preset-tonal"
          data-testid="home-edit"
          disabled={action.busy}
          onclick={() => openEditor(entry.home.id, entry.home.configDir, entry.home.binary, entry.home.label)}
          >{editing === entry.home.id ? "Cancel" : "Edit"}</button
        >
        <button
          class="btn btn-sm {confirming === entry.home.id ? 'preset-filled-error-500' : 'preset-tonal'}"
          data-testid="home-remove"
          disabled={action.busy}
          onclick={() => {
            action.error = null;
            if (confirming === entry.home.id) void remove(entry.home.id);
            else {
              editing = null;
              confirming = entry.home.id;
            }
          }}>{confirming === entry.home.id ? "Confirm remove" : "Remove"}</button
        >
      </div>
      {#if action.error && (editing === entry.home.id || confirming === entry.home.id)}
        <p role="alert" class="text-error-500">{action.error}</p>
      {/if}
      {#if confirming === entry.home.id}
        <p class="text-[11px] leading-4 text-surface-500">
          Conversations recorded under this Home stop being listed. Nothing is deleted from disk.
        </p>
      {/if}
      {#if editing === entry.home.id}
        <form
          class="space-y-2 border-t border-surface-200-800 pt-2"
          onsubmit={(event) => {
            event.preventDefault();
            void saveEdit(entry.home.id);
          }}
        >
          {@render fields(
            editLabel,
            editDir,
            editBinary,
            (value) => (editLabel = value),
            (value) => (editDir = value),
            (value) => (editBinary = value),
          )}
          <button class="btn btn-sm preset-filled-primary-500" type="submit" disabled={action.busy}
            >{action.busy ? "Saving…" : "Save Home"}</button
          >
        </form>
      {/if}
    </div>
  {/each}
  <form
    class="space-y-2"
    onsubmit={(event) => {
      event.preventDefault();
      void add();
    }}
  >
    <div class="grid grid-cols-2 gap-2">
      <label class="label text-xs"
        ><span>Harness</span
        ><select
          class="select"
          bind:value={harness}
          onchange={() => {
            configDir = harness === "codex" ? "~/.codex" : "~/.claude";
            binary = harness;
          }}><option value="codex">Codex</option><option value="claude">Claude Code</option></select
        ></label
      >
      <label class="label text-xs"
        ><span>Host</span
        ><HostSelect bind:value={host} /></label
      >
    </div>
    {@render fields(
      label,
      configDir,
      binary,
      (value) => (label = value),
      (value) => (configDir = value),
      (value) => (binary = value),
    )}
    {#if action.error && !editing && !confirming}<p role="alert" class="text-xs text-error-500">{action.error}</p>{/if}
    <button class="btn btn-sm preset-filled-primary-500" type="submit" disabled={action.busy || !isTauri()}
      >{action.busy ? "Adding…" : "Add Home"}</button
    >
  </form>
</div>
