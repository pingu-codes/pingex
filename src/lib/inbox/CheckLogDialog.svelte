<!-- The logs of a PR's failing checks, as GitHub has them at its head. -->
<script lang="ts">
  import type { DialogClose } from "$lib/app/dialogs.svelte";
  import DialogShell from "$lib/components/DialogShell.svelte";
  import type { CheckLog } from "$lib/types";

  let { title, logs, close }: { title: string; logs: CheckLog[]; close: DialogClose<never> } = $props();
</script>

<DialogShell {title} width={720} onClose={() => close()}>
  {#each logs as log (log.name)}
    <h3 class="eyebrow mt-2">{log.name}</h3>
    <pre class="code-dark mt-1 max-h-80 overflow-auto rounded-md p-3 text-[11px] leading-4">{log.log}</pre>
    {#if log.truncated}
      <p class="mt-1 text-xs text-surface-500">Only the end of this log is shown.</p>
    {/if}
  {:else}
    <p class="text-xs text-surface-500">No failing check logs at this head.</p>
  {/each}
</DialogShell>
