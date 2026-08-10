<script lang="ts">
import { type ContextStats, formatTokens, formatTokensShort } from "$lib/thread/contextUsage";

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

let hovered = $state(false);

const RADIUS = 8;
const CIRCUMFERENCE = 2 * Math.PI * RADIUS;

// Unknown context windows still get a ring: fall back to an empty one rather
// than hiding the control, so the popover stays reachable for raw counts.
const fraction = $derived(Math.min(Math.max(stats?.usedFraction ?? 0, 0), 1));
const percentUsed = $derived(stats?.percentUsed ?? null);
const ringClass = $derived(
  fraction >= 0.9 ? "stroke-error-500" : fraction >= 0.7 ? "stroke-warning-500" : "stroke-primary-500",
);
const label = $derived(
  percentUsed === null
    ? `Context: ${formatTokensShort(stats?.usedTokens ?? 0)} tokens used`
    : `Context: ${percentUsed}% used`,
);
</script>

{#if stats}
  <div
    class="relative"
    role="group"
    onmouseenter={() => (hovered = true)}
    onmouseleave={() => (hovered = false)}
  >
    <button
      type="button"
      aria-label={label}
      onfocus={() => (hovered = true)}
      onblur={() => (hovered = false)}
      onclick={(event) => {
        event.stopPropagation();
        onCompact?.();
      }}
      disabled={!onCompact || compacting || busy}
      class="grid size-7 place-items-center rounded-full text-surface-500 transition hover:bg-surface-200-800 disabled:cursor-default disabled:hover:bg-transparent"
    >
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
    </button>

    {#if hovered}
      <div
        class="card pointer-events-none absolute bottom-9 right-0 z-50 w-[248px] select-none border border-surface-200-800 bg-surface-50-950 p-2.5 shadow-xl"
        role="tooltip"
      >
        <div class="flex items-baseline justify-between gap-2">
          <span class="text-[10px] font-semibold uppercase tracking-[0.08em] text-surface-500">Context</span>
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
      </div>
    {/if}
  </div>
{/if}
