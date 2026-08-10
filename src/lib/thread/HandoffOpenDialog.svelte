<script lang="ts">
import { AlertTriangle } from "@lucide/svelte";
import { type DialogClose, submitState } from "$lib/app/dialogs.svelte";
import DialogShell from "$lib/components/DialogShell.svelte";
import { handoffHomeIssue, shortHomeName } from "$lib/thread/handoff";
import type { HandoffOpen } from "$lib/types";

let {
  handoff,
  submit,
  close,
}: {
  /** The incoming handoff whose home differs from the running one. */
  handoff: HandoffOpen;
  /** Switches the app to the requested home; rejecting keeps the dialog open. */
  submit: (handoff: HandoffOpen) => Promise<void>;
  close: DialogClose<true>;
} = $props();

const unknownHome = $derived(handoffHomeIssue(handoff));
const action = submitState();

async function switchHome() {
  if (await action.run(() => submit(handoff))) close(true);
}
</script>

<DialogShell
  title={unknownHome ? "Codex home not found" : "Switch Codex home?"}
  width={460}
  onClose={() => close()}
>
  {#snippet icon()}<AlertTriangle size={16} class="text-warning-500" />{/snippet}

  {#if unknownHome}
    <p class="mt-3 text-sm leading-6 text-surface-600-400">{unknownHome}</p>
    <p class="mt-2 text-xs leading-5 text-surface-500">
      Open the app against that Codex home, or check the link is correct, then try again.
    </p>
  {:else}
    <p class="mt-3 text-sm leading-6 text-surface-600-400">
      This link opens a thread in a different Codex home
      <span class="font-medium text-surface-900-100">{shortHomeName(handoff.requestedHome)}</span>.
      Switching reopens the app against that home.
    </p>
    <dl class="mt-3 space-y-1.5 text-xs">
      <div class="flex gap-2">
        <dt class="w-16 shrink-0 text-surface-500">Home</dt>
        <dd class="min-w-0 flex-1 truncate font-mono text-surface-900-100" title={handoff.requestedHome ?? undefined}>{handoff.requestedHome}</dd>
      </div>
      {#if handoff.threadId}
        <div class="flex gap-2">
          <dt class="w-16 shrink-0 text-surface-500">Thread</dt>
          <dd class="min-w-0 flex-1 truncate font-mono text-surface-900-100" title={handoff.threadId}>{handoff.threadId}</dd>
        </div>
      {/if}
      {#if handoff.path}
        <div class="flex gap-2">
          <dt class="w-16 shrink-0 text-surface-500">Directory</dt>
          <dd class="min-w-0 flex-1 truncate font-mono text-surface-900-100" title={handoff.path}>{handoff.path}</dd>
        </div>
      {/if}
    </dl>
  {/if}

  {#if action.error}
    <p class="mt-3 rounded-md preset-tonal-error px-3 py-2 text-xs">{action.error}</p>
  {/if}

  {#snippet footer()}
    <button type="button" onclick={() => close()} class="btn btn-sm preset-tonal">
      {unknownHome ? "Close" : "Cancel"}
    </button>
    {#if !unknownHome}
      <button type="button" disabled={action.busy} onclick={switchHome} class="btn btn-sm preset-filled-primary-500 disabled:opacity-50">
        {action.busy ? "Switching…" : "Switch home"}
      </button>
    {/if}
  {/snippet}
</DialogShell>
