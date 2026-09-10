<!--
  The system prompt's parts, shown as synthetic entries at the top of the
  scrollback — not real thread items, never written to the journal. Turned
  on from the "What is in the system prompt" toggle in the status panel, so
  the user can read what is actually consuming context.
-->
<script lang="ts">
import { ChevronDown } from "@lucide/svelte";
import { Collapsible } from "@skeletonlabs/skeleton-svelte";
import { formatTokens } from "$lib/thread/contextUsage";
import type { PromptPart } from "$lib/types";

let { parts }: { parts: PromptPart[] } = $props();

const withText = $derived(parts.filter((part) => part.text));
</script>

{#if withText.length > 0}
  <div class="rounded-xl border border-dashed border-surface-300-700 p-3">
    <p class="mb-2 text-[10px] font-semibold uppercase tracking-wide text-surface-500">
      System prompt · not sent as a message
    </p>
    <div class="space-y-1.5">
      {#each withText as part (part.kind + (part.detail ?? ""))}
        <Collapsible class="min-w-0 items-stretch">
          <div class="overflow-hidden rounded-lg border border-surface-200-800 bg-surface-100-900">
            <Collapsible.Trigger class="group flex w-full min-w-0 items-center gap-2 px-3 py-2 text-left">
              <span class="min-w-0 flex-1 truncate text-xs font-medium text-surface-700-300" title={part.detail ?? part.label}>
                {part.label}{#if part.detail}<span class="text-surface-500"> · {part.detail}</span>{/if}
              </span>
              <span class="shrink-0 font-mono text-[10px] text-surface-500">{formatTokens(part.tokens)} tok</span>
              <ChevronDown size={13} class="shrink-0 text-surface-500 transition group-data-[state=open]:rotate-180" />
            </Collapsible.Trigger>
            <Collapsible.Content>
              <pre
                class="max-h-96 overflow-auto border-t border-surface-200-800 bg-surface-50-950 px-3 py-2.5 font-mono text-[11px] leading-5 whitespace-pre-wrap text-surface-600-400">{part.text}</pre>
            </Collapsible.Content>
          </div>
        </Collapsible>
      {/each}
    </div>
  </div>
{/if}
