<script lang="ts">
import { Check, ChevronDown, Cpu } from "@lucide/svelte";
import ComposerPopover from "$lib/composer/ComposerPopover.svelte";
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

<ComposerPopover
  {open}
  {onToggle}
  triggerLabel="Select model and effort"
  triggerClass={modelId === null ? "text-error-500 hover:text-error-500" : ""}
  label="Model options"
  contentClass="w-[290px]"
>
  {#snippet trigger()}
    <Cpu size={12} />
    <span class="min-w-0 truncate" title={label}>{label}</span>
    <ChevronDown size={11} />
  {/snippet}
  <div class="mb-1 px-1 eyebrow text-[10px]">Model</div>
  {#if modelsError}
    <p class="px-1 py-2 text-xs text-error-500">{modelsError}</p>
  {:else if models === null}
    <p class="px-1 py-2 text-xs text-surface-500">Loading models…</p>
  {:else}
    <div class="max-h-44 space-y-0.5 overflow-y-auto">
      {#each models as model (model.id)}
        <button
          onclick={() => onChooseModel(model)}
          class="menu-row"
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
        <span class="px-1 eyebrow text-[10px]">Effort</span>
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
</ComposerPopover>
