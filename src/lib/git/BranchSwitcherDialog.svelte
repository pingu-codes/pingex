<script lang="ts">
import { GitBranch, Plus } from "@lucide/svelte";
import { onMount } from "svelte";
import { type DialogClose, submitState } from "$lib/app/dialogs.svelte";
import DialogShell from "$lib/components/DialogShell.svelte";
import { filterBranches } from "$lib/composer/reviewTargets";
import { gitBranches, gitCheckoutBranch, gitCreateBranch } from "$lib/services/api";
import type { GitBranch as Branch } from "$lib/types";
import { stripRemotePrefix } from "$lib/worktrees/worktrees";
import { hintFor, parseGitError } from "./gitErrors";

let {
  dir,
  currentBranch = null,
  close,
}: {
  dir: string;
  currentBranch?: string | null;
  /** Resolves with the branch now checked out. */
  close: DialogClose<string>;
} = $props();

let branches = $state<Branch[]>([]);
let loaded = $state(false);
let loadError = $state<string | null>(null);
let query = $state("");
let mode = $state<"switch" | "create">("switch");
let newName = $state("");
let base = $state("");
let checkoutNew = $state(true);
/** Set after a dirty-tree refusal so the user can carry changes over. */
let dirtyRefusal = $state<string | null>(null);
const action = submitState();

onMount(() => {
  gitBranches(dir)
    .then((list) => {
      branches = list;
    })
    .catch((cause) => {
      loadError = parseGitError(cause).message;
    })
    .finally(() => {
      loaded = true;
    });
});

const local = $derived(
  filterBranches(
    branches.filter((branch) => !branch.isRemote),
    query,
  ),
);
const remoteOnly = $derived(
  filterBranches(
    branches.filter(
      (branch) =>
        branch.isRemote && !branches.some((other) => !other.isRemote && other.name === stripRemotePrefix(branch.name)),
    ),
    query,
  ),
);
const canCreate = $derived(!action.busy && newName.trim().length > 0);

async function switchTo(name: string, force: boolean) {
  dirtyRefusal = null;
  const ok = await action.run(async () => {
    try {
      await gitCheckoutBranch(dir, name, force);
    } catch (cause) {
      const error = parseGitError(cause);
      if (error.kind === "dirtyTree") dirtyRefusal = name;
      throw new Error(error.message);
    }
  });
  if (ok) close(name);
}

/** A remote-only branch becomes a local tracking branch on checkout. */
async function trackRemote(remoteName: string) {
  const name = stripRemotePrefix(remoteName);
  const ok = await action.run(async () => {
    await gitCreateBranch(dir, name, remoteName, false);
    await gitCheckoutBranch(dir, name, false);
  });
  if (ok) close(name);
}

async function create() {
  if (!canCreate) return;
  const name = newName.trim();
  const ok = await action.run(() => gitCreateBranch(dir, name, base.trim() || null, checkoutNew));
  if (ok) close(checkoutNew ? name : (currentBranch ?? name));
}
</script>

