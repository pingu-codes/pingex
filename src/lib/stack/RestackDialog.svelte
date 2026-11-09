<!--
  Confirm a restack (or a reorder) from its plan: each git branch and what it
  is replayed onto, the conflicts a trial merge expects, which PRs' pushed
  commits it rewrites, and the threads it pauses. Resolves true, or null.
-->
<script lang="ts">
  import type { DialogClose } from "$lib/app/dialogs.svelte";
  import DialogShell from "$lib/components/DialogShell.svelte";
  import type { RestackPlan } from "$lib/types";
  import { plural } from "$lib/utils/format";
  import { basename } from "$lib/utils/path";

  let {
    plan,
    title,
    confirmLabel,
    running = 0,
    close,
  }: {
    plan: RestackPlan;
    title: string;
    confirmLabel: string;
    /** Threads working in the worktree now: the restack waits for them. */
    running?: number;
    close: DialogClose<true>;
  } = $props();

  const moving = $derived(plan.steps.filter((step) => !step.unchanged));
  const conflicts = $derived(plan.steps.reduce((sum, step) => sum + step.conflicts.length, 0));
</script>

<DialogShell
  {title}
  subtitle={plan.worktree ? `Runs in ${basename(plan.worktree)}` : null}
  width={480}
  onClose={() => close()}
>
  <div class="flex flex-col gap-3 text-xs">
    <div class="panel-card flex flex-col p-1" role="list" aria-label="Restack plan">
      {#each [...plan.steps].reverse() as step (step.number)}
        <div class="menu-row" role="listitem" aria-label="#{step.number} {step.headRef}">
          <span class="font-mono text-[11px] text-surface-500">#{step.number}</span>
          <span class="grow truncate font-mono text-[11px]">{step.headRef}</span>
          {#if step.unchanged}
            <span class="text-surface-500">Already on {step.onto}</span>
          {:else}
            <span class="text-surface-500">{plural(step.commits, "commit")} onto {step.onto}</span>
          {/if}
          {#if step.conflicts.length > 0}
            <span class="status-badge preset-tonal-warning">Conflict in {step.conflicts.map(basename).join(", ")}</span>
          {/if}
        </div>
      {/each}
    </div>
    {#if !plan.predicted}
      <p class="text-surface-500">
        This git can't predict conflicts (it needs git 2.40), so any will show as the restack runs.
      </p>
    {:else if conflicts > 0}
      <p>
        {conflicts === 1 ? "One conflict is" : `${conflicts} conflicts are`}
        expected. The restack stops there so you can resolve it in a thread or your editor, or abort and put every git
        branch back.
      </p>
    {/if}
    {#if plan.rewritesPushed.length > 0}
      <p class="error-note">
        This rewrites pushed commits on {plan.rewritesPushed.map((number) => `#${number}`).join(", ")}. GitHub only sees
        the result when you push the stack.
      </p>
    {/if}
    {#if running > 0}
      <p class="error-note">
        {running}
        thread{running === 1 ? " is" : "s are"}
        working in this worktree. Restack once
        {running === 1 ? "it finishes" : "they finish"}, so no agent edits files mid-rebase.
      </p>
    {/if}
    {#if moving.length === 0}
      <p class="text-surface-500">Every git branch is already where it should be.</p>
    {/if}
  </div>
  {#snippet footer()}
    <button type="button" class="btn btn-sm hover:preset-tonal" onclick={() => close()}>Cancel</button>
    <button
      type="button"
      class="btn btn-sm preset-filled-primary-500"
      disabled={moving.length === 0 || running > 0}
      onclick={() => close(true)}
    >
      {confirmLabel}
    </button>
  {/snippet}
</DialogShell>
