<script lang="ts">
import { FolderOpen, GitBranch } from "@lucide/svelte";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { onMount } from "svelte";
import { type DialogClose, submitState } from "$lib/app/dialogs.svelte";
import DialogShell from "$lib/components/DialogShell.svelte";
import TooltipButton from "$lib/components/TooltipButton.svelte";
import { filterBranches } from "$lib/composer/reviewTargets";
import { gitBranches, gitRecentCommits, isTauri } from "$lib/services/api";
import type { GitBranch as Branch, GitCommit, WorktreeBranchRequest } from "$lib/types";
import { folderName, stripRemotePrefix } from "$lib/worktrees/worktrees";

let {
  codexHome = null,
  repoDir = "",
  submit,
  close,
}: {
  codexHome?: string | null;
  repoDir?: string;
  /** Performs the creation; rejecting keeps the dialog open with the error. */
  submit: (path: string, branch: WorktreeBranchRequest) => Promise<void>;
  close: DialogClose<true>;
} = $props();

type Mode = "new" | "existing";
let mode = $state<Mode>("new");
let branchName = $state("");
let existingQuery = $state("");
/** The row picked from the list; cleared as soon as the query is edited. */
let selectedBranch = $state<Branch | null>(null);
let activeIndex = $state(0);
let base = $state("");
let location = $state("");
let locationTouched = $state(false);
let branches = $state<Branch[]>([]);
let commits = $state<GitCommit[]>([]);
let loadError = $state<string | null>(null);
const action = submitState();

onMount(() => {
  Promise.all([gitBranches(repoDir), gitRecentCommits(repoDir, 20).catch(() => [] as GitCommit[])])
    .then(([refs, recent]) => {
      branches = refs;
      commits = recent;
    })
    .catch((cause) => {
      loadError = cause instanceof Error ? cause.message : String(cause);
    });
});

const matches = $derived(filterBranches(branches, existingQuery));
/** A branch checked out in another worktree cannot be added again. */
const pickable = (branch: Branch) => !branch.isCurrent;

$effect(() => {
  if (activeIndex >= matches.length) activeIndex = 0;
});

function pickBranch(branch: Branch) {
  if (!pickable(branch)) return;
  selectedBranch = branch;
  existingQuery = branch.name;
}

function onQueryKeydown(event: KeyboardEvent) {
  if (event.key === "ArrowDown") {
    event.preventDefault();
    activeIndex = Math.min(activeIndex + 1, matches.length - 1);
  } else if (event.key === "ArrowUp") {
    event.preventDefault();
    activeIndex = Math.max(activeIndex - 1, 0);
  } else if (event.key === "Enter") {
    const branch = matches[activeIndex];
    if (branch && branch.name !== existingQuery) {
      event.preventDefault();
      pickBranch(branch);
    }
  }
}

/** What `create()` will send for the existing-branch mode. */
const existingRequest = $derived.by((): WorktreeBranchRequest | null => {
  const typed = existingQuery.trim();
  if (!typed) return null;
  const branch = selectedBranch?.name === typed ? selectedBranch : (branches.find((b) => b.name === typed) ?? null);
  if (branch?.isRemote) return { kind: "tracking", name: stripRemotePrefix(branch.name), remoteRef: branch.name };
  return { kind: "existing", name: typed };
});

const slug = (value: string) =>
  value
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9-_]+/g, "-")
    .replace(/^-+|-+$/g, "");

// The default agent-created location follows the Codex-home worktree
// convention; a user can override it (or browse to a custom directory).
const suggestedName = $derived(mode === "new" ? branchName : (existingRequest?.name ?? ""));
const defaultLocation = $derived.by(() => {
  const name = slug(suggestedName) || "worktree";
  const home = (codexHome ?? "~/.codex").replace(/\/+$/, "");
  return `${home}/worktrees/${folderName(repoDir) || "repository"}/${name}`;
});

$effect(() => {
  // Track the suggested location until the user edits it themselves.
  if (!locationTouched) location = defaultLocation;
});

const canCreate = $derived.by(() => {
  if (action.busy || !location.trim()) return false;
  return mode === "new" ? branchName.trim().length > 0 : existingRequest !== null;
});

async function browse() {
  if (!isTauri()) return;
  const picked = await openDialog({ directory: true, multiple: false, title: "Choose a worktree location" });
  if (typeof picked === "string") {
    locationTouched = true;
    location = picked;
  }
}

async function create() {
  if (!canCreate) return;
  const branch: WorktreeBranchRequest | null =
    mode === "new" ? { kind: "new", name: branchName.trim(), base: base.trim() || null } : existingRequest;
  if (!branch) return;
  if (await action.run(() => submit(location.trim(), branch))) close(true);
}
</script>

