<!--
  One stacked bar and its legend: tokens by row. Estimated rows are marked
  "≈" in the legend and explained once in the footnote, so a reader knows
  which figures the harness reported and which the app worked out.
-->
<script lang="ts">
  import { approx, type BarRow } from "$lib/usage/usageBreakdown";
  import { formatTokens } from "$lib/utils/format";

  let {
    rows,
    label,
    thin = false,
    footnote = "≈ estimated from message sizes (about 4 characters per token); replies and reasoning are as reported.",
  }: {
    rows: BarRow[];
    /** Accessible name for the bar. */
    label: string;
    /** A slimmer bar, for a breakdown nested under another. */
    thin?: boolean;
    /** Explains the "≈" when any row is an estimate; null hides it. */
    footnote?: string | null;
  } = $props();

  const anyEstimated = $derived(rows.some((row) => row.estimated));
</script>

{#if rows.length === 0}
  <p class="text-xs text-surface-500">Nothing recorded yet.</p>
{:else}
  <div
    class="flex w-full overflow-hidden rounded-full bg-surface-200-800 {thin ? "h-1.5" : "h-2"}"
    role="img"
    aria-label={label}
  >
    {#each rows as row (row.key)}
      <div
        class="h-full {row.color}"
        style="width: {row.width}%"
        title="{row.label}: {formatTokens(row.tokens)} tokens"
      ></div>
    {/each}
  </div>
  <dl class="mt-2 space-y-1">
    {#each rows as row (row.key)}
      <div class="flex items-baseline justify-between gap-3">
        <dt class="flex min-w-0 items-center gap-1.5 text-surface-600-400">
          <span class="inline-block size-2 shrink-0 rounded-sm {row.color}" aria-hidden="true"></span>
          <span class="truncate" title={row.detail ?? row.label}>
            {row.label}
            {#if row.detail}
              <span class="text-surface-500"> · {row.detail}</span>
            {/if}
          </span>
        </dt>
        <dd class="shrink-0 font-mono tabular-nums">
          {approx(formatTokens(row.tokens), row.estimated)}
          <span class="text-surface-500">· {Math.round(row.percent)}%</span>
        </dd>
      </div>
    {/each}
  </dl>
  {#if footnote && anyEstimated}
    <p class="mt-1.5 text-[10px] leading-4 text-surface-500">{footnote}</p>
  {/if}
{/if}
