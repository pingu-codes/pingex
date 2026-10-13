<script lang="ts">
import { GitBranch } from "@lucide/svelte";
import { onMount } from "svelte";
import type { DialogClose } from "$lib/app/dialogs.svelte";
import BranchList from "$lib/components/BranchList.svelte";
import DialogShell from "$lib/components/DialogShell.svelte";
import DirectoryField from "$lib/components/DirectoryField.svelte";
import Segmented from "$lib/components/Segmented.svelte";
import { gitBranches, gitRecentCommits } from "$lib/services/api";
import type { GitBranch as Branch, GitCommit, WorktreeBranchRequest } from "$lib/types";
import { submitState } from "$lib/utils/actionState.svelte";
import { basename, trimSlash } from "$lib/utils/path";
import { resource } from "$lib/utils/resource.svelte";
import { stripRemotePrefix } from "$lib/worktrees/worktrees";

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
let base = $state("");
let location = $state("");
let locationTouched = $state(false);
const refs = resource(
  async () => {
    const [branches, commits] = await Promise.all([
      gitBranches(repoDir),
      gitRecentCommits(repoDir, 20).catch(() => [] as GitCommit[]),
    ]);
    return { branches, commits };
  },
  { initial: { branches: [] as Branch[], commits: [] as GitCommit[] } },
);
const branches = $derived(refs.value.branches);
const commits = $derived(refs.value.commits);
const action = submitState();

onMount(() => void refs.reload());

function pickBranch(branch: Branch) {
  selectedBranch = branch;
  existingQuery = branch.name;
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
  const home = trimSlash(codexHome ?? "~/.codex");
  return `${home}/worktrees/${basename(repoDir) || "repository"}/${name}`;
});

$effect(() => {
  // Track the suggested location until the user edits it themselves.
  if (!locationTouched) location = defaultLocation;
});

const canCreate = $derived.by(() => {
  if (action.busy || !location.trim()) return false;
  return mode === "new" ? branchName.trim().length > 0 : existingRequest !== null;
});

function setLocation(value: string) {
  locationTouched = true;
  location = value;
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

  <Segmented
    label="Branch source"
    value={mode}
    onChange={(next) => (mode = next)}
    size="md"
    fill
    class="mt-4"
    options={[
      { value: "new", label: "New branch" },
      { value: "existing", label: "Existing branch" },
    ]}
  />

  <div class="mt-4 space-y-3">
    {#if mode === "new"}
      <label class="block">
        <span class="text-xs font-medium text-surface-600-400">New branch name</span>
        <input
          type="text"
          bind:value={branchName}
          placeholder="feature/my-change"
          class="mt-1 w-full field font-mono text-sm"
        />
      </label>
      <label class="block">
        <span class="text-xs font-medium text-surface-600-400">Base revision</span>
        <select
          bind:value={base}
          class="mt-1 w-full field text-sm"
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
        <BranchList
          id="worktree-branch-list"
          label="Existing branch"
          showLabel
          combobox
          placeholder="Filter local and remote branches…"
          listClass="max-h-48"
          currentLabel="checked out"
          {branches}
          bind:query={existingQuery}
          onQueryInput={() => (selectedBranch = null)}
          loaded={!refs.loading}
          error={refs.error}
          selected={(branch) => selectedBranch?.name === branch.name}
          onPick={pickBranch}
        />
        {#if existingRequest?.kind === "tracking"}
          <p class="mt-1 text-[11px] text-surface-500">
            Creates local branch <span class="font-mono">{existingRequest.name}</span> tracking
            <span class="font-mono">{existingRequest.remoteRef}</span>.
          </p>
        {/if}
      </div>
    {/if}

    <div>
      <label for="worktree-location" class="text-xs font-medium text-surface-600-400">Location</label>
      <div class="mt-1">
        <DirectoryField
          id="worktree-location"
          value={location}
          onInput={setLocation}
          browseLabel="Browse for location"
          pickerTitle="Choose a worktree location"
        />
      </div>
      <p class="mt-1 text-[11px] text-surface-500">
        Defaults to the Codex-home worktree convention. Edit or browse for a custom directory.
      </p>
    </div>
  </div>

  {#if action.error}
    <p class="mt-3 error-note">{action.error}</p>
  {/if}

  {#snippet footer()}
    <button type="button" onclick={() => close()} class="btn btn-sm preset-tonal">Cancel</button>
    <button type="button" onclick={create} disabled={!canCreate} class="btn btn-sm preset-filled-primary-500 disabled:opacity-40">
      {action.busy ? "Creating…" : "Create worktree"}
    </button>
  {/snippet}
</DialogShell>
