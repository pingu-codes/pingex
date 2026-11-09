<script lang="ts">
  /**
   * The confirmation a step plan asks for before a PR gets a worktree, or
   * before pushing to someone else's git branch. Resolves with the choice, or
   * `null` on Cancel.
   */
  import { GitBranch, Upload } from "@lucide/svelte";
  import type { DialogClose } from "$lib/app/dialogs.svelte";
  import DialogShell from "$lib/components/DialogShell.svelte";
  import Toggle from "$lib/components/Toggle.svelte";
  import { ACTION_LABELS } from "$lib/review/actions";
  import type { CheckoutTarget, DeskAction, StepConfirm } from "$lib/types";

  export type StepChoice = {
    /** Make the worktree, or just go ahead (pushing). */
    way: "worktree" | "proceed";
    /** "Don't ask again for <repository>". */
    dontAskAgain: boolean;
  };

  let {
    confirm,
    action,
    target,
    repoName,
    worktreePath,
    close,
  }: {
    confirm: StepConfirm;
    action: DeskAction;
    target: CheckoutTarget;
    repoName: string;
    /** Where the worktree goes or already is. */
    worktreePath: string;
    close: DialogClose<StepChoice>;
  } = $props();

  /** Actions the repository's policy decides for, so "Don't ask again" means
   *  something. The rest change the PR and always ask. */
  const FOLLOWS_POLICY: DeskAction[] = ["agentReviewWithTests", "runLocally", "openInEditor"];

  let dontAskAgain = $state(false);
  const actionLabel = $derived(ACTION_LABELS[action]);
  const choose = (way: StepChoice["way"]) => close({ way, dontAskAgain });

  const title = $derived(
    {
      createWorktree: `Create a worktree for #${target.number}?`,
      pushToTheirBranch: `Push to ${target.headRef}?`,
    }[confirm],
  );
</script>

<DialogShell {title} width={460} onClose={() => close()}>
  {#snippet icon()}
    {#if confirm === "pushToTheirBranch"}
      <Upload size={17} class="text-warning-500" />
    {:else}
      <GitBranch size={17} class="text-primary-500" />
    {/if}
  {/snippet}

  {#if confirm === "createWorktree"}
    <p class="mt-3 text-sm leading-6 text-surface-600-400">
      {actionLabel}
      needs a worktree.
      {#if target.mine}
        It goes on <span class="font-mono">{target.headRef}</span>, where you and agents can build, run, commit and
        push.
      {:else}
        Agents can build, run and commit there. Nothing reaches the author's branch until you push.
      {/if}
    </p>
    <p class="mt-2 rounded-md bg-surface-100-900 px-2 py-1 font-mono text-[11px] text-surface-700-300">
      {worktreePath}
    </p>
    {#if FOLLOWS_POLICY.includes(action)}
      <div class="mt-3 flex items-center gap-3 text-sm">
        <Toggle checked={dontAskAgain} label="Don't ask again for {repoName}" onChange={(on) => (dontAskAgain = on)} />
        <span>Don't ask again for {repoName}</span>
      </div>
    {/if}
  {:else}
    <p class="mt-3 text-sm leading-6 text-surface-600-400">
      <span class="font-mono">{target.headRef}</span>
      belongs to the author of #{target.number}. Pushing changes their pull request.
    </p>
  {/if}

  {#snippet footer()}
    <button type="button" class="btn btn-sm preset-tonal" onclick={() => close()}>Cancel</button>
    {#if confirm === "createWorktree"}
      <button type="button" class="btn btn-sm preset-filled-primary-500" onclick={() => choose("worktree")}>
        Create worktree
      </button>
    {:else}
      <button type="button" class="btn btn-sm preset-filled-warning-500" onclick={() => choose("proceed")}>
        Push to their branch
      </button>
    {/if}
  {/snippet}
</DialogShell>