<DialogShell title="Create worktree" width={500} onClose={() => close()}>
  {#snippet icon()}<GitBranch size={17} class="text-primary-500" />{/snippet}

  <div class="mt-4 flex gap-1 rounded-lg bg-surface-200-800 p-1 text-sm">
    <button
      type="button"
      onclick={() => (mode = "new")}
      class="flex-1 rounded-md px-3 py-1.5 font-medium transition {mode === 'new' ? 'bg-surface-50-950 shadow-sm' : 'text-surface-500 hover:text-surface-800-200'}"
    >
      New branch
    </button>
    <button
      type="button"
      onclick={() => (mode = "existing")}
      class="flex-1 rounded-md px-3 py-1.5 font-medium transition {mode === 'existing' ? 'bg-surface-50-950 shadow-sm' : 'text-surface-500 hover:text-surface-800-200'}"
    >
      Existing branch
    </button>
  </div>

  <div class="mt-4 space-y-3">
    {#if mode === "new"}
      <label class="block">
        <span class="text-xs font-medium text-surface-600-400">New branch name</span>
        <input
          type="text"
          bind:value={branchName}
          placeholder="feature/my-change"
          class="mt-1 w-full rounded-md border border-surface-300-700 bg-surface-50-950 px-2.5 py-1.5 font-mono text-sm outline-none focus:border-primary-500"
        />
      </label>
      <label class="block">
        <span class="text-xs font-medium text-surface-600-400">Base revision</span>
        <select
          bind:value={base}
          class="mt-1 w-full rounded-md border border-surface-300-700 bg-surface-50-950 px-2.5 py-1.5 text-sm outline-none focus:border-primary-500"
        >
          <option value="">Current HEAD</option>
          {#if branches.length > 0}
            <optgroup label="Branches">
              {#each branches as branch (branch.name)}
                <option value={branch.name}>{branch.name}</option>
              {/each}
            </optgroup>
          {/if}
          {#if commits.length > 0}
            <optgroup label="Recent commits">
              {#each commits as commit (commit.hash)}
                <option value={commit.hash}>{commit.shortHash} · {commit.subject}</option>
              {/each}
            </optgroup>
          {/if}
        </select>
      </label>
    {:else}
      <div>
        <label class="block">
          <span class="text-xs font-medium text-surface-600-400">Existing branch</span>
          <input
            type="text"
            value={existingQuery}
            oninput={(event) => {
              existingQuery = event.currentTarget.value;
              selectedBranch = null;
              activeIndex = 0;
            }}
            onkeydown={onQueryKeydown}
            placeholder="Filter local and remote branches…"
            role="combobox"
            aria-expanded="true"
            aria-controls="worktree-branch-list"
            aria-autocomplete="list"
            class="mt-1 w-full rounded-md border border-surface-300-700 bg-surface-50-950 px-2.5 py-1.5 font-mono text-sm outline-none focus:border-primary-500"
          />
        </label>
        <div
          id="worktree-branch-list"
          role="listbox"
          aria-label="Branches"
          class="mt-1 max-h-48 select-none overflow-y-auto rounded-md border border-surface-200-800 bg-surface-50-950 p-1"
        >
          {#if loadError}
            <p class="px-2 py-2 text-xs text-error-500">{loadError}</p>
          {:else if matches.length === 0}
            <p class="px-2 py-2 text-xs text-surface-500">
              {branches.length === 0 ? "No branches found." : `No branch matches “${existingQuery}”.`}
            </p>
          {:else}
            {#each matches as branch, index (branch.name)}
              <button
                type="button"
                role="option"
                aria-selected={selectedBranch?.name === branch.name}
                aria-disabled={!pickable(branch)}
                onmouseenter={() => (activeIndex = index)}
                onclick={() => pickBranch(branch)}
                class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left font-mono text-xs {index === activeIndex
                  ? 'preset-tonal'
                  : ''} {pickable(branch) ? '' : 'opacity-50'}"
              >
                <span class="min-w-0 flex-1 truncate">{branch.name}</span>
                {#if branch.isCurrent}
                  <span class="shrink-0 font-sans text-[10px] text-surface-500">checked out</span>
                {:else if branch.isRemote}
                  <span class="shrink-0 font-sans text-[10px] text-surface-500">remote</span>
                {/if}
              </button>
            {/each}
          {/if}
        </div>
        {#if existingRequest?.kind === "tracking"}
          <p class="mt-1 text-[11px] text-surface-500">
            Creates local branch <span class="font-mono">{existingRequest.name}</span> tracking
            <span class="font-mono">{existingRequest.remoteRef}</span>.
          </p>
        {/if}
      </div>
    {/if}

    <label class="block">
      <span class="text-xs font-medium text-surface-600-400">Location</span>
      <div class="mt-1 flex gap-2">
        <input
          type="text"
          value={location}
          oninput={(event) => {
            locationTouched = true;
            location = event.currentTarget.value;
          }}
          class="min-w-0 flex-1 rounded-md border border-surface-300-700 bg-surface-50-950 px-2.5 py-1.5 font-mono text-[13px] outline-none focus:border-primary-500"
        />
        {#if isTauri()}
          <TooltipButton label="Browse for location" type="button" onclick={browse} class="btn btn-sm preset-tonal shrink-0" aria-label="Browse for location">
            <FolderOpen size={14} />
          </TooltipButton>
        {/if}
      </div>
      <p class="mt-1 text-[11px] text-surface-500">
        Defaults to the Codex-home worktree convention. Edit or browse for a custom directory.
      </p>
    </label>
  </div>

  {#if action.error}
    <p class="mt-3 rounded-md preset-tonal-error px-3 py-2 text-xs">{action.error}</p>
  {/if}

  {#snippet footer()}
    <button type="button" onclick={() => close()} class="btn btn-sm preset-tonal">Cancel</button>
    <button type="button" onclick={create} disabled={!canCreate} class="btn btn-sm preset-filled-primary-500 disabled:opacity-40">
      {action.busy ? "Creating…" : "Create worktree"}
    </button>
  {/snippet}
</DialogShell>
