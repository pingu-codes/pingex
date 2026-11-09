<!-- What the Stack page offers once PRs in the stack merged: restack the rest
     and delete the merged git branches, or, with every PR merged, remove the
     worktree. -->
<script lang="ts">
  import type { MergeFollowUp } from "$lib/stack/stack";

  let {
    followUp,
    baseRef,
    worktreeName,
    busy,
    onRestackAndCleanUp,
    onRemoveWorktree,
    onLater,
  }: {
    followUp: MergeFollowUp;
    baseRef: string;
    worktreeName: string | null;
    /** A stack operation is running. */
    busy: boolean;
    onRestackAndCleanUp: () => void;
    onRemoveWorktree: () => void;
    onLater: () => void;
  } = $props();
</script>

{#if followUp.kind === "restack"}
  <div class="card flex flex-col gap-2 preset-tonal-success px-3 py-2.5" role="status">
    <div class="font-medium">
      {followUp.merged.map((number) => `#${number}`).join(", ")}
      merged into {baseRef}
    </div>
    <div>
      Restack {followUp.rest.map((number) => `#${number}`).join(" and ")} onto {baseRef} and delete
      {followUp.branches.join(", ")}?
    </div>
    <div class="flex gap-1.5">
      <button
        type="button"
        class="btn btn-sm text-[11px] preset-filled-primary-500"
        disabled={busy}
        onclick={onRestackAndCleanUp}
      >
        Restack and clean up
      </button>
      <button type="button" class="btn btn-sm text-[11px] hover:preset-tonal" onclick={onLater}>Later</button>
    </div>
  </div>
{:else if followUp.kind === "cleanup"}
  <div class="card flex flex-col gap-2 preset-tonal-success px-3 py-2.5" role="status">
    <div class="font-medium">Every PR in this stack merged</div>
    <div>Remove {worktreeName} and archive its threads?</div>
    <div class="flex gap-1.5">
      <button
        type="button"
        class="btn btn-sm text-[11px] preset-filled-primary-500"
        disabled={busy}
        onclick={onRemoveWorktree}
      >
        Remove worktree
      </button>
      <button type="button" class="btn btn-sm text-[11px] hover:preset-tonal" onclick={onLater}>Later</button>
    </div>
  </div>
{/if}
