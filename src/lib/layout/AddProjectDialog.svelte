<script lang="ts">
import { open } from "@tauri-apps/plugin-dialog";
import { onMount } from "svelte";
import { type DialogClose, submitState } from "$lib/app/dialogs.svelte";
import { hostFromOption } from "$lib/app/host";
import DialogShell from "$lib/components/DialogShell.svelte";
import { listWslDistros } from "$lib/services/api";
import type { Host } from "$lib/types";

let { submit, close }: { submit: (path: string, host: Host | null) => Promise<void>; close: DialogClose<true> } =
  $props();
let path = $state("");
let selectedHost = $state("native");
let distros = $state<string[]>([]);
let explicit = $state(false);
const action = submitState();

onMount(() => {
  listWslDistros()
    .then((value) => {
      distros = value;
    })
    .catch(() => {});
});

async function browse() {
  const value = await open({ directory: true, multiple: false, title: "Choose a project folder" });
  if (typeof value !== "string") return;
  path = value;
  if (!explicit) {
    const match = /^\\\\(?:wsl\.localhost|wsl\$)\\([^\\]+)/i.exec(value);
    selectedHost = match?.[1] ?? "native";
    if (match && !distros.includes(match[1])) distros = [...distros, match[1]];
  }
}

async function add() {
  if (!path.trim() || action.busy) return;
  if (await action.run(() => submit(path.trim(), explicit ? hostFromOption(selectedHost) : null))) close(true);
}
</script>

<DialogShell title="Add project" onClose={() => close()}>
  <form id="add-project-form" onsubmit={(event) => { event.preventDefault(); void add(); }} class="mt-4 flex flex-col gap-4">
    <label class="label">
      <span>Run on</span>
      <select class="select" bind:value={selectedHost} onchange={() => { explicit = true; }}>
        <option value="native">This computer</option>
        {#each distros as distro}<option value={distro}>WSL · {distro}</option>{/each}
      </select>
    </label>
    <label class="label">
      <span>Project folder</span>
      <div class="flex gap-2">
        <input class="input flex-1" bind:value={path} placeholder={selectedHost === "native" ? "Choose a folder" : "/home/you/project"} />
        <button class="btn preset-tonal" type="button" onclick={browse}>Browse</button>
      </div>
    </label>
    <p class="text-xs text-surface-500">Codex, Claude Code and Git will run on this project's Host.</p>
    {#if action.error}<p role="alert" class="text-sm text-error-500">{action.error}</p>{/if}
  </form>
  {#snippet footer()}
    <button class="btn preset-tonal" onclick={() => close()}>Cancel</button>
    <button class="btn preset-filled-primary-500" form="add-project-form" type="submit" disabled={!path.trim() || action.busy}>
      {action.busy ? "Adding…" : "Add project"}
    </button>
  {/snippet}
</DialogShell>
