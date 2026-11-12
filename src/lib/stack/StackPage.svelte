<!--
  The Stack page (the canvas's "Stack" board): the git branch tree from the
  top PR down to the base, the stack's worktree, and the selected PR's review
  page beside it. A PR in a stack opens here too, however it was reached. The
  tree folds to a strip of its PRs. Restack onto the base, push the stack, and
  every stack edit (add, insert, reorder by drag or ⌥↑/⌥↓, the git branch menu)
  start here; after a PR merges it offers the follow-up.

  What runs, in what order, is the stack under work's (`stackUnderWork.svelte.ts`);
  this page lays it out, keeps the selection, and answers its questions with
  dialogs.
-->
<script lang="ts">
  import { ChevronLeft, PanelLeftClose, PanelLeftOpen, Sparkles } from "@lucide/svelte";
  import { onMount, tick, untrack } from "svelte";
  import { projectForCwd } from "$lib/app/appData.svelte";
  import { openDialog } from "$lib/app/dialogs.svelte";
  import { openReview, openThreadInCwd, type PrMove } from "$lib/app/navigation.svelte";
  import { place } from "$lib/app/places";
  import ConfirmDialog from "$lib/components/ConfirmDialog.svelte";
  import RefreshIcon from "$lib/components/RefreshIcon.svelte";
  import Segmented from "$lib/components/Segmented.svelte";
  import TooltipButton from "$lib/components/TooltipButton.svelte";
  import { inboxes } from "$lib/inbox/inboxStore.svelte";
  import CheckoutStepDialog from "$lib/review/CheckoutStepDialog.svelte";
  import { ensurePrLinks, prLinks } from "$lib/review/prLinks.svelte";
  import ReviewView from "$lib/review/ReviewView.svelte";
  import { loadStackTreeCollapsed, saveStackTreeCollapsed } from "$lib/review/reviewPrefs";
  import { prefetch } from "$lib/services/queryCache.svelte";
  import BaseRow from "$lib/stack/BaseRow.svelte";
  import BranchNameDialog from "$lib/stack/BranchNameDialog.svelte";
  import MergeFollowUpCard from "$lib/stack/MergeFollowUpCard.svelte";
  import RestackDialog from "$lib/stack/RestackDialog.svelte";
  import RestackProgressCard from "$lib/stack/RestackProgressCard.svelte";
  import SplitDialog from "$lib/stack/SplitDialog.svelte";
  import StackNode from "$lib/stack/StackNode.svelte";
  import { branchMenu, driftLabel, dropped, moved, openOrder } from "$lib/stack/stack";
  import { type StackQuestion, StackUnderWork } from "$lib/stack/stackUnderWork.svelte";
  import WorktreeCard from "$lib/stack/WorktreeCard.svelte";
  import type { StackInputPr } from "$lib/types";
  import { plural } from "$lib/utils/format";
  import { basename } from "$lib/utils/path";

  let {
    repoDir,
    repoName,
    stackNumber,
    initialPr = null,
    onBack,
    backLabel = "Pull requests",
    onAskCodex,
    restackOnOpen = false,
  }: {
    repoDir: string;
    repoName: string;
    stackNumber: number;
    /** The PR to open on; else the one shown last, or the checked-out one. */
    initialPr?: number | null;
    onBack: () => void;
    /** Names where `onBack` leads. */
    backLabel?: string;
    onAskCodex: (cwd: string, prompt: string) => void;
    /** Start a restack once the stack has loaded (the inbox's Restack). */
    restackOnOpen?: boolean;
  } = $props();

  // svelte-ignore state_referenced_locally -- the page remounts per navigation.
  const stack = new StackUnderWork({ repoDir, repoName, number: stackNumber, confirm: ask });
  // svelte-ignore state_referenced_locally -- likewise.
  const left = place(`stack:${repoDir}#${stackNumber}`, () => ({
    selected: null as number | null,
    view: "stack" as "stack" | "commits",
    pageScrollTop: 0,
    treeScrollTop: 0,
  }));
  // svelte-ignore state_referenced_locally -- likewise.
  let selected = $state<number | null>(initialPr);
  let view = $state(left.view);
  let treeCollapsed = $state(loadStackTreeCollapsed());
  let page = $state<HTMLElement>();
  let tree = $state<HTMLElement>();
  let dragging = $state<number | null>(null);
  let dropTarget = $state<number | "base" | null>(null);

  const repoState = $derived(inboxes[repoDir]);
  const input = $derived(stack.input);
  const order = $derived(input ? openOrder(input) : []);
  const openPrs = $derived(stack.openPrs);
  /** Top first, the way the tree draws them. */
  const topDown = $derived([...openPrs].reverse());
  const prByNumber = $derived(new Map((repoState?.inbox?.prs ?? []).map((pr) => [pr.number, pr])));
  const overview = $derived(stack.overview);
  const branchByNumber = $derived(new Map((overview?.branches ?? []).map((branch) => [branch.number, branch])));
  const totalCommits = $derived((overview?.branches ?? []).reduce((sum, branch) => sum + branch.commits, 0));
  const worktree = $derived(overview?.worktree ?? null);
  const worktreeName = $derived(worktree ? basename(worktree.path) : null);
  /** Open PRs with no local git branch yet. */
  const missingBranches = $derived((overview?.branches ?? []).filter((branch) => !branch.local).length);
  const blocked = $derived(stack.busy !== null || stack.restacking);

  // Unless opened for a PR, back on the one shown last, while it is still open.
  // svelte-ignore state_referenced_locally -- once, as the page opens.
  if (selected === null && openPrs.some((pr) => pr.number === left.selected)) selected = left.selected;

  $effect(() => {
    left.selected = selected;
    left.view = view;
  });

  // A restack started from the header has its progress and conflicts in the tree.
  $effect(() => {
    if (stack.restacking) treeCollapsed = false;
  });

  // Once what is on disk is read: the checked-out PR, else the top one.
  $effect(() => {
    const branch = overview?.worktree?.branch;
    if (!overview) return;
    untrack(() => {
      selected ??= openPrs.find((pr) => pr.headRef === branch)?.number ?? openPrs.at(-1)?.number ?? null;
    });
  });

  // Warm the PRs either side of the selected one, so a branch click shows
  // its review at once.
  $effect(() => {
    const at = openPrs.findIndex((pr) => pr.number === selected);
    if (at === -1) return;
    const around = [openPrs[at - 1], openPrs[at + 1]];
    untrack(() => {
      for (const pr of around) if (pr) prefetch("reviewPrDetail", repoDir, pr.number);
    });
  });

  onMount(() => {
    void ensurePrLinks();
    void tick().then(() => {
      if (page) page.scrollTop = left.pageScrollTop;
      if (tree) tree.scrollTop = left.treeScrollTop;
    });
    void stack.load({ restack: restackOnOpen });
  });

  function setTreeCollapsed(value: boolean) {
    treeCollapsed = value;
    saveStackTreeCollapsed(value);
  }

  function select(number: number) {
    selected = number;
  }

  /** Another PR, from Up next or j/k: within the stack it is a selection. */
  function openPr(number: number, move?: PrMove) {
    if (openPrs.some((pr) => pr.number === number)) {
      select(number);
      return;
    }
    const project = projectForCwd(repoDir);
    if (project) openReview(project, number, move);
  }

  function threadsOn(number: number): number {
    return Object.values(prLinks.byThread).filter(
      (link) => link.repoKey === repoDir && link.prNumber === number && link.closedAt === null,
    ).length;
  }

  /** The stack's questions, as dialogs. */
  async function ask(question: StackQuestion): Promise<boolean> {
    const baseRef = stack.input?.baseRef ?? "the base";
    switch (question.kind) {
      case "restack":
        return !!(await openDialog(RestackDialog, {
          plan: question.plan,
          title: question.from === null ? `Restack onto ${baseRef}` : `Restack from #${question.from}`,
          confirmLabel: "Restack",
          running: question.running,
        }));
      case "pushToTheirBranch":
        return !!(await openDialog(CheckoutStepDialog, {
          confirm: "pushToTheirBranch" as const,
          action: "push" as const,
          target: question.target,
          repoName,
          worktreePath: question.worktreePath,
        }));
      case "reorder": {
        const next = question.order;
        const headRef = (number: number) => openPrs.find((pr) => pr.number === number)?.headRef;
        const lines = next
          .map(
            (number, index) =>
              `#${number} ${headRef(number) ?? ""} onto ${index === 0 ? baseRef : headRef(next[index - 1])}`,
          )
          .reverse();
        return !!(await openDialog(ConfirmDialog, {
          title: "Reorder the stack",
          body: [
            "Each git branch is rebased onto the one now below it, the stack is force-pushed with leases, and the PRs are retargeted.",
            "This rewrites pushed commits.",
          ],
          detail: lines,
          confirmLabel: "Reorder",
          tone: "warning" as const,
        }));
      }
      case "fold": {
        const { pr, parent } = question;
        return !!(await openDialog(ConfirmDialog, {
          title: `Fold #${pr.number} into #${parent.number}`,
          body: `Squash ${pr.headRef} into ${parent.headRef} as one commit, replay the git branches above onto it, push, and close #${pr.number}.`,
          detail: `#${pr.number} ${pr.title}`,
          confirmLabel: "Fold and close",
        }));
      }
      case "remove": {
        const { pr } = question;
        return !!(await openDialog(ConfirmDialog, {
          title: `Remove #${pr.number} from the stack`,
          body: `#${pr.number} is rebased onto ${baseRef} and retargeted there, and stays open. The PR above it moves onto the one below.`,
          detail: `#${pr.number} ${pr.title}`,
          confirmLabel: "Remove from stack",
        }));
      }
      case "removeWorktree":
        return !!(await openDialog(ConfirmDialog, {
          title: `Remove ${basename(question.path)}`,
          body: `Every PR in this stack has merged. Remove the worktree and archive its ${plural(question.threads, "thread")}?`,
          detail: question.path,
          confirmLabel: "Remove and archive",
        }));
    }
  }

  async function resolveInThread() {
    const started = await stack.resolveInThread();
    if (started) openThreadInCwd(started.threadId, started.cwd);
  }

  async function addBranch(below: number | null) {
    if (!input) return;
    const name = await openDialog(BranchNameDialog, {
      title: below === null ? "Add a git branch on top" : `Insert a git branch below #${below}`,
      subtitle:
        below === null
          ? `It starts at ${openPrs.at(-1)?.headRef ?? input.baseRef}. Commit there, then open its PR.`
          : `#${below} is retargeted onto it.`,
      confirmLabel: below === null ? "Add git branch" : "Insert git branch",
    });
    if (name) await stack.edit({ kind: "add", name, below });
  }

  function reorder(next: number[] | null) {
    if (next) void stack.edit({ kind: "reorder", order: next });
  }

  async function runBranch(id: string, pr: StackInputPr) {
    switch (id) {
      case "checkout":
        return void stack.checkOut(pr.number);
      case "restackFrom":
        return void stack.restack(pr.number);
      case "push":
        return void stack.push(pr.number);
      case "moveUp":
        return reorder(moved(order, pr.number, "up"));
      case "moveDown":
        return reorder(moved(order, pr.number, "down"));
      case "insertBelow":
        return void addBranch(pr.number);
      case "split": {
        const split = await openDialog(SplitDialog, {
          number: pr.number,
          headRef: pr.headRef,
          commits: branchByNumber.get(pr.number)?.commitLines ?? [],
        });
        if (split) await stack.edit({ kind: "split", number: pr.number, split });
        return;
      }
      case "fold":
      case "remove":
        return void stack.edit({ kind: id, number: pr.number });
      case "rename": {
        const to = await openDialog(BranchNameDialog, {
          title: `Rename ${pr.headRef}`,
          subtitle: `#${pr.number} and the PR above it follow the new name on GitHub.`,
          initial: pr.headRef,
          confirmLabel: "Rename",
        });
        if (to) await stack.edit({ kind: "rename", number: pr.number, to });
        return;
      }
    }
  }

  // --- Drag to reorder: nothing moves until the drop ---

  function onDragStart(event: DragEvent, number: number) {
    dragging = number;
    event.dataTransfer?.setData("text/plain", String(number));
    if (event.dataTransfer) event.dataTransfer.effectAllowed = "move";
  }

  function onDragOver(event: DragEvent, target: number | "base") {
    if (dragging === null || target === dragging) return;
    event.preventDefault();
    dropTarget = target;
  }

  function onDrop(event: DragEvent, target: number | "base") {
    event.preventDefault();
    const dragged = dragging;
    dragging = null;
    dropTarget = null;
    if (dragged === null) return;
    reorder(dropped(order, dragged, target === "base" ? null : target));
  }

  function onDragEnd() {
    dragging = null;
    dropTarget = null;
  }

  const dropHint = $derived(
    dragging !== null && dropTarget !== null
      ? `Drop to rebase onto ${dropTarget === "base" ? (input?.baseRef ?? "the base") : `#${dropTarget}`} · retargets PR`
      : null,
  );
</script>

<div class="flex h-full min-h-0 flex-col text-xs">
  <header class="flex flex-wrap items-center gap-2 border-b border-surface-200-800 px-5 py-3">
    <button type="button" class="btn btn-sm hover:preset-tonal px-1 text-surface-500" onclick={onBack}>
      <ChevronLeft size={14} aria-hidden="true" />{backLabel}
    </button>
    <span class="text-surface-500">/</span>
    <span class="text-surface-500">{repoName}</span>
    <span class="text-surface-500">/</span>
    <h1 class="text-sm font-semibold">{input?.name ?? `Stack ${stackNumber}`}</h1>
    {#if input}
      <span class="status-badge preset-tonal"
        >{plural(openPrs.length, "PR")}{overview ? ` · ${plural(totalCommits, "commit")}` : ""}</span
      >
    {/if}
    <!-- The stack's actions wrap as one group. -->
    <div class="ml-auto flex items-center gap-2">
      <button
        type="button"
        class="btn btn-sm text-xs {stack.theirs ? "preset-filled-primary-500" : "hover:preset-tonal"}"
        disabled={openPrs.length === 0 || stack.busy !== null}
        onclick={() => void stack.reviewWhole()}
      >
        <Sparkles size={14} aria-hidden="true" />Review whole stack
      </button>
      <button
        type="button"
        class="btn btn-sm text-xs preset-tonal"
        disabled={!input || blocked}
        onclick={() => void stack.push()}
      >
        Push stack
      </button>
      <button
        type="button"
        class="btn btn-sm text-xs {stack.theirs ? "hover:preset-tonal" : "preset-filled-primary-500"}"
        disabled={!input || blocked}
        onclick={() => void stack.restack()}
      >
        <RefreshIcon spinning={stack.busy === "restack"} size={14} />Restack onto
        {input?.baseRef ?? "base"}
      </button>
    </div>
  </header>

  {#if !input}
    <div class="p-6">
      {#if repoState?.error}
        <div class="error-note">{repoState.error}</div>
      {:else if repoState?.syncing || !repoState}
        <div class="placeholder h-10 animate-pulse rounded-md" aria-label="Loading the stack"></div>
      {:else}
        <div class="empty-state">This stack is no longer on GitHub.</div>
      {/if}
    </div>
  {:else}
    <!-- Side by side (400 + 560px) each pane scrolls on its own; stacked, the
         page scrolls and the PR pane takes the full height under the tree. -->
    <div
      bind:this={page}
      class="@container/stack flex min-h-0 grow flex-wrap items-stretch overflow-y-auto"
      onscroll={(event) => (left.pageScrollTop = event.currentTarget.scrollTop)}
    >
      {#if treeCollapsed}
        <!-- Beside the PR, or a row above it when the two don't fit. -->
        <nav
          aria-label="Stack branches"
          class="flex w-14 shrink-0 flex-col items-center gap-1 overflow-y-auto border-r border-surface-200-800 bg-surface-100-900 py-3 @min-[620px]/stack:h-full @max-[619px]/stack:w-full @max-[619px]/stack:flex-row @max-[619px]/stack:overflow-x-auto @max-[619px]/stack:border-r-0 @max-[619px]/stack:border-b @max-[619px]/stack:px-3 @max-[619px]/stack:py-1.5"
        >
          <TooltipButton
            type="button"
            label="Expand stack"
            class="btn-icon btn-icon-sm hover:preset-tonal shrink-0 text-surface-500"
            aria-expanded={false}
            onclick={() => setTreeCollapsed(false)}
          >
            <PanelLeftOpen size={14} aria-hidden="true" />
          </TooltipButton>
          {#if stack.followUp}
            <span
              class="size-2 shrink-0 rounded-full bg-success-500"
              role="img"
              aria-label="A PR merged: the stack has a follow-up"
              title="A PR merged: the stack has a follow-up"
            ></span>
          {/if}
          {#each topDown as pr (pr.number)}
            <button
              type="button"
              class="shrink-0 rounded px-1 py-0.5 font-mono text-[11px] {selected === pr.number
                ? "preset-tonal-primary font-semibold"
                : "hover:preset-tonal text-surface-500"}"
              aria-current={selected === pr.number}
              title="#{pr.number} {pr.title}"
              onclick={() => select(pr.number)}
            >
              #{pr.number}
            </button>
          {/each}
        </nav>
      {:else}
        <section
          bind:this={tree}
          aria-label="Stack tree"
          class="flex min-w-0 flex-[1_1_400px] flex-col gap-3 border-r border-surface-200-800 bg-surface-100-900 p-5 @min-[960px]/stack:h-full @min-[960px]/stack:overflow-y-auto"
          onscroll={(event) => (left.treeScrollTop = event.currentTarget.scrollTop)}
        >
          <div class="flex items-center gap-2">
            <h2 class="eyebrow grow">Branches</h2>
            <Segmented
              label="Tree view"
              value={view}
              options={[
                { value: "stack", label: "Stack" },
                { value: "commits", label: "Commits" },
              ]}
              onChange={(value) => (view = value)}
            />
            <TooltipButton
              type="button"
              label="Collapse stack"
              class="btn-icon btn-icon-sm hover:preset-tonal shrink-0 text-surface-500"
              aria-expanded={true}
              onclick={() => setTreeCollapsed(true)}
            >
              <PanelLeftClose size={14} aria-hidden="true" />
            </TooltipButton>
          </div>

          {#if stack.followUp}
            <MergeFollowUpCard
              followUp={stack.followUp}
              baseRef={input.baseRef}
              {worktreeName}
              busy={stack.busy !== null}
              onRestackAndCleanUp={() => void stack.cleanUpMerged()}
              onRemoveWorktree={() => void stack.removeWorktree()}
              onLater={() => stack.putOffFollowUp()}
            />
          {/if}

          {#if stack.progress}
            <RestackProgressCard
              progress={stack.progress}
              conflict={stack.conflict}
              busy={stack.busy !== null}
              onResolve={resolveInThread}
              onContinue={() => void stack.continueRestack()}
              onAbort={() => void stack.abortRestack()}
              onPush={() => void stack.push()}
              onDismiss={() => stack.dismissProgress()}
            />
          {/if}

          {#if stack.overviewError}
            <div class="error-note">{stack.overviewError}</div>
          {/if}

          <div class="flex flex-col" role="list" aria-label="Git branches, top first">
            {#each topDown as pr, index (pr.number)}
              {@const inboxPr = prByNumber.get(pr.number)}
              <!-- The bottom PR's lead is the base row's "N commits ahead". -->
              {@const drift =
                inboxPr && inboxPr.baseRef !== input.baseRef
                  ? driftLabel(inboxPr, stack.behind.get(pr.number), repoState?.inbox)
                  : null}
              <div role="listitem">
                <StackNode
                  {pr}
                  {index}
                  total={openPrs.length}
                  {inboxPr}
                  branch={branchByNumber.get(pr.number)}
                  {drift}
                  threads={threadsOn(pr.number)}
                  checkedOut={worktree?.branch === pr.headRef}
                  checkout={stack.checkouts?.get(pr.number) ?? null}
                  checkoutKnown={stack.checkouts !== null}
                  entries={branchMenu(input, pr.number, { worktreeName, canRestack: !stack.restacking })}
                  selected={selected === pr.number}
                  dragging={dragging === pr.number}
                  dropHint={dropTarget === pr.number ? dropHint : null}
                  showCommits={view === "commits"}
                  onSelect={select}
                  onBranchAction={(id, target) => void runBranch(id, target)}
                  {onDragStart}
                  {onDragOver}
                  {onDrop}
                  {onDragEnd}
                />
              </div>
            {/each}
            <BaseRow
              baseRef={input.baseRef}
              {overview}
              preview={stack.preview}
              dropHint={dropTarget === "base" ? dropHint : null}
              {onDragOver}
              {onDrop}
            />
          </div>

          <div class="flex flex-wrap gap-2">
            <button
              type="button"
              class="btn btn-sm text-xs hover:preset-tonal"
              disabled={blocked}
              onclick={() => void addBranch(null)}
            >
              Add branch on top
            </button>
            {#if selected !== null && order.includes(selected)}
              <button
                type="button"
                class="btn btn-sm text-xs hover:preset-tonal"
                disabled={blocked}
                onclick={() => selected !== null && void addBranch(selected)}
              >
                Insert below #{selected}
              </button>
            {/if}
          </div>

          <h2 class="eyebrow mt-2">Worktree</h2>
          <WorktreeCard
            {worktree}
            name={worktreeName}
            loaded={overview !== null}
            {missingBranches}
            busy={stack.busy !== null}
            restacking={stack.restacking}
            onCheckOut={() => void stack.checkOut()}
          />
        </section>
      {/if}

      <section
        aria-label="Pull request {selected ?? ""}"
        class="flex h-full min-h-[480px] min-w-0 flex-[999_1_560px] flex-col"
      >
        {#if selected !== null}
          {#key selected}
            <ReviewView {repoDir} {repoName} initialPr={selected} onOpenPr={openPr} {onAskCodex} />
          {/key}
        {:else}
          <div class="p-6"><div class="empty-state">Pick a git branch to see its PR.</div></div>
        {/if}
      </section>
    </div>
  {/if}
</div>
