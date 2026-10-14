<script lang="ts">
import { FolderGit2, GitBranch } from "@lucide/svelte";
import { onMount } from "svelte";
import type { DialogClose } from "$lib/app/dialogs.svelte";
import DialogShell from "$lib/components/DialogShell.svelte";
import DirectoryField from "$lib/components/DirectoryField.svelte";
import { commands } from "$lib/services/homeRouting";
import type { Project } from "$lib/types";
import { submitState } from "$lib/utils/actionState.svelte";
import { basename } from "$lib/utils/path";
import { resource } from "$lib/utils/resource.svelte";
import { adoptableWorktrees, branchLabel } from "$lib/worktrees/worktrees";

let {
  repoDir,
  projects = [],
  submit,
  close,
}: {
  repoDir: string;
  projects?: Project[];
  /** Adopts the worktree; rejecting keeps the dialog open with the error. */
  submit: (path: string) => Promise<void>;
  close: DialogClose<true>;
} = $props();

const worktrees = resource(() => commands.gitWorktrees(repoDir));
/** Chosen from the list, or browsed to by hand. */
let selectedPath = $state("");
const action = submitState();

onMount(() => void worktrees.reload());

const candidates = $derived(adoptableWorktrees(worktrees.value ?? [], projects));
const canAdd = $derived(!action.busy && selectedPath.trim().length > 0);

async function add() {
  if (!canAdd) return;
  const path = selectedPath.trim();
  if (await action.run(() => submit(path))) close(true);
}
</script>

<DialogShell title="Add worktree" width={500} onClose={() => close()}>
  {#snippet icon()}<FolderGit2 size={17} class="text-primary-500" />{/snippet}

  <p class="mt-3 text-xs text-surface-500">
    Add a worktree that already exists for <span class="font-mono">{basename(repoDir)}</span> to the sidebar.
  </p>

  <div class="mt-3">
    <span class="text-xs font-medium text-surface-600-400">Existing worktrees</span>
    <div
      role="listbox"
      aria-label="Existing worktrees"
      class="mt-1 max-h-56 select-none overflow-y-auto rounded-md border border-surface-200-800 bg-surface-50-950 p-1"
    >
      {#if worktrees.error}
        <p class="px-2 py-2 text-xs text-error-500">{worktrees.error}</p>
      {:else if !worktrees.value}
        <div class="placeholder m-1 h-8 animate-pulse rounded"></div>
      {:else if candidates.length === 0}
        <p class="px-2 py-2 text-xs text-surface-500">No other worktrees to add. Browse to one below, or create a new one.</p>
      {:else}
        {#each candidates as entry (entry.path)}
          <button
            type="button"
            role="option"
            aria-selected={selectedPath === entry.path}
            onclick={() => (selectedPath = entry.path)}
            class="flex w-full flex-col gap-0.5 rounded px-2 py-1.5 text-left {selectedPath === entry.path ? 'preset-tonal' : 'hover:preset-tonal'}"
          >
            <span class="flex items-center gap-2 text-sm">
              <span class="min-w-0 flex-1 truncate font-medium">{basename(entry.path)}</span>
              <span class="inline-flex shrink-0 items-center gap-1 font-mono text-[11px] text-surface-500">
                <GitBranch size={11} />{branchLabel(entry)}
              </span>
            </span>
            <span class="truncate font-mono text-[10px] text-surface-500" title={entry.path}>{entry.path}</span>
          </button>
        {/each}
      {/if}
    </div>
  </div>

  <div class="mt-3">
    <label for="add-worktree-location" class="text-xs font-medium text-surface-600-400">Location</label>
    <div class="mt-1">
      <DirectoryField
        id="add-worktree-location"
        value={selectedPath}
        onInput={(next) => (selectedPath = next)}
        placeholder="/path/to/worktree"
        browseLabel="Browse for worktree"
        pickerTitle="Choose a worktree"
      />
    </div>
    <p class="mt-1 text-[11px] text-surface-500">Must be a linked worktree of this repository, not the main checkout.</p>
  </div>

  {#if action.error}
    <p class="mt-3 error-note">{action.error}</p>
  {/if}

  {#snippet footer()}
    <button type="button" onclick={() => close()} class="btn btn-sm preset-tonal">Cancel</button>
    <button type="button" onclick={add} disabled={!canAdd} class="btn btn-sm preset-filled-primary-500 disabled:opacity-40">
      {action.busy ? "Adding…" : "Add worktree"}
    </button>
  {/snippet}
</DialogShell>
