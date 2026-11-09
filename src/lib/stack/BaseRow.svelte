<!-- The base at the bottom of the Stack page's tree: how far it has moved past
     the stack, and what a restack would do. Dropping a PR here moves it to the
     bottom of the stack. -->
<script lang="ts">
  import type { RestackPlan, StackOverview } from "$lib/types";
  import { plural, shortSha } from "$lib/utils/format";
  import { basename } from "$lib/utils/path";

  let {
    baseRef,
    overview,
    preview,
    dropHint,
    onDragOver,
    onDrop,
  }: {
    baseRef: string;
    overview: StackOverview | null;
    /** The restack preview, when one is needed. */
    preview: RestackPlan | null;
    /** What dropping here does, while a drag is over the base. */
    dropHint: string | null;
    onDragOver: (event: DragEvent, target: "base") => void;
    onDrop: (event: DragEvent, target: "base") => void;
  } = $props();
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="flex gap-3" ondragover={(event) => onDragOver(event, "base")} ondrop={(event) => onDrop(event, "base")}>
  <div class="relative w-10 shrink-0" aria-hidden="true">
    <div class="absolute top-0 left-[27px] h-3.5 w-0.5 bg-surface-300-700"></div>
    <div class="absolute top-3 left-[21px] size-3.5 rounded-full bg-surface-500"></div>
  </div>
  <div class="flex min-w-0 grow flex-col gap-2 py-2.5" aria-label="Base {baseRef}">
    {#if dropHint}
      <div class="drop-line" aria-live="polite">{dropHint}</div>
    {/if}
    <div class="flex items-center gap-2">
      <span class="font-mono text-[11px] font-semibold">{baseRef}</span>
      {#if overview && overview.baseAhead > 0}
        <span class="status-badge preset-tonal-warning"
          >{plural(overview.baseAhead, "commit")}
          ahead of stack base</span
        >
      {:else if overview}
        <span class="text-surface-500">The stack is on its tip</span>
      {/if}
    </div>
    {#if overview && (overview.baseCommits.length > 0 || preview)}
      <div class="panel-card flex flex-col gap-1.5 bg-surface-50-950 px-3 py-2.5">
        {#each overview.baseCommits as commit (commit.sha)}
          <div class="flex gap-2">
            <span class="font-mono text-[11px] text-surface-500">{shortSha(commit.sha)}</span>
            <span class="truncate">{commit.subject}</span>
          </div>
        {/each}
        {#if preview}
          {@const expected = preview.steps.flatMap((step) => step.conflicts)}
          <div class="border-t border-surface-200-800 pt-1 text-surface-500">
            Restack rebases all {preview.steps.filter((step) => !step.unchanged).length} git branches in the worktree.
            {#if expected.length === 1}
              One conflict expected in <span class="font-mono">{basename(expected[0])}</span>.
            {:else if expected.length > 1}
              {expected.length}
              conflicts expected.
            {:else if preview.predicted}
              No conflicts expected.
            {/if}
          </div>
        {/if}
      </div>
    {/if}
  </div>
</div>
