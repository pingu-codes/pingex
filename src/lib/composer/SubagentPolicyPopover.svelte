<script lang="ts">
import { Bot, ChevronDown } from "@lucide/svelte";
import ComposerPopover from "$lib/composer/ComposerPopover.svelte";
import { policyAllows, policyIsEmpty } from "$lib/composer/composerPrefs";
import type { Model, SubagentPolicy } from "$lib/types";

let {
  open,
  models,
  modelsError,
  efforts,
  modelPolicy,
  effortPolicy,
  onToggle,
  onToggleModel,
  onToggleEffort,
}: {
  open: boolean;
  models: Model[] | null;
  modelsError: string | null;
  efforts: string[];
  modelPolicy: SubagentPolicy | null;
  effortPolicy: SubagentPolicy | null;
  onToggle: () => void;
  onToggleModel: (id: string) => void;
  onToggleEffort: (effort: string) => void;
} = $props();

const noModels = $derived(
  policyIsEmpty(
    modelPolicy,
    (models ?? []).map((model) => model.id),
  ),
);
const noEfforts = $derived(policyIsEmpty(effortPolicy, efforts));
</script>

<ComposerPopover
  {open}
  {onToggle}
  triggerLabel="Set subagent models and effort"
  triggerClass={noModels || noEfforts ? "text-error-500 hover:text-error-500" : ""}
  label="Subagent options"
  contentClass="w-[300px]"
>
  {#snippet trigger()}
    <Bot size={12} />
    Subagents
    <ChevronDown size={11} />
  {/snippet}
  <div class="px-1 eyebrow text-[10px]">Included models</div>
  {#if modelsError}
    <p class="px-1 py-2 text-xs text-error-500">{modelsError}</p>
  {:else if models === null}
    <p class="px-1 py-2 text-xs text-surface-500">Loading models…</p>
  {:else}
    <div class="mt-1 max-h-44 space-y-0.5 overflow-y-auto">
      {#each models as model (model.id)}
        <label class="flex cursor-pointer items-center gap-2 rounded px-2 py-1.5 text-xs hover:preset-tonal">
          <input
            type="checkbox"
            checked={policyAllows(modelPolicy, model.id)}
            onchange={() => onToggleModel(model.id)}
            class="checkbox"
          />
          <span class="min-w-0 flex-1 truncate">{model.displayName}</span>
          {#if model.hidden}<span class="text-[10px] text-surface-500">hidden</span>{/if}
        </label>
      {/each}
    </div>
    {#if noModels}
      <p class="px-1 pt-1 text-[10px] leading-4 text-error-500">Keep at least one model checked.</p>
    {/if}
    <div class="mt-2 border-t border-surface-200-800 pt-1.5">
      <span class="px-1 eyebrow text-[10px]">Included effort levels</span>
      <div class="mt-1 grid grid-cols-2 gap-0.5">
        {#each efforts as effort (effort)}
          <label class="flex cursor-pointer items-center gap-2 rounded px-2 py-1.5 text-xs capitalize hover:preset-tonal">
            <input
              type="checkbox"
              checked={policyAllows(effortPolicy, effort)}
              onchange={() => onToggleEffort(effort)}
              class="checkbox"
            />
            {effort}
          </label>
        {/each}
      </div>
      {#if noEfforts}
        <p class="px-1 pt-1 text-[10px] leading-4 text-error-500">Keep at least one effort level checked.</p>
      {/if}
    </div>
  {/if}
  <p class="mt-2 border-t border-surface-200-800 px-1 pt-2 text-[10px] leading-4 text-surface-500">
    Applies to agents spawned from this thread. Subagents may use any checked value, so at least
    one has to stay checked.
  </p>
</ComposerPopover>