<DialogShell title={mode === "switch" ? "Switch branch" : "Create branch"} width={480} onClose={() => close()}>
  {#snippet icon()}<GitBranch size={17} class="text-primary-500" />{/snippet}

  <div class="mt-3 flex gap-1 rounded-lg bg-surface-200-800 p-0.5 text-xs" role="tablist" aria-label="Branch action">
    <button type="button" role="tab" aria-selected={mode === "switch"} onclick={() => (mode = "switch")} class="flex-1 rounded-md px-2 py-1 {mode === 'switch' ? 'bg-surface-50-950 font-medium shadow-sm' : 'text-surface-500'}">Switch</button>
    <button type="button" role="tab" aria-selected={mode === "create"} onclick={() => (mode = "create")} class="flex-1 rounded-md px-2 py-1 {mode === 'create' ? 'bg-surface-50-950 font-medium shadow-sm' : 'text-surface-500'}">Create</button>
  </div>

  {#if mode === "switch"}
    <input
      type="search"
      bind:value={query}
      placeholder="Filter branches"
      aria-label="Filter branches"
      class="mt-3 w-full rounded-md border border-surface-300-700 bg-surface-50-950 px-2.5 py-1.5 text-sm outline-none focus:border-primary-500"
    />
    <div role="listbox" aria-label="Branches" class="mt-2 max-h-64 overflow-y-auto rounded-md border border-surface-200-800 bg-surface-50-950 p-1">
      {#if loadError}
        <p class="px-2 py-2 text-xs text-error-500">{loadError}</p>
      {:else if !loaded}
        <div class="placeholder m-1 h-8 animate-pulse rounded"></div>
      {:else if local.length === 0 && remoteOnly.length === 0}
        <p class="px-2 py-2 text-xs text-surface-500">No branches match.</p>
      {:else}
        {#each local as branch (branch.name)}
          <button
            type="button"
            role="option"
            aria-selected={branch.isCurrent}
            disabled={action.busy || branch.isCurrent}
            onclick={() => switchTo(branch.name, false)}
            class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-sm hover:preset-tonal disabled:opacity-60"
          >
            <GitBranch size={12} class="shrink-0 text-surface-500" />
            <span class="min-w-0 flex-1 truncate font-mono text-[13px]">{branch.name}</span>
            {#if branch.isCurrent}<span class="rounded-full bg-primary-500/15 px-1.5 py-0.5 text-[10px] font-medium text-primary-500">current</span>{/if}
          </button>
        {/each}
        {#if remoteOnly.length > 0}
          <div class="mt-1 px-2 pt-1 text-[10px] font-semibold uppercase tracking-[0.08em] text-surface-500">Remote</div>
          {#each remoteOnly as branch (branch.name)}
            <button
              type="button"
              role="option"
              aria-selected="false"
              disabled={action.busy}
              onclick={() => trackRemote(branch.name)}
              class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-sm hover:preset-tonal disabled:opacity-60"
              title="Create a local branch tracking {branch.name}"
            >
              <GitBranch size={12} class="shrink-0 text-surface-500" />
              <span class="min-w-0 flex-1 truncate font-mono text-[13px]">{branch.name}</span>
            </button>
          {/each}
        {/if}
      {/if}
    </div>
  {:else}
    <label class="mt-3 block">
      <span class="text-xs font-medium text-surface-600-400">Branch name</span>
      <input type="text" bind:value={newName} placeholder="feat/thing" aria-label="New branch name" class="mt-1 w-full rounded-md border border-surface-300-700 bg-surface-50-950 px-2.5 py-1.5 font-mono text-[13px] outline-none focus:border-primary-500" />
    </label>
    <label class="mt-3 block">
      <span class="text-xs font-medium text-surface-600-400">Start from</span>
      <input type="text" bind:value={base} list="branch-bases" placeholder={currentBranch ?? "HEAD"} aria-label="Base revision" class="mt-1 w-full rounded-md border border-surface-300-700 bg-surface-50-950 px-2.5 py-1.5 font-mono text-[13px] outline-none focus:border-primary-500" />
      <datalist id="branch-bases">
        {#each branches as branch (branch.name)}<option value={branch.name}></option>{/each}
      </datalist>
    </label>
    <label class="mt-3 flex items-center gap-2 text-xs">
      <input type="checkbox" bind:checked={checkoutNew} class="checkbox" />
      Switch to it
    </label>
  {/if}

  {#if action.error}
    <div class="mt-3 rounded-md preset-tonal-error px-3 py-2 text-xs">
      <div>{action.error}</div>
      {#if dirtyRefusal}
        <div class="mt-0.5 opacity-80">{hintFor("dirtyTree")}</div>
      {/if}
    </div>
  {/if}

  {#snippet footer()}
    <button type="button" onclick={() => close()} class="btn btn-sm preset-tonal">Cancel</button>
    {#if mode === "switch" && dirtyRefusal}
      <button type="button" onclick={() => switchTo(dirtyRefusal!, true)} disabled={action.busy} class="btn btn-sm preset-filled-warning-500 disabled:opacity-40">Switch anyway</button>
    {:else if mode === "create"}
      <button type="button" onclick={create} disabled={!canCreate} class="btn btn-sm preset-filled-primary-500 disabled:opacity-40">
        <Plus size={13} />
        {action.busy ? "Creating…" : "Create branch"}
      </button>
    {/if}
  {/snippet}
</DialogShell>
