<!-- A restack on the Stack page: each git branch's progress, then the way out
     of a conflict, or Push stack once it finished. -->
<script lang="ts">
  import { commands } from "$lib/services/homeRouting";
  import { toastFailure } from "$lib/toaster";
  import type { RestackBranch, RestackProgress } from "$lib/types";
  import { plural } from "$lib/utils/format";
  import { basename } from "$lib/utils/path";

  let {
    progress,
    conflict,
    busy,
    onResolve,
    onContinue,
    onAbort,
    onPush,
    onDismiss,
  }: {
    progress: RestackProgress;
    /** The git branch stopped on a conflict. */
    conflict: RestackBranch | null;
    /** A stack operation is running. */
    busy: boolean;
    onResolve: () => void;
    onContinue: () => void;
    onAbort: () => void;
    onPush: () => void;
    onDismiss: () => void;
  } = $props();
</script>

<div class="panel-card flex flex-col gap-2 px-3 py-2.5" aria-label="Restack progress">
  <div class="flex items-center gap-2">
    <span class="grow font-medium"
      >{progress.done ? "Restacked" : "Restacking"}
      onto {progress.ontoRef}</span
    >
    <span class="text-surface-500">{basename(progress.worktree)}</span>
  </div>
  {#each progress.branches as entry (entry.number)}
    <div class="flex items-center gap-2" class:text-surface-500={entry.status === "waiting"}>
      {#if entry.status === "conflict"}
        <span class="size-3 rounded-full bg-warning-500" aria-hidden="true"></span>
      {:else if entry.status === "waiting"}
        <span class="size-3 rounded-full border-2 border-dashed border-surface-400-600" aria-hidden="true"></span>
      {:else}
        <span class="size-3 rounded-full bg-success-500" aria-hidden="true"></span>
      {/if}
      <span class="grow font-mono text-[11px]">{entry.headRef}</span>
      {#if entry.status === "conflict"}
        <span class="status-badge preset-tonal-warning">Conflict in {entry.conflicts.map(basename).join(", ")}</span>
      {:else if entry.status === "waiting"}
        <span>Waiting</span>
      {:else if entry.status === "unchanged"}
        <span class="text-surface-500">Already in place</span>
      {:else}
        <span class="text-surface-500"
          >{plural(entry.commits, "commit")}
          replayed</span
        >
      {/if}
    </div>
  {/each}
  {#if conflict}
    <div class="flex flex-wrap gap-1.5 border-t border-surface-200-800 pt-1.5">
      <button type="button" class="btn btn-sm text-[11px] preset-tonal" onclick={onResolve}>Resolve in a thread</button>
      <button
        type="button"
        class="btn btn-sm text-[11px] hover:preset-tonal"
        onclick={() => void commands.openInZed(progress.worktree).catch(toastFailure)}
      >
        Open in editor
      </button>
      <button type="button" class="btn btn-sm text-[11px] hover:preset-tonal" disabled={busy} onclick={onContinue}>
        Continue
      </button>
      <span class="grow"></span>
      <button
        type="button"
        class="btn btn-sm text-[11px] text-error-600-400 hover:preset-tonal"
        disabled={busy}
        onclick={onAbort}
      >
        Abort restack
      </button>
    </div>
  {:else if progress.done}
    <div class="flex gap-1.5 border-t border-surface-200-800 pt-1.5">
      <button type="button" class="btn btn-sm text-[11px] preset-tonal" disabled={busy} onclick={onPush}>
        Push stack
      </button>
      <button type="button" class="btn btn-sm text-[11px] hover:preset-tonal" onclick={onDismiss}>Dismiss</button>
    </div>
  {/if}
</div>
