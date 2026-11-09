<!-- A PR's failing check logs, as GitHub has them at its head. -->
<script lang="ts">
  import type { CheckLog } from "$lib/types";

  let { logs }: { logs: CheckLog[] } = $props();
</script>

{#each logs as log}
  <div class="panel-card overflow-hidden">
    <div class="file-head font-mono text-[11px] font-semibold">{log.name}</div>
    <pre class="max-h-80 overflow-auto p-2 font-mono text-[11px] whitespace-pre-wrap">{log.log}</pre>
    {#if log.truncated}
      <p class="px-2 pb-2 text-xs text-surface-500">Only the end of this log is shown.</p>
    {/if}
  </div>
{:else}
  <p class="text-xs text-surface-500">No failing check logs at this head.</p>
{/each}
