<!--
  What `/status` answers: how much context this thread is using, what it has
  cost, and how much account quota is left.

  Everything here is already in the app — the context ring reads the same
  `ContextStats`, the sidebar reads the same rate-limit snapshot. This gathers
  it into one place so the question can be asked directly rather than inferred
  from three widgets.
-->
<script lang="ts">
import { accountUsage } from "$lib/services/accountUsage.svelte";
import { readThreadUsage } from "$lib/services/api";
import { type ContextStats, formatTokens } from "$lib/thread/contextUsage";
import type { ThreadUsage } from "$lib/types";
import { usageWindows } from "$lib/utils/rateLimits";

let {
  stats,
  costUsd = null,
  model = null,
  threadId = null,
}: {
  stats: ContextStats | null;
  /** Estimated spend for this thread, or null when the model has no pricing. */
  costUsd?: number | null;
  model?: string | null;
  /** Thread to read the server-side usage estimate for. */
  threadId?: string | null;
} = $props();

const windows = $derived(usageWindows(accountUsage.snapshot));

/** Server-side usage estimate; a billing read, so refreshed at most every 15s. */
let threadUsage = $state<ThreadUsage | null>(null);
let usageFetchedAt = 0;
let usageFetchedFor: string | null = null;
const USAGE_REFRESH_MS = 15_000;

$effect(() => {
  const id = threadId;
  stats; // Re-run when token figures move — that is when usage can change.
  if (!id) {
    threadUsage = null;
    usageFetchedFor = null;
    return;
  }
  const now = Date.now();
  if (usageFetchedFor === id && now - usageFetchedAt < USAGE_REFRESH_MS) return;
  usageFetchedAt = now;
  usageFetchedFor = id;
  readThreadUsage(id)
    .then((usage) => {
      if (threadId === id) threadUsage = usage;
    })
    .catch(() => {});
});

function formatCredits(micros: number): string {
  const credits = micros / 1e6;
  return credits.toFixed(credits < 1 ? 3 : 2);
}

function formatUsdMicros(micros: number): string {
  const usd = micros / 1e6;
  return `$${usd.toFixed(usd < 1 ? 3 : 2)}`;
}

const contextRows = $derived(
  stats
    ? [
        { label: "In context now", value: formatTokens(stats.usedTokens) },
        {
          label: "Context window",
          value: stats.contextWindow ? formatTokens(stats.contextWindow) : "unknown",
        },
        {
          label: "Remaining",
          value: stats.percentRemaining === null ? "unknown" : `${stats.percentRemaining}%`,
        },
      ]
    : [],
);

const sessionRows = $derived(
  stats
    ? [
        { label: "Total", value: formatTokens(stats.sessionTotalTokens) },
        { label: "Input", value: formatTokens(stats.sessionInputTokens) },
        { label: "Cached input", value: formatTokens(stats.sessionCachedInputTokens) },
        { label: "Output", value: formatTokens(stats.sessionOutputTokens) },
        { label: "Reasoning", value: formatTokens(stats.sessionReasoningTokens) },
      ]
    : [],
);
</script>

{#if !stats && !threadUsage}
  <p class="text-xs text-surface-500">
    No usage reported yet — Codex sends these figures once this thread runs a turn.
  </p>
{:else}
  <div class="space-y-4 text-xs">
    {#if stats}
      <section>
        <h3 class="mb-1.5 text-[10px] font-semibold uppercase tracking-wide text-surface-500">Context</h3>
        <dl class="space-y-1">
          {#each contextRows as row (row.label)}
            <div class="flex items-baseline justify-between gap-3">
              <dt class="text-surface-600-400">{row.label}</dt>
              <dd class="font-mono">{row.value}</dd>
            </div>
          {/each}
        </dl>
      </section>

      <section>
        <h3 class="mb-1.5 text-[10px] font-semibold uppercase tracking-wide text-surface-500">
          This thread{model ? ` · ${model}` : ""}
        </h3>
        <dl class="space-y-1">
          {#each sessionRows as row (row.label)}
            <div class="flex items-baseline justify-between gap-3">
              <dt class="text-surface-600-400">{row.label}</dt>
              <dd class="font-mono">{row.value}</dd>
            </div>
          {/each}
          {#if costUsd !== null}
            <div class="flex items-baseline justify-between gap-3 border-t border-surface-200-800 pt-1">
              <dt class="text-surface-600-400">Estimated cost</dt>
              <dd class="font-mono">${costUsd.toFixed(costUsd < 1 ? 3 : 2)}</dd>
            </div>
          {/if}
        </dl>
      </section>
    {/if}

    {#if threadUsage}
      <section>
        <h3 class="mb-1.5 text-[10px] font-semibold uppercase tracking-wide text-surface-500">Estimated usage</h3>
        <dl class="space-y-1">
          <div class="flex items-baseline justify-between gap-3">
            <dt class="text-surface-600-400">Credits</dt>
            <dd class="font-mono">{formatCredits(threadUsage.estimatedUsageCreditsMicros)}</dd>
          </div>
          {#if threadUsage.estimatedUsageUsdMicros != null}
            <div class="flex items-baseline justify-between gap-3">
              <dt class="text-surface-600-400">Estimated spend</dt>
              <dd class="font-mono">{formatUsdMicros(threadUsage.estimatedUsageUsdMicros)}</dd>
            </div>
          {/if}
          {#each threadUsage.groups as group, index (index)}
            <div class="flex items-baseline justify-between gap-3 border-t border-surface-200-800 pt-1">
              <dt class="min-w-0 truncate text-surface-600-400">
                {group.model ?? "unknown"}{group.reasoningEffort ? ` · ${group.reasoningEffort}` : ""}
              </dt>
              <dd class="shrink-0 font-mono">
                {group.totalTokens != null ? `${formatTokens(group.totalTokens)} · ` : ""}{formatCredits(
                  group.estimatedUsageCreditsMicros,
                )} cr
              </dd>
            </div>
          {/each}
        </dl>
      </section>
    {/if}

    <section>
      <h3 class="mb-1.5 text-[10px] font-semibold uppercase tracking-wide text-surface-500">Account limits</h3>
      {#if accountUsage.error}
        <p class="text-surface-500">Could not read usage limits.</p>
      {:else if windows.length === 0}
        <p class="text-surface-500">No limits reported for this account.</p>
      {:else}
        <dl class="space-y-1.5">
          {#each windows as window (window.label)}
            <div>
              <div class="flex items-baseline justify-between gap-3">
                <dt class="text-surface-600-400">{window.label}</dt>
                <dd class="font-mono">{window.remainingPercent}% left</dd>
              </div>
              <div class="mt-1 h-1 overflow-hidden rounded-full bg-surface-200-800">
                <div
                  class="h-full rounded-full {window.usedPercent >= 90
                    ? 'bg-error-500'
                    : window.usedPercent >= 70
                      ? 'bg-warning-500'
                      : 'bg-primary-500'}"
                  style="width: {window.usedPercent}%"
                ></div>
              </div>
            </div>
          {/each}
        </dl>
      {/if}
    </section>
  </div>
{/if}
