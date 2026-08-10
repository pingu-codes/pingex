<script lang="ts">
import type { RateLimitSnapshot } from "$lib/types";
import { primaryUsageWindow, resetLabel, usageToneClass, usageWindows } from "$lib/utils/rateLimits";

let {
  snapshot,
  compact = false,
  namePrefix = null,
}: {
  snapshot: RateLimitSnapshot | null;
  /** Sidebar footer variant: one headline bar, details on hover. */
  compact?: boolean;
  /** Bucket name to prefix window labels with, for per-model limits. */
  namePrefix?: string | null;
} = $props();

const windows = $derived(usageWindows(snapshot));
const headline = $derived(primaryUsageWindow(snapshot));
</script>

{#if headline}
  {#if compact}
    <div
      class="px-2 pb-1.5 pt-1"
      title={windows
        .map((window) => `${window.label}: ${window.remainingPercent}% left${resetLabel(window.resetsAt) ? ` (${resetLabel(window.resetsAt)})` : ""}`)
        .join("\n")}
    >
      <div class="flex items-baseline justify-between gap-2 text-[10px] text-surface-500">
        <span>{headline.label} usage left</span>
        <span class="font-medium tabular-nums text-surface-700-300">{headline.remainingPercent}%</span>
      </div>
      <div class="mt-1 h-1 w-full overflow-hidden rounded-full bg-surface-300-700">
        <div
          class="h-full rounded-full transition-[width] duration-500 {usageToneClass(headline.usedPercent)}"
          style={`width: ${headline.usedPercent}%`}
        ></div>
      </div>
    </div>
  {:else}
    <div class="space-y-2">
      {#each windows as window (window.label)}
        <div>
          <div class="flex items-baseline justify-between gap-2 text-[11px]">
            <span class="min-w-0 truncate text-surface-500">{namePrefix ? `${namePrefix} · ${window.label}` : window.label}</span>
            <span class="font-medium tabular-nums">{window.remainingPercent}% left</span>
          </div>
          <div class="mt-1 h-1.5 w-full overflow-hidden rounded-full bg-surface-300-700">
            <div
              class="h-full rounded-full transition-[width] duration-500 {usageToneClass(window.usedPercent)}"
              style={`width: ${window.usedPercent}%`}
            ></div>
          </div>
          {#if resetLabel(window.resetsAt)}
            <div class="mt-0.5 text-[10px] text-surface-500">{resetLabel(window.resetsAt)}</div>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
{:else}
  <p class="{compact ? 'px-2 pb-1.5 pt-1' : ''} text-[10px] text-surface-500">Usage limits unavailable.</p>
{/if}
