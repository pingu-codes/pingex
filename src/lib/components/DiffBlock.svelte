<script lang="ts">
import { ChevronDown, FileDiff } from "@lucide/svelte";
import { Collapsible } from "@skeletonlabs/skeleton-svelte";
import { changeLabel } from "$lib/thread/fileChanges";
import type { FileUpdateChange } from "$lib/types";
import { highlightAs } from "$lib/utils/markdown";

let { change, autoCollapse = false }: { change: FileUpdateChange; autoCollapse?: boolean } = $props();

const LINE_LIMIT = 200;
const BYTE_LIMIT = 100_000;

const lines = $derived(change.diff.split("\n"));
const oversized = $derived(lines.length > LINE_LIMIT || change.diff.length > BYTE_LIMIT);

let showAll = $state(false);
const visible = $derived(!oversized || showAll ? change.diff : lines.slice(0, LINE_LIMIT).join("\n"));

// A user toggle wins permanently; until then, open tracks autoCollapse so a
// diff that expanded while it was the only change folds up when more appear.
let userOpen = $state<boolean | null>(null);
const open = $derived(userOpen ?? (!autoCollapse && !oversized));
</script>

<Collapsible {open} onOpenChange={(details) => (userOpen = details.open)}>
  <div class="overflow-hidden rounded-xl border border-surface-200-800 bg-surface-100-900">
    <Collapsible.Trigger class="group flex w-full items-center gap-2.5 px-3 py-2 text-left">
      <FileDiff size={13} class="shrink-0 text-surface-500" />
      <span class="min-w-0 flex-1 truncate font-mono text-xs">{change.path}</span>
      <span class="shrink-0 text-[10px] uppercase tracking-wide text-surface-500">{changeLabel(change.kind.type)}</span>
      <ChevronDown size={13} class="shrink-0 text-surface-500 transition group-data-[state=open]:rotate-180" />
    </Collapsible.Trigger>
    <Collapsible.Content>
      <pre class="diff-block max-h-96 overflow-auto border-t border-surface-200-800 px-3 py-2.5 font-mono text-[11px] leading-5"><code class="hljs">{@html highlightAs(visible, "diff")}</code></pre>
      {#if oversized && !showAll}
        <button
          onclick={() => (showAll = true)}
          class="w-full border-t border-surface-200-800 px-3 py-1.5 text-left text-[11px] text-surface-500 hover:preset-tonal"
        >
          Show all {lines.length} lines
        </button>
      {/if}
    </Collapsible.Content>
  </div>
</Collapsible>

<style>
  .diff-block {
    background: #0d1117;
    color: #e6edf3;
  }
  .diff-block :global(.hljs) {
    background: transparent;
    padding: 0;
  }
</style>
