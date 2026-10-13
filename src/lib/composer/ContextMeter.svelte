<script lang="ts">
import TooltipAnchor from "$lib/components/TooltipAnchor.svelte";
import type { ContextStats } from "$lib/thread/contextUsage";
import { formatTokens, formatTokensShort } from "$lib/utils/format";

let {
  stats,
  compacting = false,
  busy = false,
  onCompact,
}: {
  stats: ContextStats | null;
  /** A compaction turn is running, so the ring shows a pending state. */
  compacting?: boolean;
  /** Another turn is running, so compaction has to wait. */
  busy?: boolean;
  onCompact?: () => void;
} = $props();

const RADIUS = 8;
const CIRCUMFERENCE = 2 * Math.PI * RADIUS;

// Unknown context windows still get a ring: fall back to an empty one rather
// than hiding the control, so the popover stays reachable for raw counts.
const fraction = $derived(Math.min(Math.max(stats?.usedFraction ?? 0, 0), 1));
const percentUsed = $derived(stats?.percentUsed ?? null);
const ringClass = $derived(
  fraction >= 0.9 ? "stroke-error-500" : fraction >= 0.7 ? "stroke-warning-500" : "stroke-primary-500",
);
// aria-disabled rather than disabled, so the stats stay reachable by hover and
// keyboard while compaction is unavailable.
const unavailable = $derived(!onCompact || compacting || busy);
const label = $derived(
  percentUsed === null
    ? `Context: ${formatTokensShort(stats?.usedTokens ?? 0)} tokens used`
    : `Context: ${percentUsed}% used`,
);
</script>

{#snippet details(stats: ContextStats)}
  <div class="flex items-baseline justify-between gap-2">
    <span class="eyebrow text-[10px]">Context</span>
    {#if percentUsed !== null}
      <span class="text-xs font-medium">{percentUsed}% used</span>
    {/if}
  </div>
  <dl class="mt-1.5 space-y-1 text-[11px] leading-4">
    <div class="flex justify-between gap-3">
      <dt class="text-surface-500">In context</dt>
      <dd class="font-mono">{formatTokens(stats.usedTokens)}</dd>
    </div>
    <div class="flex justify-between gap-3">
      <dt class="text-surface-500">Context window</dt>
      <dd class="font-mono">{stats.contextWindow === null ? "unknown" : formatTokens(stats.contextWindow)}</dd>
    </div>
    {#if stats.percentRemaining !== null}
      <div class="flex justify-between gap-3">
        <dt class="text-surface-500">Left</dt>
        <dd class="font-mono">{stats.percentRemaining}%</dd>
      </div>
    {/if}
  </dl>
  <hr class="my-2 border-surface-200-800" />
  <dl class="space-y-1 text-[11px] leading-4">
    <div class="flex justify-between gap-3">
      <dt class="text-surface-500">Session total</dt>
      <dd class="font-mono">{formatTokens(stats.sessionTotalTokens)}</dd>
    </div>
    <div class="flex justify-between gap-3">
      <dt class="text-surface-500">Input</dt>
      <dd class="font-mono">{formatTokens(stats.sessionInputTokens)}</dd>
    </div>
    <div class="flex justify-between gap-3">
      <dt class="text-surface-500">Cached input</dt>
      <dd class="font-mono">{formatTokens(stats.sessionCachedInputTokens)}</dd>
    </div>
    <div class="flex justify-between gap-3">
      <dt class="text-surface-500">Output</dt>
      <dd class="font-mono">{formatTokens(stats.sessionOutputTokens)}</dd>
    </div>
    <div class="flex justify-between gap-3">
      <dt class="text-surface-500">Reasoning</dt>
      <dd class="font-mono">{formatTokens(stats.sessionReasoningTokens)}</dd>
    </div>
  </dl>
  {#if onCompact}
    <p class="mt-2 text-[10px] leading-4 text-surface-500">
      {compacting
        ? "Compacting…"
        : busy
          ? "Compact once this turn finishes, with /compact."
          : "Click to compact, or type /compact."}
    </p>
  {/if}
{/snippet}

{#if stats}
  <TooltipAnchor
    tag="button"
    {label}
    aria-label={label}
    aria-disabled={unavailable}
    openDelay={0}
    placement="top-end"
    contentClass="popover-card w-[248px] select-none p-2.5"
    onclick={(event) => {
      event.stopPropagation();
      if (!unavailable) onCompact?.();
    }}
    class="grid size-7 place-items-center rounded-full text-surface-500 transition hover:bg-surface-200-800 aria-disabled:cursor-default aria-disabled:hover:bg-transparent"
  >
    {#snippet content()}{@render details(stats)}{/snippet}
    <svg width="20" height="20" viewBox="0 0 20 20" class={compacting ? "animate-pulse" : ""} aria-hidden="true">
      <circle
        cx="10"
        cy="10"
        r={RADIUS}
        fill="none"
        stroke-width="2.5"
        class="stroke-surface-300-700"
      />
      <circle
        cx="10"
        cy="10"
        r={RADIUS}
        fill="none"
        stroke-width="2.5"
        stroke-linecap="round"
        class="{ringClass} transition-[stroke-dashoffset] duration-700 ease-out"
        stroke-dasharray={CIRCUMFERENCE}
        stroke-dashoffset={CIRCUMFERENCE * (1 - fraction)}
        transform="rotate(-90 10 10)"
      />
    </svg>
  </TooltipAnchor>
{/if}
