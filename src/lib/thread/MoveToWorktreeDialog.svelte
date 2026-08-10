<script lang="ts">
import { GitBranch, RefreshCw } from "@lucide/svelte";
import { onMount } from "svelte";
import { type DialogClose, submitState } from "$lib/app/dialogs.svelte";
import DialogShell from "$lib/components/DialogShell.svelte";
import { gitWorktrees } from "$lib/services/api";
import { dirName } from "$lib/thread/handoff";
import type { WorktreeEntry } from "$lib/types";

let {
  repoDir,
  currentCwd,
  submit,
  close,
}: {
  repoDir: string;
  currentCwd: string;
  /** Forks the thread into the chosen worktree; rejecting keeps the dialog open. */
  submit: (path: string) => Promise<void>;
  close: DialogClose<true>;
} = $props();

let worktrees = $state<WorktreeEntry[]>([]);
let loading = $state(true);
let loadError = $state<string | null>(null);
let selected = $state<string | null>(null);
const action = submitState();

const normalize = (value: string) => value.replace(/\/+$/, "");

// Offer every usable worktree except the one this thread already runs in.
const options = $derived(
  worktrees.filter((entry) => !entry.missingDir && normalize(entry.path) !== normalize(currentCwd)),
);

onMount(() => {
  gitWorktrees(repoDir)
    .then((entries) => {
      worktrees = entries;
    })
    .catch((cause) => {
      loadError = cause instanceof Error ? cause.message : String(cause);
    })
    .finally(() => {
      loading = false;
    });
});

async function fork() {
  const path = selected;
  if (!path) return;
  if (await action.run(() => submit(path))) close(true);
}
</script>

<DialogShell title="Move to worktree" width={480} onClose={() => close()}>
  <p class="mt-2 text-sm leading-6 text-surface-600-400">
    This creates a <span class="font-medium text-surface-900-100">forked continuation</span> of the thread in the
    chosen worktree. Existing turns keep their original directory — only new work runs in the new worktree.
  </p>

  {#if loading}
    <div class="mt-4 grid place-items-center py-6"><RefreshCw size={18} class="animate-spin text-surface-500" /></div>
  {:else if loadError}
    <div class="card preset-tonal-error mt-4 p-3 text-xs">{loadError}</div>
  {:else if options.length === 0}
    <p class="mt-4 rounded-lg border border-surface-200-800 bg-surface-100-900 px-3 py-4 text-center text-xs text-surface-500">
      No other worktrees are available to move to.
    </p>
  {:else}
    <div class="mt-3 max-h-64 space-y-1 overflow-y-auto">
      {#each options as entry (entry.path)}
        <button
          type="button"
          onclick={() => (selected = entry.path)}
          class="flex w-full items-center gap-2 rounded-lg border px-3 py-2 text-left text-xs transition {selected === entry.path
            ? 'border-primary-500 bg-primary-500/10'
            : 'border-surface-200-800 hover:preset-tonal'}"
        >
          <GitBranch size={13} class="shrink-0 text-surface-500" />
          <span class="min-w-0 flex-1">
            <span class="block font-medium text-surface-900-100">
              {entry.detached ? "detached" : (entry.branch ?? dirName(entry.path))}
              {#if entry.isMain}<span class="ml-1 text-[10px] text-surface-500">(main)</span>{/if}
            </span>
            <span class="block truncate text-[10px] text-surface-500" title={entry.path}>{entry.path}</span>
          </span>
        </button>
      {/each}
    </div>
  {/if}

  {#if action.error}
    <div class="card preset-tonal-error mt-3 p-3 text-xs">{action.error}</div>
  {/if}

  {#snippet footer()}
    <button type="button" onclick={() => close()} class="btn btn-sm preset-tonal">Cancel</button>
    <button
      type="button"
      disabled={!selected || action.busy}
      onclick={fork}
      class="btn btn-sm preset-filled-primary-500 disabled:opacity-50"
    >
      {action.busy ? "Forking…" : "Fork here"}
    </button>
  {/snippet}
</DialogShell>
