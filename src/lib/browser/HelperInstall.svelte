<script lang="ts">
  // Puts the Pingex Browser Helper (built into the app) in Pingex's app-data
  // folder and shows it, ready for Chrome's "Load unpacked".
  import { Copy, FolderOpen } from "@lucide/svelte";
  import { commands } from "$lib/services/homeRouting";
  import { submitState } from "$lib/utils/actionState.svelte";

  let { dir = null }: { dir?: string | null } = $props();

  const action = submitState();
  let installed = $state<string | null>(null);
  const shown = $derived(installed ?? dir);

  const install = () =>
    action.run(async () => {
      installed = await commands.browserInstallHelper(true);
    });
</script>

<div class="space-y-1.5">
  {#if action.error}
    <p role="alert" class="text-error-500">{action.error}</p>
  {/if}
  <button class="btn btn-sm preset-tonal" disabled={action.busy} onclick={install}>
    <FolderOpen size={12} />
    {installed ? "Show helper folder" : "Install helper"}
  </button>
  <p class="text-surface-600-400">
    Then in <span class="font-mono">chrome://extensions</span> turn on Developer mode, click Load unpacked and choose
    {#if shown}
      <button
        class="inline-flex items-center gap-1 font-mono text-primary-500 hover:underline"
        title="Copy path"
        onclick={() => commands.copyText(shown)}
      >
        {shown}
        <Copy size={10} />
      </button>
    {:else}
      that folder
    {/if}
  </p>
</div>
