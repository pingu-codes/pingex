<script lang="ts">
import { onMount } from "svelte";
import { submitState } from "$lib/app/dialogs.svelte";
import { hostFromOption } from "$lib/app/host";
import { commands, type HarnessKind } from "$lib/bindings";
import { chooseDefaultHome, loadProfile, profileHomes } from "$lib/services/homeRouting";
import { isTauri } from "$lib/services/tauri";

let harness = $state<HarnessKind>("codex");
let host = $state("native");
let configDir = $state("~/.codex");
let binary = $state("codex");
let label = $state("");
let distros = $state<string[]>([]);
const action = submitState();
const homes = $derived(profileHomes());
onMount(() => {
  if (isTauri())
    commands
      .listWslDistros()
      .then((value) => {
        distros = value;
      })
      .catch(() => {});
});

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

<div class="mt-5 space-y-3 border-t border-surface-200-800 pt-4">
  <h3 class="text-sm font-medium">Homes in this Profile</h3>
  <p class="text-xs text-surface-500">Each Host has its own Codex and Claude Code defaults. Existing conversations keep their Home.</p>
  {#each homes as entry (entry.home.id)}
    <div class="flex items-center gap-3 rounded border border-surface-200-800 p-2 text-xs">
      <div class="min-w-0 flex-1">
        <div>{entry.home.label} · {entry.home.host.kind === "wsl" ? `WSL · ${entry.home.host.distro}` : "This computer"}</div>
        <div class="truncate font-mono text-surface-500" title={entry.home.configDir}>{entry.home.configDir}</div>
        {#if entry.error}<p class="mt-1 text-error-500">{entry.error}</p>{/if}
      </div>
      {#if entry.home.isDefault}<span class="text-surface-500">Default</span>
      {:else}<button class="btn btn-sm preset-tonal" disabled={action.busy} onclick={() => action.run(() => chooseDefaultHome(entry.home.id))}>Make default</button>{/if}
    </div>
  {/each}
  <form class="space-y-2" onsubmit={(event) => { event.preventDefault(); void add(); }}>
    <div class="grid grid-cols-2 gap-2">
      <label class="label text-xs"><span>Harness</span><select class="select" bind:value={harness} onchange={() => { configDir = harness === "codex" ? "~/.codex" : "~/.claude"; binary = harness; }}><option value="codex">Codex</option><option value="claude">Claude Code</option></select></label>
      <label class="label text-xs"><span>Host</span><select class="select" bind:value={host}><option value="native">This computer</option>{#each distros as distro}<option value={distro}>WSL · {distro}</option>{/each}</select></label>
    </div>
    <label class="label text-xs"><span>Name</span><input class="input" bind:value={label} placeholder="Optional label, for example Work" /></label>
    <label class="label text-xs"><span>Configuration directory</span><input class="input font-mono" bind:value={configDir} required /></label>
    <label class="label text-xs"><span>CLI executable</span><input class="input font-mono" bind:value={binary} required /></label>
    {#if action.error}<p role="alert" class="text-xs text-error-500">{action.error}</p>{/if}
    <button class="btn btn-sm preset-filled-primary-500" type="submit" disabled={action.busy || !isTauri()}>{action.busy ? "Adding…" : "Add Home"}</button>
  </form>
</div>
