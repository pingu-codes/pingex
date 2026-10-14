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
import type { PromptPart } from "$lib/types";
import UsageBreakdownBar from "$lib/usage/UsageBreakdownBar.svelte";
import { promptPartRows } from "$lib/usage/usageBreakdown";

let {
  parts,
  total,
  scaled = false,
  showInThread = false,
  onToggleShowInThread,
}: {
  parts: PromptPart[];
  /** Tokens in the whole context; the percentages are of this. */
  total: number;
  /** The estimated parts were scaled down to the measured system prompt. */
  scaled?: boolean;
  /** Whether the full text is currently shown inline in the thread. */
  showInThread?: boolean;
  /** Fetches (once) and toggles the full text in the thread. Omitted when
   *  the thread's harness cannot supply it. */
  onToggleShowInThread?: () => void;
} = $props();

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

const rows = $derived(
  promptPartRows(parts, total).map((row, index) => ({
    ...row,
    color: SHADES[index % SHADES.length],
    width: row.percentOfPrompt,
    percent: row.percentOfContext,
  })),
);
const footnote = $derived(
  [
    "≈ sized from the harness's own record of the prompt (about 4 bytes per token).",
    hasRemainder ? "Tool definitions are never written down, so they are what remains." : "",
    scaled ? "Scaled to fit the measured system prompt." : "",
  ]
    .filter(Boolean)
    .join(" "),
);
</script>

{#if rows.length > 0}
  <Collapsible class="mt-2 min-w-0 items-stretch" data-testid="prompt-parts">
    <div class="flex items-center justify-between gap-2">
      <Collapsible.Trigger class="group flex items-center gap-1 text-xs text-surface-500 hover:text-surface-700-300">
        <span>What is in the system prompt</span>
        <ChevronRight size={12} class="transition group-data-[state=open]:rotate-90" />
      </Collapsible.Trigger>
      {#if onToggleShowInThread}
        <button
          type="button"
          onclick={onToggleShowInThread}
          class="shrink-0 text-[10px] text-surface-500 underline decoration-dotted hover:text-surface-700-300"
        >
          {showInThread ? "Hide text in thread" : "View text in thread"}
        </button>
      {/if}
    </div>
    <Collapsible.Content>
      <div class="mt-2 border-l-2 border-surface-200-800 pl-3">
        <UsageBreakdownBar {rows} label="System prompt parts" thin {footnote} />
      </div>
    </Collapsible.Content>
  </Collapsible>
{/if}
