<!--
  Cumulative token spend for one scope — a thread, a project, or the whole
  Home — by category, by model and (above thread scope) by thread. Reads the
  ledger through `readUsageBreakdown` and re-reads, briefly debounced, when a
  usage report arrives; the aggregate can change on any thread's turn.
-->
<script lang="ts">
import { onMount } from "svelte";
import { openThreadById } from "$lib/app/navigation.svelte";
import { readUsageBreakdown } from "$lib/services/api";
import { usageStatus } from "$lib/services/codexEvents.svelte";
import { formatTokens, formatTokensShort } from "$lib/thread/contextUsage";
import type { UsageBreakdown, UsageScope } from "$lib/types";
import UsageBreakdownBar from "$lib/usage/UsageBreakdownBar.svelte";
import { costFor, costLabel, rangeSince, USAGE_RANGES, type UsageRange } from "$lib/usage/usageBreakdown";
import { relativeTime } from "$lib/utils/time";

let {
  scope,
  compact = false,
}: {
  scope: UsageScope;
  /** Tight layout for the right panel: no range control, no thread list. */
  compact?: boolean;
} = $props();

let range = $state<UsageRange>("all");
let breakdown = $state<UsageBreakdown | null>(null);
let error = $state<string | null>(null);
let loading = $state(true);
let refreshTimer: ReturnType<typeof setTimeout> | null = null;
let generation = 0;

async function load() {
  const mine = ++generation;
  try {
    const next = await readUsageBreakdown(scope, rangeSince(range));
    if (mine !== generation) return;
    breakdown = next;
    error = null;
  } catch (cause) {
    if (mine !== generation) return;
    error = cause instanceof Error ? cause.message : String(cause);
  } finally {
    if (mine === generation) loading = false;
  }
}

onMount(() => {
  void load();
  return () => {
    if (refreshTimer) clearTimeout(refreshTimer);
  };
});

// A new usage report anywhere may move these figures; coalesce a burst of
// per-request reports into one read.
$effect(() => {
  usageStatus.nonce;
  if (usageStatus.nonce === 0) return;
  if (refreshTimer) clearTimeout(refreshTimer);
  refreshTimer = setTimeout(() => void load(), 1000);
});

function selectRange(next: UsageRange) {
  if (next === range) return;
  range = next;
  loading = true;
  void load();
}

const cost = $derived(breakdown ? costFor(breakdown.byModel, breakdown.reportedCostUsd) : null);

const tokenRows = $derived(
  breakdown
    ? [
        { label: "Input", value: formatTokens(breakdown.tokens.inputTokens) },
        { label: "Cached input", value: formatTokens(breakdown.tokens.cachedInputTokens) },
        ...(breakdown.tokens.cacheWriteInputTokens > 0
          ? [{ label: "Cache writes", value: formatTokens(breakdown.tokens.cacheWriteInputTokens) }]
          : []),
        { label: "Output", value: formatTokens(breakdown.tokens.outputTokens) },
        { label: "Reasoning", value: formatTokens(breakdown.tokens.reasoningOutputTokens) },
      ]
    : [],
);

function modelName(model: string | null): string {
  return model ?? "unknown model";
}

function groupCost(group: UsageBreakdown["byModel"][number]): string | null {
  return costLabel(costFor([group], group.costUsd));
}
</script>

<div class="space-y-4 text-xs" data-testid="usage-summary">
  {#if !compact}
    <div class="flex items-center gap-1" role="group" aria-label="Usage time range">
      {#each USAGE_RANGES as option (option.id)}
        <button
          type="button"
          class="rounded-full px-2.5 py-1 text-[11px] {range === option.id ? 'preset-filled-primary-500' : 'preset-tonal'}"
          aria-pressed={range === option.id}
          onclick={() => selectRange(option.id)}
        >
          {option.label}
        </button>
      {/each}
    </div>
  {/if}

  {#if loading && !breakdown}
    <p class="text-surface-500">Reading usage…</p>
  {:else if error}
    <p class="text-error-500">Could not read usage: {error}</p>
  {:else if breakdown && breakdown.turns === 0}
    <p class="text-surface-500">
      No usage recorded yet. Turns are recorded from the moment they run; older threads count from their next
      turn.
    </p>
  {:else if breakdown}
    <section>
      <h3 class="mb-1.5 flex items-baseline justify-between gap-3 text-[10px] font-semibold uppercase tracking-wide text-surface-500">
        <span>Spend by category</span>
        <span class="font-mono normal-case tracking-normal">{formatTokensShort(breakdown.tokens.totalTokens)} tokens</span>
      </h3>
      <UsageBreakdownBar categories={breakdown.categories} label="Tokens spent by category" />
    </section>

    <section>
      <h3 class="mb-1.5 text-[10px] font-semibold uppercase tracking-wide text-surface-500">Totals</h3>
      <dl class="space-y-1">
        {#each tokenRows as row (row.label)}
          <div class="flex items-baseline justify-between gap-3">
            <dt class="text-surface-600-400">{row.label}</dt>
            <dd class="font-mono tabular-nums">{row.value}</dd>
          </div>
        {/each}
        <div class="flex items-baseline justify-between gap-3">
          <dt class="text-surface-600-400">Turns</dt>
          <dd class="font-mono tabular-nums">{breakdown.turns}</dd>
        </div>
        {#if cost}
          <div class="flex items-baseline justify-between gap-3 border-t border-surface-200-800 pt-1">
            <dt class="text-surface-600-400">{cost.estimated ? "Estimated cost" : "Cost"}</dt>
            <dd class="font-mono tabular-nums" data-testid="usage-cost">{costLabel(cost)}</dd>
          </div>
        {/if}
      </dl>
    </section>

    {#if breakdown.byModel.length > 0}
      <section>
        <h3 class="mb-1.5 text-[10px] font-semibold uppercase tracking-wide text-surface-500">By model</h3>
        <dl class="space-y-1">
          {#each breakdown.byModel as group, index (`${group.harness}:${group.model ?? ""}:${index}`)}
            <div class="flex items-baseline justify-between gap-3">
              <dt class="min-w-0 truncate text-surface-600-400" title={modelName(group.model)}>
                {modelName(group.model)}
                <span class="text-surface-500">· {group.harness}</span>
              </dt>
              <dd class="shrink-0 font-mono tabular-nums">
                {formatTokensShort(group.tokens.totalTokens)}{groupCost(group) ? ` · ${groupCost(group)}` : ""}
              </dd>
            </div>
          {/each}
        </dl>
      </section>
    {/if}

    {#if !compact && breakdown.byThread.length > 0}
      <section>
        <h3 class="mb-1.5 text-[10px] font-semibold uppercase tracking-wide text-surface-500">Heaviest threads</h3>
        <ul class="space-y-1">
          {#each breakdown.byThread as row (row.threadId)}
            <li>
              <button
                type="button"
                class="flex w-full items-baseline justify-between gap-3 rounded px-1 py-0.5 text-left hover:preset-tonal"
                onclick={() => openThreadById(row.threadId)}
              >
                <span class="min-w-0 flex-1 truncate">{row.title ?? "Deleted thread"}</span>
                <span class="shrink-0 text-surface-500">{relativeTime(row.lastAt)}</span>
                <span class="shrink-0 font-mono tabular-nums">{formatTokensShort(row.tokens.totalTokens)}</span>
              </button>
            </li>
          {/each}
        </ul>
      </section>
    {/if}
  {/if}
</div>
