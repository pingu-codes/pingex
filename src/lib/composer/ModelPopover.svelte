<script lang="ts">
import { Check, ChevronDown, Cpu } from "@lucide/svelte";
import type { Model, ReasoningEffortOption } from "$lib/types";

let {
  open,
  models,
  modelsError,
  modelId,
  effort,
  effortOptions,
  label,
  onToggle,
  onChooseModel,
  onChooseEffort,
}: {
  open: boolean;
  models: Model[] | null;
  modelsError: string | null;
  modelId: string | null;
  effort: string | null;
  effortOptions: ReasoningEffortOption[];
  label: string;
  onToggle: () => void;
  onChooseModel: (model: Model) => void;
  onChooseEffort: (effort: string) => void;
} = $props();

function retirementLabel(model: Model): string | null {
  const retirementAt = model.upgradeInfo?.retirementAt;
  if (retirementAt == null) return null;
  const date = new Date(retirementAt * 1000);
  return `Retiring ${date.toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" })}`;
}
</script>

<div class="relative">
  <button
    onclick={(event) => {
      event.stopPropagation();
      onToggle();
    }}
    aria-label="Select model and effort"
    class="inline-flex items-center gap-1.5 rounded-full px-2 py-1 text-[11px] transition hover:bg-surface-200-800
      {modelId === null ? 'text-error-500' : 'text-surface-500 hover:text-surface-800-200'}"
  >
    <Cpu size={12} />
    {label}
    <ChevronDown size={11} />
  </button>
  {#if open}
    <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events, a11y_interactive_supports_focus -->
    <div
      class="card absolute bottom-8 left-0 z-50 w-[290px] select-none border border-surface-200-800 bg-surface-50-950 p-2 shadow-xl"
      onclick={(event) => event.stopPropagation()}
      role="dialog"
      aria-label="Model options"
    >
      <div class="mb-1 px-1 text-[10px] font-semibold uppercase tracking-[0.08em] text-surface-500">Model</div>
      {#if modelsError}
        <p class="px-1 py-2 text-xs text-error-500">{modelsError}</p>
      {:else if models === null}
        <p class="px-1 py-2 text-xs text-surface-500">Loading models…</p>
      {:else}
        <div class="max-h-44 space-y-0.5 overflow-y-auto">
          {#each models as model (model.id)}
            <button
              onclick={() => onChooseModel(model)}
              class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-xs hover:preset-tonal"
            >
              <span class="min-w-0 flex-1">
                <span class="flex items-center gap-1.5">
                  <span class="min-w-0 truncate">{model.displayName}</span>
                  {#if retirementLabel(model)}
                    <span
                      class="shrink-0 rounded-full bg-warning-500/15 px-1.5 py-px text-[9px] font-medium text-warning-600-400"
                      title={model.upgradeInfo?.upgradeCopy ?? undefined}
                    >
                      {retirementLabel(model)}
                    </span>
                  {/if}
                </span>
                <span class="block truncate text-[10px] text-surface-500">{model.description}</span>
              </span>
              {#if (modelId ?? (models ?? []).find((entry) => entry.isDefault)?.id) === model.id}
                <Check size={13} class="shrink-0 text-primary-500" />
              {/if}
            </button>
          {/each}
        </div>
        {#if effortOptions.length > 0}
          <div class="mt-2 border-t border-surface-200-800 pt-1.5">
            <span class="px-1 text-[10px] font-semibold uppercase tracking-[0.08em] text-surface-500">Effort</span>
            <div class="mt-1 flex flex-wrap gap-1 px-1">
              {#each effortOptions as option (option.reasoningEffort)}
                <button
                  onclick={() => onChooseEffort(option.reasoningEffort)}
                  title={option.description}
                  class="rounded-full px-2.5 py-1 text-[11px] capitalize transition {effort === option.reasoningEffort ? 'preset-filled-primary-500' : 'bg-surface-200-800 hover:preset-tonal'}"
                >
                  {option.reasoningEffort}
                </button>
              {/each}
            </div>
          </div>
        {/if}
      {/if}
    </div>
  {/if}
</div>
