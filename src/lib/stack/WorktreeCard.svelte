<!-- The stack's worktree on the Stack page: where it is, what it is on and what
     it holds, or the way to check the stack out when it has none. -->
<script lang="ts">
  import { Folder } from "@lucide/svelte";
  import { projects } from "$lib/app/appData.svelte";
  import type { StackWorktree } from "$lib/types";
  import { plural } from "$lib/utils/format";
  import { threadCountForPath } from "$lib/worktrees/worktrees";

  let {
    worktree,
    name,
    loaded,
    missingBranches,
    busy,
    restacking,
    onCheckOut,
  }: {
    worktree: StackWorktree | null;
    /** The worktree's folder name. */
    name: string | null;
    /** The overview has answered: no `worktree` then means there is none. */
    loaded: boolean;
    /** Open PRs with no local git branch yet. */
    missingBranches: number;
    /** A stack operation is running. */
    busy: boolean;
    /** A restack has not finished. */
    restacking: boolean;
    onCheckOut: () => void;
  } = $props();
</script>

{#if worktree}
  <div class="panel-card flex flex-col gap-1.5 bg-surface-50-950 px-3 py-2.5" aria-label="Worktree {name}">
    <div class="flex items-center gap-2">
      <Folder size={14} class="text-surface-500" aria-hidden="true" />
      <span class="font-semibold">{name}</span>
      <span class="grow"></span>
      <span class="status-badge {worktree.clean ? "preset-tonal-success" : "preset-tonal-warning"}"
        >{worktree.clean ? "Clean" : "Uncommitted changes"}</span
      >
    </div>
    <div class="truncate font-mono text-[11px] text-surface-500" title={worktree.path}>{worktree.path}</div>
    <div class="text-surface-500">
      On <span class="font-mono text-[11px]">{worktree.branch ?? "a detached HEAD"}</span>
    </div>
    <div class="text-surface-500">
      {plural(threadCountForPath(projects(), worktree.path), "thread")}
      here. Switching PR checks out its git branch; threads stay with the PR they were started on.
    </div>
    {#if missingBranches > 0}
      <div>
        <button type="button" class="btn btn-sm text-xs preset-tonal" disabled={busy} onclick={onCheckOut}>
          Create local branches ({missingBranches})
        </button>
      </div>
    {/if}
  </div>
{:else if loaded}
  <div class="empty-state flex flex-col items-start gap-2">
    <span>No worktree has this stack's git branches yet.</span>
    <button
      type="button"
      class="btn btn-sm text-xs preset-filled-primary-500"
      disabled={busy || restacking}
      onclick={onCheckOut}
    >
      Check out stack
    </button>
  </div>
{/if}
