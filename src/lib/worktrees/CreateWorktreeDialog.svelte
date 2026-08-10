<script lang="ts">
import { FolderOpen, GitBranch } from "@lucide/svelte";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { type DialogClose, submitState } from "$lib/app/dialogs.svelte";
import DialogShell from "$lib/components/DialogShell.svelte";
import TooltipButton from "$lib/components/TooltipButton.svelte";
import { isTauri } from "$lib/services/api";
import type { GitCommit, WorktreeBranchRequest } from "$lib/types";
import { folderName } from "$lib/worktrees/worktrees";

let {
  codexHome = null,
  repoDir = "",
  commits = [],
  submit,
  close,
}: {
  codexHome?: string | null;
  repoDir?: string;
  commits?: GitCommit[];
  /** Performs the creation; rejecting keeps the dialog open with the error. */
  submit: (path: string, branch: WorktreeBranchRequest) => Promise<void>;
  close: DialogClose<true>;
} = $props();

type Mode = "new" | "existing";
let mode = $state<Mode>("new");
let branchName = $state("");
let existingBranch = $state("");
let base = $state("");
let location = $state("");
let locationTouched = $state(false);
const action = submitState();

const slug = (value: string) =>
  value
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9-_]+/g, "-")
    .replace(/^-+|-+$/g, "");

// The default agent-created location follows the Codex-home worktree
// convention; a user can override it (or browse to a custom directory).
const suggestedName = $derived(mode === "new" ? branchName : existingBranch);
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
  return mode === "new" ? branchName.trim().length > 0 : existingBranch.trim().length > 0;
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
  const branch: WorktreeBranchRequest =
    mode === "new"
      ? { kind: "new", name: branchName.trim(), base: base.trim() || null }
      : { kind: "existing", name: existingBranch.trim() };
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
          {#each commits as commit (commit.hash)}
            <option value={commit.hash}>{commit.shortHash} · {commit.subject}</option>
          {/each}
        </select>
      </label>
    {:else}
      <label class="block">
        <span class="text-xs font-medium text-surface-600-400">Existing local branch</span>
        <input
          type="text"
          bind:value={existingBranch}
          placeholder="main"
          class="mt-1 w-full rounded-md border border-surface-300-700 bg-surface-50-950 px-2.5 py-1.5 font-mono text-sm outline-none focus:border-primary-500"
        />
      </label>
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
