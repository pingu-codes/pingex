<script lang="ts">
import { GitBranch, Plus } from "@lucide/svelte";
import { onMount } from "svelte";
import type { DialogClose } from "$lib/app/dialogs.svelte";
import BranchList from "$lib/components/BranchList.svelte";
import DialogShell from "$lib/components/DialogShell.svelte";
import Segmented from "$lib/components/Segmented.svelte";
import { commands } from "$lib/services/homeRouting";
import type { GitBranch as Branch } from "$lib/types";
import { submitState } from "$lib/utils/actionState.svelte";
import { resource } from "$lib/utils/resource.svelte";
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

const listing = resource(() => commands.gitBranches(dir, null), { describe: (cause) => parseGitError(cause).message });
const branches = $derived<Branch[]>(listing.value ?? []);
let mode = $state<"switch" | "create">("switch");
let newName = $state("");
let base = $state("");
let checkoutNew = $state(true);
/** Set after a dirty-tree refusal so the user can carry changes over. */
let dirtyRefusal = $state<string | null>(null);
const action = submitState();

onMount(() => void listing.reload());

const local = $derived(branches.filter((branch) => !branch.isRemote));
const remoteOnly = $derived(
  branches.filter(
    (branch) =>
      branch.isRemote && !branches.some((other) => !other.isRemote && other.name === stripRemotePrefix(branch.name)),
  ),
);
const canCreate = $derived(!action.busy && newName.trim().length > 0);

async function switchTo(name: string, force: boolean) {
  dirtyRefusal = null;
  const ok = await action.run(async () => {
    try {
      await commands.gitCheckoutBranch(dir, name, force);
    } catch (cause) {
      const error = parseGitError(cause);
      if (error.kind === "dirtyTree") dirtyRefusal = name;
      throw new Error(error.message);
    }
  });
  if (ok) close(name);
}

/** A remote-only branch becomes a local tracking branch on checkout. The
 *  create and the switch are separate steps: a dirty tree refuses the switch
 *  after the branch exists, and that must surface as the usual "Switch
 *  anyway" rather than a retry that fails with "already exists". */
async function trackRemote(remoteName: string) {
  const name = stripRemotePrefix(remoteName);
  if (!branches.some((branch) => !branch.isRemote && branch.name === name)) {
    const created = await action.run(() => commands.gitCreateBranch(dir, name, remoteName, false));
    if (!created) return;
    // A failed re-read keeps the list as it was.
    listing.set(await commands.gitBranches(dir, null).catch(() => branches));
  }
  await switchTo(name, false);
}

async function create() {
  if (!canCreate) return;
  const name = newName.trim();
  const ok = await action.run(() => commands.gitCreateBranch(dir, name, base.trim() || null, checkoutNew));
  if (ok) close(checkoutNew ? name : (currentBranch ?? name));
}
</script>

<DialogShell title={mode === "switch" ? "Switch branch" : "Create branch"} width={480} onClose={() => close()}>
  {#snippet icon()}<GitBranch size={17} class="text-primary-500" />{/snippet}

  <Segmented
    label="Branch action"
    value={mode}
    onChange={(next) => (mode = next)}
    fill
    class="mt-3"
    options={[
      { value: "switch", label: "Switch" },
      { value: "create", label: "Create" },
    ]}
  />

  {#if mode === "switch"}
    <BranchList
      id="branch-switch-list"
      class="mt-3"
      label="Filter branches"
      branches={local}
      remote={remoteOnly}
      loaded={Boolean(listing.value)}
      error={listing.error}
      disabled={(branch) => action.busy || branch.isCurrent}
      onPick={(branch) => (branch.isRemote ? trackRemote(branch.name) : switchTo(branch.name, false))}
    />
  {:else}
    <label class="mt-3 block">
      <span class="text-xs font-medium text-surface-600-400">Branch name</span>
      <input type="text" bind:value={newName} placeholder="feat/thing" aria-label="New branch name" class="mt-1 w-full field font-mono text-[13px]" />
    </label>
    <label class="mt-3 block">
      <span class="text-xs font-medium text-surface-600-400">Start from</span>
      <input type="text" bind:value={base} list="branch-bases" placeholder={currentBranch ?? "HEAD"} aria-label="Base revision" class="mt-1 w-full field font-mono text-[13px]" />
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
    <div class="mt-3 error-note">
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
