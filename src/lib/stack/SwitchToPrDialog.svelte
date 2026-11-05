<!-- Switch a stack's worktree to another of its PRs (Context menus board,
     "Worktree in the sidebar" → Switch to PR). Resolves the PR's git branch,
     or null. Running threads make the switch wait. -->
<script lang="ts">
  import type { DialogClose } from "$lib/app/dialogs.svelte";
  import DialogShell from "$lib/components/DialogShell.svelte";
  import type { StackInputPr } from "$lib/types";

  let {
    name,
    prs,
    current,
    running,
    close,
  }: {
    /** The worktree's name. */
    name: string;
    /** The stack's open PRs, bottom first. */
    prs: StackInputPr[];
    /** The git branch checked out there now. */
    current: string | null;
    /** Threads running in the worktree, and the PR each is on. */
    running: { prNumber: number | null }[];
    close: DialogClose<string>;
  } = $props();

  // svelte-ignore state_referenced_locally -- starts on the current one.
  let choice = $state<string | null>(current);
</script>

<DialogShell title="Switch {name} to a PR" subtitle="Checks out the PR's git branch here" onClose={() => close()}>
  <div class="flex flex-col gap-2 text-xs">
    <div class="panel-card flex flex-col p-1" role="radiogroup" aria-label="Switch to PR">
      {#each prs as pr (pr.number)}
        <label class="menu-row cursor-pointer">
          <input type="radio" name="switch-pr" value={pr.headRef} bind:group={choice}>
          <span class="font-mono text-[11px] text-surface-500">#{pr.number}</span>
          <span class="grow truncate">{pr.headRef}</span>
        </label>
      {/each}
    </div>
    {#if running.length > 0}
      <p class="text-surface-500">
        {running.length}
        thread{running.length === 1 ? " is" : "s are"}
        running{running[0]?.prNumber ? ` on #${running[0].prNumber}` : ""}. Switching waits until
        {running.length === 1 ? "it finishes" : "they finish"}.
      </p>
    {/if}
  </div>
  {#snippet footer()}
    <button type="button" class="btn btn-sm hover:preset-tonal" onclick={() => close()}>Cancel</button>
    <button
      type="button"
      class="btn btn-sm preset-filled-primary-500"
      disabled={!choice || choice === current}
      onclick={() => choice && close(choice)}
    >
      Switch
    </button>
  {/snippet}
</DialogShell>
