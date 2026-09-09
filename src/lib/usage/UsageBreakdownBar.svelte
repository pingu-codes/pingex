<!--
  One stacked bar and its legend: tokens by category. Estimated segments are
  marked "≈" in the legend and explained once in the footnote, so a reader
  knows which figures the harness reported and which the app worked out from
  message sizes.
-->
<script lang="ts">
import { formatTokens } from "$lib/thread/contextUsage";
import type { CategoryTokens } from "$lib/types";
import { approx, segmentsFor } from "$lib/usage/usageBreakdown";

let {
  categories,
  exact = false,
  label,
  footnote = true,
}: {
  categories: CategoryTokens;
  /** Every figure was reported by the harness; nothing is estimated. */
  exact?: boolean;
  /** Accessible name for the bar. */
  label: string;
  /** Show the "≈ estimated" explanation when any segment is an estimate. */
  footnote?: boolean;
} = $props();

const segments = $derived(segmentsFor(categories, { exact }));
const anyEstimated = $derived(segments.some((segment) => segment.estimated));
</script>

{#if segments.length === 0}
  <p class="text-xs text-surface-500">Nothing recorded yet.</p>
{:else}
  <div
    class="flex h-2 w-full overflow-hidden rounded-full bg-surface-200-800"
    role="img"
    aria-label={label}
  >
    {#each segments as segment (segment.id)}
      <div
        class="h-full {segment.color}"
        style="width: {segment.percent}%"
        title="{segment.label}: {formatTokens(segment.tokens)} tokens"
      ></div>
    {/each}
  </div>
  <dl class="mt-2 space-y-1">
    {#each segments as segment (segment.id)}
      <div class="flex items-baseline justify-between gap-3">
        <dt class="flex min-w-0 items-center gap-1.5 text-surface-600-400">
          <span class="inline-block size-2 shrink-0 rounded-sm {segment.color}" aria-hidden="true"></span>
          <span class="truncate">{segment.label}</span>
        </dt>
        <dd class="shrink-0 font-mono tabular-nums">
          {approx(formatTokens(segment.tokens), segment.estimated)}
          <span class="text-surface-500">· {Math.round(segment.percent)}%</span>
        </dd>
      </div>
    {/each}
  </dl>
  {#if footnote && anyEstimated}
    <p class="mt-1.5 text-[10px] leading-4 text-surface-500">
      ≈ estimated from message sizes (about 4 characters per token); replies and reasoning are as reported.
    </p>
  {/if}
{/if}
