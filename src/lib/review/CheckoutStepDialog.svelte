<script lang="ts">
  /**
   * The confirmation a step plan asks for before a PR moves up a checkout
   * step, or before pushing to someone else's git branch. Resolves with the
   * chosen way up, or `null` on Cancel.
   */
  import { FolderGit2, GitBranch, Upload } from "@lucide/svelte";
  import type { DialogClose } from "$lib/app/dialogs.svelte";
  import DialogShell from "$lib/components/DialogShell.svelte";
  import Toggle from "$lib/components/Toggle.svelte";
  import { ACTION_LABELS } from "$lib/review/actions";
  import type { CheckoutTarget, DeskAction, StepConfirm } from "$lib/types";

  export type StepChoice = {
    /** How to go up: a review checkout, promoting it, a new worktree, or
     *  just going ahead (pushing). */
    way: "reviewCheckout" | "promote" | "worktree" | "proceed";
    /** "Don't ask again for <repository>". */
    dontAskAgain: boolean;
  };

  let {
    confirm,
    action,
    target,
    repoName,
    reviewCheckoutPath,
    close,
  }: {
    confirm: StepConfirm;
    action: DeskAction;
    target: CheckoutTarget;
    repoName: string;
    /** Where a review checkout goes or already is. */
    reviewCheckoutPath: string;
    close: DialogClose<StepChoice>;
  } = $props();

  let dontAskAgain = $state(false);
  const actionLabel = $derived(ACTION_LABELS[action]);
  const choose = (way: StepChoice["way"]) => close({ way, dontAskAgain });

  const title = $derived(
    {
      createReviewCheckout: `Create a review checkout of #${target.number}?`,
      promoteOrCreateWorktree: `Make a worktree for #${target.number}?`,
      createWorktree: `Create a worktree for #${target.number}?`,
      pushToTheirBranch: `Push to ${target.headRef}?`,
    }[confirm],
  );
</script>

<DialogShell {title} width={460} onClose={() => close()}>
  {#snippet icon()}
    {#if confirm === "pushToTheirBranch"}
      <Upload size={17} class="text-warning-500" />
    {:else if confirm === "createReviewCheckout"}
      <FolderGit2 size={17} class="text-primary-500" />
    {:else}
      <GitBranch size={17} class="text-primary-500" />
    {/if}
  {/snippet}

  {#if confirm === "createReviewCheckout"}
    <p class="mt-3 text-sm leading-6 text-surface-600-400">
      {actionLabel}
      needs a review checkout: a detached checkout at the PR head that Pingex owns. It stays out of your Worktrees list,
      and agents there can't commit or push.
    </p>
    <p class="mt-2 rounded-md bg-surface-100-900 px-2 py-1 font-mono text-[11px] text-surface-700-300">
      {reviewCheckoutPath}
    </p>
    <div class="mt-3 flex items-center gap-3 text-sm">
      <Toggle checked={dontAskAgain} label="Don't ask again for {repoName}" onChange={(on) => (dontAskAgain = on)} />
      <span>Don't ask again for {repoName}</span>
    </div>
  {:else if confirm === "promoteOrCreateWorktree"}
    <p class="mt-3 text-sm leading-6 text-surface-600-400">
      {actionLabel}
      needs a worktree on <span class="font-mono">{target.headRef}</span>. Promoting the review checkout keeps its path,
      so threads running there keep their place.
    </p>
    <p class="mt-2 rounded-md bg-surface-100-900 px-2 py-1 font-mono text-[11px] text-surface-700-300">
      {reviewCheckoutPath}
    </p>
  {:else if confirm === "createWorktree"}
    <p class="mt-3 text-sm leading-6 text-surface-600-400">
      {actionLabel}
      needs a worktree on <span class="font-mono">{target.headRef}</span>, where you and agents can commit and push. It
      shows under {repoName} in the sidebar.
    </p>
  {:else}
    <p class="mt-3 text-sm leading-6 text-surface-600-400">
      <span class="font-mono">{target.headRef}</span>
      belongs to the author of #{target.number}. Pushing changes their pull request.
    </p>
  {/if}

  {#snippet footer()}
    <button type="button" class="btn btn-sm preset-tonal" onclick={() => close()}>Cancel</button>
    {#if confirm === "createReviewCheckout"}
      <button type="button" class="btn btn-sm preset-filled-primary-500" onclick={() => choose("reviewCheckout")}>
        Create review checkout
      </button>
    {:else if confirm === "promoteOrCreateWorktree"}
      <button type="button" class="btn btn-sm preset-tonal" onclick={() => choose("worktree")}>
        Create a new worktree
      </button>
      <button type="button" class="btn btn-sm preset-filled-primary-500" onclick={() => choose("promote")}>
        Promote the review checkout
      </button>
    {:else if confirm === "createWorktree"}
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
