<script lang="ts">
import type { DialogClose } from "$lib/app/dialogs.svelte";
import DialogShell from "$lib/components/DialogShell.svelte";
import { BUDGET_PRESETS, formatTokens, parseTokenBudget } from "$lib/thread/goalBudget";

let {
  current,
  used,
  close,
}: {
  /** The cap the goal holds now, or null for none. */
  current: number | null;
  /** Tokens the goal has spent so far. */
  used: number;
  /** `null` lifts the cap; closing with nothing leaves it alone. */
  close: DialogClose<number | null>;
} = $props();

// svelte-ignore state_referenced_locally
let text = $state(current ? formatTokens(current) : "");
const parsed = $derived(parseTokenBudget(text));
const invalid = $derived(text.trim() !== "" && parsed === null);

function save() {
  if (parsed) close(parsed);
}
</script>

<DialogShell title="Goal budget" onClose={() => close()}>
  <p class="mt-3 text-sm leading-6 text-surface-600-400">
    Codex pauses the goal once it has spent this many tokens. Used so far:
    <span class="font-medium text-surface-900-100">{formatTokens(used)}</span>.
  </p>
  <form
    class="mt-3 space-y-2"
    onsubmit={(event) => {
      event.preventDefault();
      save();
    }}
  >
    <label class="label">
      <span class="label-text">Tokens</span>
      <input
        class="input"
        type="text"
        inputmode="text"
        placeholder="e.g. 500k or 2m"
        aria-label="Token budget"
        aria-invalid={invalid}
        bind:value={text}
      />
    </label>
    <div class="flex flex-wrap gap-1.5">
      {#each BUDGET_PRESETS as preset (preset)}
        <button
          type="button"
          class="chip {parsed === preset ? 'preset-filled-primary-500' : 'preset-tonal'}"
          onclick={() => (text = formatTokens(preset))}
        >
          {formatTokens(preset)}
        </button>
      {/each}
    </div>
    {#if invalid}
      <p class="text-xs text-error-500">Enter a whole number of tokens, like 250000, 500k or 2m.</p>
    {/if}
  </form>

  {#snippet footer()}
    <button type="button" onclick={() => close()} class="btn btn-sm preset-tonal">Cancel</button>
    {#if current !== null}
      <button type="button" onclick={() => close(null)} class="btn btn-sm preset-tonal-warning">No cap</button>
    {/if}
    <button type="button" onclick={save} disabled={!parsed} class="btn btn-sm preset-filled-primary-500">Save</button>
  {/snippet}
</DialogShell>
