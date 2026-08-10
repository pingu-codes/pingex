<script lang="ts">
import { Bot, ChevronDown } from "@lucide/svelte";
import { policyAllows, policyIsEmpty } from "$lib/composer/composerPrefs.svelte";
import type { Model, SubagentPolicy } from "$lib/types";

let {
  open,
  models,
  modelsError,
  efforts,
  modelPolicy,
  effortPolicy,
  appSubagents,
  appSubagentsLocked = false,
  onToggle,
  onToggleModel,
  onToggleEffort,
  onSetAppSubagents,
}: {
  open: boolean;
  models: Model[] | null;
  modelsError: string | null;
  efforts: string[];
  modelPolicy: SubagentPolicy | null;
  effortPolicy: SubagentPolicy | null;
  /** Per-thread override; `null` follows the global setting. */
  appSubagents: boolean | null;
  /**
   * True once the thread exists. `dynamicTools` is only accepted on
   * `thread/start`, so the choice cannot be changed after that point.
   */
  appSubagentsLocked?: boolean;
  onToggle: () => void;
  onToggleModel: (id: string) => void;
  onToggleEffort: (effort: string) => void;
  onSetAppSubagents: (value: boolean | null) => void;
} = $props();

const APP_SUBAGENT_CHOICES: { value: boolean | null; label: string }[] = [
  { value: null, label: "Default" },
  { value: true, label: "On" },
  { value: false, label: "Off" },
];

const noModels = $derived(
  policyIsEmpty(
    modelPolicy,
    (models ?? []).map((model) => model.id),
  ),
);
const noEfforts = $derived(policyIsEmpty(effortPolicy, efforts));
</script>

<div class="relative">
  <button
    onclick={(event) => {
      event.stopPropagation();
      onToggle();
    }}
    aria-label="Set subagent models and effort"
    class="inline-flex items-center gap-1.5 rounded-full px-2 py-1 text-[11px] transition hover:bg-surface-200-800
      {noModels || noEfforts ? 'text-error-500' : 'text-surface-500 hover:text-surface-800-200'}"
  >
    <Bot size={12} />
    Subagents
    <ChevronDown size={11} />
  </button>
  {#if open}
    <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events, a11y_interactive_supports_focus -->
    <div
      class="card absolute bottom-8 left-0 z-50 w-[300px] select-none border border-surface-200-800 bg-surface-50-950 p-2 shadow-xl"
      onclick={(event) => event.stopPropagation()}
      role="dialog"
      aria-label="Subagent options"
    >
      <div class="px-1 text-[10px] font-semibold uppercase tracking-[0.08em] text-surface-500">Included models</div>
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
          <span class="px-1 text-[10px] font-semibold uppercase tracking-[0.08em] text-surface-500">Included effort levels</span>
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
      <div class="mt-2 border-t border-surface-200-800 pt-2">
        <div class="flex items-center justify-between gap-2 px-1">
          <span class="text-[10px] font-semibold uppercase tracking-[0.08em] text-surface-500">
            Run in separate processes
          </span>
          <div class="flex shrink-0 gap-0.5">
            {#each APP_SUBAGENT_CHOICES as choice (choice.label)}
              <button
                type="button"
                disabled={appSubagentsLocked}
                aria-pressed={appSubagents === choice.value}
                onclick={() => onSetAppSubagents(choice.value)}
                class="rounded px-1.5 py-0.5 text-[10px] transition disabled:opacity-40
                  {appSubagents === choice.value
                  ? 'preset-filled-primary-500'
                  : 'text-surface-500 hover:preset-tonal'}"
              >
                {choice.label}
              </button>
            {/each}
          </div>
        </div>
        <p class="mt-1 px-1 text-[10px] leading-4 text-surface-500">
          {#if appSubagentsLocked}
            Fixed when this thread was created — start a new thread to change it.
          {:else}
            Uses Pingex's own spawn tools so each subagent runs in a process you can watch and stop.
          {/if}
        </p>
      </div>
    </div>
  {/if}
</div>
