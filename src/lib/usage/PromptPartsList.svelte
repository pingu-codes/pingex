<!--
  What the system prompt holds. Folded under the context composition bar: a
  thin bar of the parts and a row per part with its tokens and share of the
  context. Parts the harness reported are plain; parts the app sized from the
  harness's own record of the text are marked "≈", and when Codex's tool
  definitions are the remainder the footnote says so.
-->
<script lang="ts">
import { ChevronRight } from "@lucide/svelte";
import { Collapsible } from "@skeletonlabs/skeleton-svelte";
import { formatTokens } from "$lib/thread/contextUsage";
import type { PromptPart } from "$lib/types";
import { approx, promptPartRows } from "$lib/usage/usageBreakdown";

let {
  parts,
  total,
  scaled = false,
}: {
  parts: PromptPart[];
  /** Tokens in the whole context; the percentages are of this. */
  total: number;
  /** The estimated parts were scaled down to the measured system prompt. */
  scaled?: boolean;
} = $props();

const rows = $derived(promptPartRows(parts, total));
const anyEstimated = $derived(rows.some((row) => row.estimated));
const hasRemainder = $derived(parts.some((part) => part.kind === "tools" && part.source === "estimated"));

/** One hue per row, darkening with the index so the sub-bar reads left to right. */
const SHADES = [
  "bg-surface-500",
  "bg-surface-400-600",
  "bg-surface-300-700",
  "bg-surface-600-400",
  "bg-surface-700-300",
  "bg-surface-200-800",
];
</script>

{#if rows.length > 0}
  <Collapsible class="mt-2 min-w-0 items-stretch" data-testid="prompt-parts">
    <Collapsible.Trigger class="group flex items-center gap-1 text-xs text-surface-500 hover:text-surface-700-300">
      <span>What is in the system prompt</span>
      <ChevronRight size={12} class="transition group-data-[state=open]:rotate-90" />
    </Collapsible.Trigger>
    <Collapsible.Content>
      <div class="mt-2 border-l-2 border-surface-200-800 pl-3">
        <div class="flex h-1.5 w-full overflow-hidden rounded-full bg-surface-200-800" role="img" aria-label="System prompt parts">
          {#each rows as row, index (row.key)}
            <div
              class="h-full {SHADES[index % SHADES.length]}"
              style="width: {row.percentOfPrompt}%"
              title="{row.label}: {formatTokens(row.tokens)} tokens"
            ></div>
          {/each}
        </div>
        <dl class="mt-2 space-y-1">
          {#each rows as row, index (row.key)}
            <div class="flex items-baseline justify-between gap-3">
              <dt class="flex min-w-0 items-center gap-1.5 text-surface-600-400">
                <span class="inline-block size-2 shrink-0 rounded-sm {SHADES[index % SHADES.length]}" aria-hidden="true"></span>
                <span class="truncate" title={row.detail ?? row.label}>
                  {row.label}{#if row.detail}<span class="text-surface-500"> · {row.detail}</span>{/if}
                </span>
              </dt>
              <dd class="shrink-0 font-mono tabular-nums">
                {approx(formatTokens(row.tokens), row.estimated)}
                <span class="text-surface-500">· {Math.round(row.percentOfContext)}%</span>
              </dd>
            </div>
          {/each}
        </dl>
        {#if anyEstimated}
          <p class="mt-1.5 text-[10px] leading-4 text-surface-500">
            ≈ sized from the harness's own record of the prompt (about 4 bytes per token).
            {#if hasRemainder}Tool definitions are never written down, so they are what remains.{/if}
            {#if scaled}Scaled to fit the measured system prompt.{/if}
          </p>
        {/if}
      </div>
    </Collapsible.Content>
  </Collapsible>
{/if}
