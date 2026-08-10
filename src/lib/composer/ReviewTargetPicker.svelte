<!--
  What `/review` should look at, in the shape the Codex TUI uses: pick the
  working tree outright, or step into a list of base branches or commits.

  Two stages rather than one long list, because `PickerList` has no notion of a
  section header — Escape steps back out of a list before it closes the picker.
-->
<script lang="ts">
import PickerList from "$lib/composer/PickerList.svelte";
import {
  filterBranches,
  filterCommits,
  filterModes,
  REVIEW_MODES,
  type ReviewMode,
  type ReviewModeOption,
} from "$lib/composer/reviewTargets";
import { gitBranches, gitRecentCommits } from "$lib/services/api";
import type { GitBranch, GitCommit, ReviewTarget } from "$lib/types";

let {
  cwd,
  query,
  scope = null,
  onPick,
  onClose,
  onCount,
  onStageChange,
}: {
  cwd: string;
  /** Anything typed after `/review` opened the picker, used to filter. */
  query: string;
  scope?: HTMLElement | null;
  onPick: (target: ReviewTarget) => void;
  onClose: () => void;
  onCount?: (count: number) => void;
  /** Lets the composer clear the typed filter when the list changes under it. */
  onStageChange?: () => void;
} = $props();

let stage = $state<ReviewMode>("uncommittedChanges");
let branches = $state<GitBranch[]>([]);
let commits = $state<GitCommit[]>([]);
let loadError = $state<string | null>(null);

const listLabel = $derived((REVIEW_MODES.find((mode) => mode.id === stage) ?? REVIEW_MODES[0]).listLabel);
const emptyMessage = $derived((REVIEW_MODES.find((mode) => mode.id === stage) ?? REVIEW_MODES[0]).emptyMessage);

function chooseMode(mode: ReviewModeOption) {
  if (mode.id === "uncommittedChanges") {
    onPick({ type: "uncommittedChanges" });
    return;
  }
  stage = mode.id;
  loadError = null;
  onStageChange?.();
  void load(mode.id);
}

async function load(mode: ReviewMode) {
  try {
    if (mode === "baseBranch") branches = await gitBranches(cwd);
    else commits = await gitRecentCommits(cwd, 50);
  } catch (cause) {
    loadError = cause instanceof Error ? cause.message : String(cause);
  }
}

/** Escape steps back to the mode list first, and only then closes. */
function back() {
  if (stage === "uncommittedChanges") {
    onClose();
    return;
  }
  stage = "uncommittedChanges";
  loadError = null;
  onStageChange?.();
}
</script>

{#if stage === "uncommittedChanges"}
  <PickerList
    items={filterModes(query ?? "")}
    label="Review targets"
    emptyMessage={`No review target matches “${query}”.`}
    {scope}
    onPick={chooseMode}
    onClose={back}
    {onCount}
    key={(mode) => mode.id}
  >
    {#snippet row(mode: ReviewModeOption)}
      <span class="min-w-0 flex-1 truncate font-medium">{mode.label}</span>
    {/snippet}
  </PickerList>
{:else if stage === "baseBranch"}
  <PickerList
    items={filterBranches(branches, query ?? "")}
    label={listLabel}
    {emptyMessage}
    error={loadError}
    {scope}
    onPick={(branch) => onPick({ type: "baseBranch", branch: branch.name })}
    onClose={back}
    {onCount}
    key={(branch) => branch.name}
  >
    {#snippet row(branch: GitBranch)}
      <span class="min-w-0 flex-1 truncate font-medium">{branch.name}</span>
      {#if branch.isCurrent}
        <span class="shrink-0 text-[10px] text-surface-500">current</span>
      {:else if branch.isRemote}
        <span class="shrink-0 text-[10px] text-surface-500">remote</span>
      {/if}
    {/snippet}
  </PickerList>
{:else}
  <PickerList
    items={filterCommits(commits, query ?? "")}
    label={listLabel}
    {emptyMessage}
    error={loadError}
    {scope}
    onPick={(commit) => onPick({ type: "commit", sha: commit.hash, title: commit.subject })}
    onClose={back}
    {onCount}
    key={(commit) => commit.hash}
  >
    {#snippet row(commit: GitCommit)}
      <span class="shrink-0 font-mono text-[10px] text-surface-500">{commit.shortHash}</span>
      <span class="min-w-0 flex-1 truncate">{commit.subject}</span>
    {/snippet}
  </PickerList>
{/if}
