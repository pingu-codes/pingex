<script lang="ts">
import { FolderGit2, FolderOpen, GitBranch } from "@lucide/svelte";
import { open as openPicker } from "@tauri-apps/plugin-dialog";
import { onMount } from "svelte";
import { type DialogClose, submitState } from "$lib/app/dialogs.svelte";
import DialogShell from "$lib/components/DialogShell.svelte";
import TooltipButton from "$lib/components/TooltipButton.svelte";
import { gitWorktrees, isTauri } from "$lib/services/api";
import type { Project, WorktreeEntry } from "$lib/types";
import { adoptableWorktrees, folderName } from "$lib/worktrees/worktrees";

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

let entries = $state<WorktreeEntry[]>([]);
let loaded = $state(false);
let loadError = $state<string | null>(null);
/** Chosen from the list, or browsed to by hand. */
let selectedPath = $state("");
const action = submitState();

onMount(() => {
  gitWorktrees(repoDir)
    .then((list) => {
      entries = list;
    })
    .catch((cause) => {
      loadError = cause instanceof Error ? cause.message : String(cause);
    })
    .finally(() => {
      loaded = true;
    });
});

const candidates = $derived(adoptableWorktrees(entries, projects));
const canAdd = $derived(!action.busy && selectedPath.trim().length > 0);

function branchLabel(entry: WorktreeEntry): string {
  if (entry.detached) return entry.head ? `detached @ ${entry.head.slice(0, 7)}` : "detached";
  return entry.branch ?? "(no branch)";
}

async function browse() {
  if (!isTauri()) return;
  const picked = await openPicker({ directory: true, multiple: false, title: "Choose a worktree" });
  if (typeof picked === "string") selectedPath = picked;
}

async function add() {
  if (!canAdd) return;
  const path = selectedPath.trim();
  if (await action.run(() => submit(path))) close(true);
}
</script>

<DialogShell title="Add worktree" width={500} onClose={() => close()}>
  {#snippet icon()}<FolderGit2 size={17} class="text-primary-500" />{/snippet}

  <p class="mt-3 text-xs text-surface-500">
    Add a worktree that already exists for <span class="font-mono">{folderName(repoDir)}</span> to the sidebar.
  </p>

  <div class="mt-3">
    <span class="text-xs font-medium text-surface-600-400">Existing worktrees</span>
    <div
      role="listbox"
      aria-label="Existing worktrees"
      class="mt-1 max-h-56 select-none overflow-y-auto rounded-md border border-surface-200-800 bg-surface-50-950 p-1"
    >
      {#if loadError}
        <p class="px-2 py-2 text-xs text-error-500">{loadError}</p>
      {:else if !loaded}
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
              <span class="min-w-0 flex-1 truncate font-medium">{folderName(entry.path)}</span>
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

  <label class="mt-3 block">
    <span class="text-xs font-medium text-surface-600-400">Location</span>
    <div class="mt-1 flex gap-2">
      <input
        type="text"
        value={selectedPath}
        oninput={(event) => (selectedPath = event.currentTarget.value)}
        placeholder="/path/to/worktree"
        class="min-w-0 flex-1 rounded-md border border-surface-300-700 bg-surface-50-950 px-2.5 py-1.5 font-mono text-[13px] outline-none focus:border-primary-500"
      />
      {#if isTauri()}
        <TooltipButton label="Browse for worktree" type="button" onclick={browse} class="btn btn-sm preset-tonal shrink-0" aria-label="Browse for worktree">
          <FolderOpen size={14} />
        </TooltipButton>
      {/if}
    </div>
    <p class="mt-1 text-[11px] text-surface-500">Must be a linked worktree of this repository, not the main checkout.</p>
  </label>

  {#if action.error}
    <p class="mt-3 rounded-md preset-tonal-error px-3 py-2 text-xs">{action.error}</p>
  {/if}

  {#snippet footer()}
    <button type="button" onclick={() => close()} class="btn btn-sm preset-tonal">Cancel</button>
    <button type="button" onclick={add} disabled={!canAdd} class="btn btn-sm preset-filled-primary-500 disabled:opacity-40">
      {action.busy ? "Adding…" : "Add worktree"}
    </button>
  {/snippet}
</DialogShell>
