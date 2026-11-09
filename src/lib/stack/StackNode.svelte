<!-- One PR's node in the Stack page's tree: its place in the stack, its git
     branch and state, and its menu. It drags to reorder; the page holds what
     is being dragged and where. -->
<script lang="ts">
  import { Ellipsis } from "@lucide/svelte";
  import ActionMenu from "$lib/components/ActionMenu.svelte";
  import type { MenuEntry } from "$lib/components/actionMenu";
  import { RING_CLASS, TONE_CLASS } from "$lib/inbox/badges";
  import { ciBadge, reviewBadge, stateTone } from "$lib/inbox/inbox";
  import WorktreeBadge from "$lib/review/WorktreeBadge.svelte";
  import type { InboxPr, PrCheckout, StackBranch, StackInputPr } from "$lib/types";
  import { plural, shortSha } from "$lib/utils/format";

  let {
    pr,
    index,
    total,
    inboxPr,
    branch,
    drift,
    threads,
    checkedOut,
    checkout,
    checkoutKnown,
    entries,
    selected,
    dragging,
    dropHint,
    showCommits,
    onSelect,
    onBranchAction,
    onDragStart,
    onDragOver,
    onDrop,
    onDragEnd,
  }: {
    pr: StackInputPr;
    /** From the top of the tree. */
    index: number;
    /** Open PRs in the stack. */
    total: number;
    /** The PR as GitHub has it. */
    inboxPr: InboxPr | undefined;
    branch: StackBranch | undefined;
    /** How this PR has drifted from the one below it. */
    drift: string | null;
    /** Open threads linked to this PR. */
    threads: number;
    /** The stack's worktree is on this PR's git branch. */
    checkedOut: boolean;
    checkout: PrCheckout | null;
    /** The checkouts read has answered. */
    checkoutKnown: boolean;
    entries: MenuEntry[];
    selected: boolean;
    dragging: boolean;
    /** What dropping here does, while a drag is over this node. */
    dropHint: string | null;
    showCommits: boolean;
    onSelect: (number: number) => void;
    onBranchAction: (id: string, pr: StackInputPr) => void;
    onDragStart: (event: DragEvent, number: number) => void;
    onDragOver: (event: DragEvent, target: number) => void;
    onDrop: (event: DragEvent, target: number) => void;
    onDragEnd: () => void;
  } = $props();

  const review = $derived(inboxPr ? reviewBadge(inboxPr) : null);
  const ci = $derived(inboxPr ? ciBadge(inboxPr) : null);
</script>

<div class="flex gap-3">
  <div class="relative w-10 shrink-0" aria-hidden="true">
    <div class="absolute top-0 bottom-0 left-[27px] w-0.5 bg-surface-300-700" class:top-5={index === 0}></div>
    <div
      class="stack-ring {inboxPr ? RING_CLASS[stateTone(inboxPr)] : RING_CLASS.surface}"
      class:border-dashed={inboxPr?.isDraft}
      title="{total - index} of {total} in the stack"
    >
      {total - index}
    </div>
  </div>
  <div class="mb-2.5 flex min-w-0 grow flex-col">
    {#if dropHint}
      <div class="drop-line mb-1.5" aria-live="polite">{dropHint}</div>
    {/if}
    <ActionMenu {entries} label="Branch {pr.headRef}" onSelect={(id) => onBranchAction(id, pr)}>
      {#snippet row({ rowProps, buttonProps })}
        <div
          {...rowProps}
          role="button"
          aria-label="#{pr.number} {pr.headRef}"
          aria-pressed={selected}
          class="stack-node"
          data-selected={selected}
          data-dragging={dragging}
          draggable="true"
          ondragstart={(event) => onDragStart(event, pr.number)}
          ondragover={(event) => onDragOver(event, pr.number)}
          ondrop={(event) => onDrop(event, pr.number)}
          ondragend={onDragEnd}
          onclick={() => onSelect(pr.number)}
        >
          <div class="flex items-center gap-2">
            <span class="min-w-0 truncate font-mono text-[11px]" class:font-semibold={checkedOut} title={pr.headRef}
              >{pr.headRef}</span
            >
            {#if checkedOut}
              <span class="status-badge preset-tonal-primary">Checked out</span>
            {:else if branch && !branch.local}
              <span class="shrink-0 text-[10px] text-surface-500">No local branch</span>
            {/if}
            <WorktreeBadge variant="icon" {checkout} known={checkoutKnown} noneHint="Check out stack creates one." />
            <span class="grow"></span>
            <button {...buttonProps} type="button" class="btn-icon btn-icon-sm hover:preset-tonal shrink-0">
              <Ellipsis size={14} aria-hidden="true" />
            </button>
          </div>
          <div class="flex min-w-0 items-center gap-2">
            <span class="font-mono text-[11px] text-surface-500">#{pr.number}</span>
            <span class="truncate" class:font-medium={selected}>{pr.title}</span>
          </div>
          <div class="flex flex-wrap items-center gap-2 text-surface-500">
            {#if review}
              <span class="status-badge {TONE_CLASS[review.tone]}">{review.label}</span>
            {/if}
            {#if branch}
              <span>{plural(branch.commits, "commit")}</span>
            {/if}
            {#if inboxPr && inboxPr.unresolvedThreads > 0}
              <span>·</span><span class="text-surface-950-50">{inboxPr.unresolvedThreads} unresolved</span>
            {:else if ci}
              <span>·</span><span>{ci.label}</span>
            {/if}
            {#if threads > 0}
              <span>·</span><span>{plural(threads, "thread")}</span>
            {/if}
            {#if branch?.unpushed}
              <span>·</span><span class="text-warning-600-400">Not pushed</span>
            {/if}
            {#if drift}
              <span>·</span><span class="text-warning-600-400">{drift}</span>
            {/if}
          </div>
          {#if showCommits && branch}
            <div class="flex flex-col gap-1 border-t border-surface-200-800 pt-1.5">
              {#each branch.commitLines as commit (commit.sha)}
                <div class="flex gap-2">
                  <span class="font-mono text-[11px] text-surface-500">{shortSha(commit.sha)}</span>
                  <span class="truncate">{commit.subject}</span>
                </div>
              {/each}
            </div>
          {/if}
        </div>
      {/snippet}
    </ActionMenu>
  </div>
</div>
