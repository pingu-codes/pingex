<!--
  The review page's Checks tab: how the head's checks stand and, when some
  fail, their logs and a re-run. The page holds the logs, which outlive the tab.
-->
<script lang="ts">
  import CheckLogList from "$lib/inbox/CheckLogList.svelte";
  import { checksFailing, checksLabel } from "$lib/review/review";
  import type { CheckLog, PrDetail } from "$lib/types";

  let {
    checks,
    logs,
    busy,
    onLoadLogs,
    onRerun,
  }: {
    checks: PrDetail["checks"];
    logs: CheckLog[] | null;
    busy: boolean;
    onLoadLogs: () => void;
    onRerun: () => void;
  } = $props();
</script>

<div class="flex max-w-3xl flex-col gap-3">
  <div class="flex items-center gap-2">
    <span class={checksFailing(checks) ? "text-error-500" : "text-surface-500"}>
      {checksLabel(checks) ?? "No checks on this head."}
    </span>
    <span class="grow"></span>
    {#if checksFailing(checks)}
      <button type="button" class="btn btn-sm hover:preset-tonal text-[11px]" disabled={busy} onclick={onLoadLogs}>
        View failing logs
      </button>
      <button type="button" class="btn btn-sm preset-tonal text-[11px]" disabled={busy} onclick={onRerun}>
        Re-run failed checks
      </button>
    {/if}
  </div>
  {#if logs}
    <CheckLogList {logs} />
  {/if}
</div>
