<script lang="ts">
import { Check, ChevronDown, ListTodo } from "@lucide/svelte";
import { Collapsible } from "@skeletonlabs/skeleton-svelte";
import type { TurnPlan } from "$lib/types";

let { plan }: { plan: TurnPlan } = $props();

const done = $derived(plan.steps.filter((step) => step.status === "completed").length);
// The step Codex says it is on, or the next one it has not done — the summary
// line is more useful showing what is happening than what has finished.
const current = $derived(
  plan.steps.find((step) => step.status === "inProgress") ?? plan.steps.find((step) => step.status === "pending"),
);
</script>

<Collapsible>
  <div class="overflow-hidden rounded-xl border border-surface-200-800 bg-surface-100-900">
    <Collapsible.Trigger class="group flex w-full items-center gap-2.5 px-3 py-2 text-left">
      <ListTodo size={13} class="shrink-0 text-surface-500" />
      <span class="shrink-0 text-xs font-medium">{done}/{plan.steps.length}</span>
      <span class="min-w-0 flex-1 truncate text-xs text-surface-500">{current?.step ?? "All done"}</span>
      <ChevronDown size={13} class="shrink-0 text-surface-500 transition group-data-[state=open]:rotate-180" />
    </Collapsible.Trigger>
    <Collapsible.Content>
      <div class="space-y-1.5 border-t border-surface-200-800 px-3 py-2.5">
        {#if plan.explanation}
          <p class="pb-1 text-[11px] leading-4 text-surface-500">{plan.explanation}</p>
        {/if}
        {#each plan.steps as step, index (index)}
          <div class="flex items-start gap-2 text-xs leading-5">
            <span
              class="mt-0.5 grid size-3.5 shrink-0 place-items-center rounded-full border {step.status === 'completed'
                ? 'border-success-500 bg-success-500 text-white'
                : step.status === 'inProgress'
                  ? 'animate-pulse border-primary-500'
                  : 'border-surface-400-600'}"
            >
              {#if step.status === "completed"}<Check size={9} />{/if}
            </span>
            <span class="min-w-0 {step.status === 'completed' ? 'text-surface-500 line-through' : ''}">
              {step.step}
            </span>
          </div>
        {/each}
      </div>
    </Collapsible.Content>
  </div>
</Collapsible>
