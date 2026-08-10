<script lang="ts">
import { AlertTriangle } from "@lucide/svelte";
import type { DialogClose } from "$lib/app/dialogs.svelte";
import DialogShell from "$lib/components/DialogShell.svelte";
import type { WorktreeEntry } from "$lib/types";
import { folderName, isDirty } from "$lib/worktrees/worktrees";

let {
  entry,
  threadCount = 0,
  close,
}: {
  entry: WorktreeEntry;
  threadCount?: number;
  /** Resolves `{ force }` once the removal is confirmed. */
  close: DialogClose<{ force: boolean }>;
} = $props();

let typed = $state("");

const name = $derived(folderName(entry.path));
const dirty = $derived(isDirty(entry.status));
// A clean worktree removes without ceremony; a dirty one force-removes only
// after the user retypes its folder name, so it can never happen by a stray click.
const canRemove = $derived(!dirty || typed.trim() === name);

function confirm() {
  if (!canRemove) return;
  close({ force: dirty });
}
</script>

<DialogShell title="Remove worktree" width={460} onClose={() => close()}>
  <div class="mt-3 space-y-2 text-sm">
    <div class="flex items-center justify-between gap-2">
      <span class="text-surface-500">Branch</span>
      <span class="font-mono text-[13px]">{entry.detached ? "detached HEAD" : (entry.branch ?? "(none)")}</span>
    </div>
    <div class="flex items-center justify-between gap-2">
      <span class="text-surface-500">Working tree</span>
      <span class="font-medium {dirty ? 'text-warning-600 dark:text-warning-400' : 'text-success-600 dark:text-success-400'}">
        {dirty ? "Uncommitted changes" : "Clean"}
      </span>
    </div>
    <div class="flex items-center justify-between gap-2">
      <span class="text-surface-500">Linked threads</span>
      <span class="font-medium">{threadCount}</span>
    </div>
    <div class="truncate font-mono text-[11px] text-surface-500" title={entry.path}>{entry.path}</div>
  </div>

  {#if dirty}
    <div class="mt-4 rounded-lg border border-warning-500/40 bg-warning-500/10 p-3">
      <div class="flex items-center gap-1.5 text-xs font-semibold text-warning-700 dark:text-warning-300">
        <AlertTriangle size={14} />
        This worktree has uncommitted changes
      </div>
      <p class="mt-1 text-xs leading-5 text-surface-600-400">
        Force-removing discards that work permanently. Type
        <span class="font-mono font-semibold text-surface-900-100">{name}</span> to confirm.
      </p>
      <input
        type="text"
        bind:value={typed}
        placeholder={name}
        aria-label="Type the worktree folder name to confirm"
        class="mt-2 w-full rounded-md border border-surface-300-700 bg-surface-50-950 px-2.5 py-1.5 font-mono text-sm outline-none focus:border-primary-500"
      />
    </div>
  {/if}

  {#snippet footer()}
    <button type="button" onclick={() => close()} class="btn btn-sm preset-tonal">Cancel</button>
    <button
      type="button"
      onclick={confirm}
      disabled={!canRemove}
      class="btn btn-sm preset-filled-error-500 disabled:opacity-40"
    >
      {dirty ? "Force remove" : "Remove"}
    </button>
  {/snippet}
</DialogShell>
