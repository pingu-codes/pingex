<script lang="ts">
  import type { DialogClose } from "$lib/app/dialogs.svelte";
  import { hostFromOption } from "$lib/app/host";
  import DialogShell from "$lib/components/DialogShell.svelte";
  import DirectoryField from "$lib/components/DirectoryField.svelte";
  import HostSelect from "$lib/components/HostSelect.svelte";
  import type { Host } from "$lib/types";
  import { submitState } from "$lib/utils/actionState.svelte";

  let { submit, close }: { submit: (path: string, host: Host | null) => Promise<void>; close: DialogClose<true> } =
    $props();
  let path = $state("");
  let selectedHost = $state("native");
  let explicit = $state(false);
  const action = submitState();

  /** A folder picked inside a distribution (`\\wsl.localhost\<distro>\…`) runs there unless a host was chosen. */
  function picked(value: string) {
    path = value;
    if (!explicit) selectedHost = /^\\\\(?:wsl\.localhost|wsl\$)\\([^\\]+)/i.exec(value)?.[1] ?? "native";
  }

  async function add() {
    if (!path.trim() || action.busy) return;
    if (await action.run(() => submit(path.trim(), explicit ? hostFromOption(selectedHost) : null))) close(true);
  }
</script>

<DialogShell title="Add project" onClose={() => close()}>
  <form
    id="add-project-form"
    onsubmit={(event) => {
      event.preventDefault();
      void add();
    }}
    class="mt-4 flex flex-col gap-4"
  >
    <label class="label">
      <span>Run on</span>
      <HostSelect
        bind:value={selectedHost}
        onchange={() => {
          explicit = true;
        }}
      />
    </label>
    <div class="label">
      <label for="add-project-path">Project folder</label>
      <DirectoryField
        id="add-project-path"
        value={path}
        onInput={(next) => (path = next)}
        onPick={picked}
        placeholder={selectedHost === "native" ? "Choose a folder" : "/home/you/project"}
        browseLabel="Browse for a project folder"
        pickerTitle="Choose a project folder"
        inputClass="input"
        buttonSize="btn-base"
      />
    </div>
    <p class="text-xs text-surface-500">Codex, Claude Code and Git will run on this project's Host.</p>
    {#if action.error}
      <p role="alert" class="text-sm text-error-500">{action.error}</p>
    {/if}
  </form>
  {#snippet footer()}
    <button class="btn preset-tonal" onclick={() => close()}>Cancel</button>
    <button
      class="btn preset-filled-primary-500"
      form="add-project-form"
      type="submit"
      disabled={!path.trim() || action.busy}
    >
      {action.busy ? "Adding…" : "Add project"}
    </button>
  {/snippet}
</DialogShell>
