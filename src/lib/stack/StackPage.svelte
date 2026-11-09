<!--
  The Stack page (the canvas's "Stack" board): the git branch tree from the
  top PR down to the base, the stack's worktree, and the selected PR's review
  page beside it. A PR in a stack opens here too, however it was reached. The
  tree folds to a strip of its PRs. Restack onto the base, push the stack, and
  every stack edit (add, insert, reorder by drag or ⌥↑/⌥↓, the git branch menu)
  run here over the backend's stack operations; after a PR merges it offers the
  follow-up.

  The stack is GitHub's, read from the inbox store; what is on disk comes
  from `reviewStackOverview`.
-->
<script lang="ts">
  import { ChevronLeft, PanelLeftClose, PanelLeftOpen, Sparkles } from "@lucide/svelte";
  import { onMount, tick, untrack } from "svelte";
  import { projectForCwd, projects } from "$lib/app/appData.svelte";
  import { openDialog } from "$lib/app/dialogs.svelte";
  import { openReview, openThreadInCwd, type PrMove } from "$lib/app/navigation.svelte";
  import { place } from "$lib/app/places";
  import ConfirmDialog from "$lib/components/ConfirmDialog.svelte";
  import RefreshIcon from "$lib/components/RefreshIcon.svelte";
  import Segmented from "$lib/components/Segmented.svelte";
  import TooltipButton from "$lib/components/TooltipButton.svelte";
  import { inboxes, resyncInbox, syncInbox } from "$lib/inbox/inboxStore.svelte";
  import { startStackReview } from "$lib/review/agentReviews.svelte";
  import CheckoutStepDialog from "$lib/review/CheckoutStepDialog.svelte";
  import { checkOutStack, stepUpFor } from "$lib/review/checkoutSteps";
  import { checkoutTarget } from "$lib/review/checkoutTarget";
  import { ensurePrLinks, prLinks, rememberPrLink } from "$lib/review/prLinks.svelte";
  import ReviewView from "$lib/review/ReviewView.svelte";
  import { loadStackTreeCollapsed, saveStackTreeCollapsed } from "$lib/review/reviewPrefs";
  import { startThreadWithTurn } from "$lib/services/api";
  import { commands } from "$lib/services/homeRouting";
  import { prefetch, query, readFresh } from "$lib/services/queryCache.svelte";
  import BaseRow from "$lib/stack/BaseRow.svelte";
  import BranchNameDialog from "$lib/stack/BranchNameDialog.svelte";
  import { followUps, putOffFollowUp } from "$lib/stack/followUps.svelte";
  import MergeFollowUpCard from "$lib/stack/MergeFollowUpCard.svelte";
  import RestackDialog from "$lib/stack/RestackDialog.svelte";
  import RestackProgressCard from "$lib/stack/RestackProgressCard.svelte";
  import SplitDialog from "$lib/stack/SplitDialog.svelte";
  import StackNode from "$lib/stack/StackNode.svelte";
  import {
    baseInput,
    branchMenu,
    conflictPrompt,
    driftLabel,
    dropped,
    followUpKey,
    mergeFollowUp,
    moved,
    openOrder,
    stackInputFor,
  } from "$lib/stack/stack";
  import WorktreeCard from "$lib/stack/WorktreeCard.svelte";
  import { toastError, toaster, toastFailure } from "$lib/toaster";
  import type { CheckoutTarget, RestackProgress, StackInput, StackInputPr, StackOverview } from "$lib/types";
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

  let progress = $state<RestackProgress | null>(null);
  /** A restack that has not finished blocks other stack edits; a finished one only shows its result. */
  const restacking = $derived(progress !== null && !progress.done);
  // svelte-ignore state_referenced_locally -- the page remounts per navigation.
  const left = place(`stack:${repoDir}#${stackNumber}`, () => ({
    selected: null as number | null,
    view: "stack" as "stack" | "commits",
    pageScrollTop: 0,
    treeScrollTop: 0,
  }));
  // svelte-ignore state_referenced_locally -- likewise.
  let selected = $state<number | null>(initialPr);
  let busy = $state<string | null>(null);
  let view = $state(left.view);
  let treeCollapsed = $state(loadStackTreeCollapsed());
  let page = $state<HTMLElement>();
  let tree = $state<HTMLElement>();
  let dragging = $state<number | null>(null);
  let dropTarget = $state<number | "base" | null>(null);

  const repoState = $derived(inboxes[repoDir]);
  const inbox = $derived(repoState?.inbox ?? null);
  const stack = $derived(repoState?.stacks?.stacks.find((candidate) => candidate.number === stackNumber) ?? null);
  const input = $derived<StackInput | null>(stack && inbox ? stackInputFor(stack, inbox) : null);
  const order = $derived(input ? openOrder(input) : []);
  const openPrs = $derived(input ? input.prs.filter((pr) => pr.open && !pr.merged) : []);
  /** Top first, the way the tree draws them. */
  const topDown = $derived([...openPrs].reverse());
  const prByNumber = $derived(new Map((inbox?.prs ?? []).map((pr) => [pr.number, pr])));
  /** Each open PR as GitHub has it, for how far behind its base it is. */
  const behindInputs = $derived(
    openPrs.flatMap((pr) => {
      const found = prByNumber.get(pr.number);
      return found ? [baseInput(found)] : [];
    }),
  );
  // All three re-read when `input` changes or a write marks them stale, so a
  // merge in the nested pane shows here. The stack's last overview stays up
  // while one for new input is read.
  const overviewQuery = query("reviewStackOverview", () => (input ? [repoDir, input] : null));
  let lastOverview: StackOverview | null = null;
  const overview = $derived((lastOverview = overviewQuery.value ?? lastOverview));
  const overviewError = $derived(overviewQuery.error);
  const behindQuery = query("reviewPrsBehind", () => (input ? [repoDir, behindInputs] : null));
  /** Commits each open PR's base has that GitHub's head lacks. */
  const behind = $derived(new Map((behindQuery.value ?? []).map((row) => [row.number, row.behind])));
  /** The restack preview, when the base moved or a PR left the head below it. */
  const planQuery = query("reviewRestackPlan", () =>
    input && overview && needsRestack(overview, behind) ? [repoDir, input, null] : null,
  );
  const preview = $derived(planQuery.value ?? null);
  const branchByNumber = $derived(new Map((overview?.branches ?? []).map((branch) => [branch.number, branch])));
  const totalCommits = $derived((overview?.branches ?? []).reduce((sum, branch) => sum + branch.commits, 0));
  const worktree = $derived(overview?.worktree ?? null);
  const worktreeName = $derived(worktree ? basename(worktree.path) : null);
  const followUp = $derived(
    input && overview
      ? mergeFollowUp(input, overview.mergedBranches, worktree !== null, new Set(followUps.later))
      : null,
  );
  const conflict = $derived(progress?.branches.find((branch) => branch.status === "conflict") ?? null);

  // Unless opened for a PR, back on the one shown last, while it is still open.
  // svelte-ignore state_referenced_locally -- once, as the page opens.
  if (selected === null && openPrs.some((pr) => pr.number === left.selected)) selected = left.selected;

  $effect(() => {
    left.selected = selected;
    left.view = view;
  });

  // A restack started from the header has its progress and conflicts in the tree.
  $effect(() => {
    if (restacking) treeCollapsed = false;
  });

  function setTreeCollapsed(value: boolean) {
    treeCollapsed = value;
    saveStackTreeCollapsed(value);
  }

  function select(number: number) {
    selected = number;
    prefetchAround(number);
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

  $effect(() => {
    const next = overviewQuery.value;
    if (!next) return;
    untrack(() => {
      // A finished restack stays up, offering Push stack, until dismissed.
      progress = next.restack ?? (progress?.done ? progress : null);
      selected ??= openPrs.find((pr) => pr.headRef === next.worktree?.branch)?.number ?? openPrs.at(-1)?.number ?? null;
    });
  });

  // Merged PRs with nothing left to offer (restacked, or cleaned up elsewhere)
  // are settled, or the sidebar's dot stays with no card to clear it from.
  // Only from a settled read of these PRs: an older one hasn't seen the merge.
  $effect(() => {
    const settled = overviewQuery.value && !overviewQuery.refreshing;
    if (settled && input?.prs.some((pr) => pr.merged) && !followUp && !restacking) later();
  });

  /** Wait for what a write changed: it marked these stale, so this waits for
   *  new reads, not the cached answers. */
  async function loadOverview() {
    // `input` may have just changed, and the queries follow it on the next tick.
    await tick();
    if (!input) return;
    await Promise.all([
      readFresh("reviewStackOverview", repoDir, input).catch(() => null),
      readFresh("reviewPrsBehind", repoDir, behindInputs).catch(() => null),
    ]);
    await tick();
    prefetchAround(selected);
  }

  /** The base moved, or a PR no longer sits on the head of the one below. */
  function needsRestack(next: StackOverview, counts: Map<number, number | null>): boolean {
    return next.baseAhead > 0 || [...counts.values()].some((count) => (count ?? 0) > 0);
  }

  /** Warm the PRs either side of the selected one, so a branch click shows
   *  its review at once. */
  function prefetchAround(number: number | null) {
    const at = openPrs.findIndex((pr) => pr.number === number);
    if (at === -1) return;
    for (const pr of [openPrs[at - 1], openPrs[at + 1]]) if (pr) prefetch("reviewPrDetail", repoDir, pr.number);
  }

  /** Re-read GitHub's stack and the inbox, then what is on disk. */
  async function refresh() {
    await resyncInbox(repoDir);
    await loadOverview();
  }

  onMount(() => {
    void ensurePrLinks();
    void tick().then(() => {
      if (page) page.scrollTop = left.pageScrollTop;
      if (tree) tree.scrollTop = left.treeScrollTop;
    });
    const loading = inboxes[repoDir]?.inbox ? loadOverview() : syncInbox(repoDir).then(loadOverview);
    if (restackOnOpen) void loading.then(() => (progress ? undefined : restack()));
  });

  /** One agent review over the stack's open PRs, bottom first. */
  async function reviewWholeStack() {
    const prs = openPrs.flatMap((pr) => prByNumber.get(pr.number) ?? []);
    if (prs.length === 0) return;
    try {
      await startStackReview(repoDir, prs, { viewer: inbox?.viewer ?? null });
      toaster.success({
        title: "Stack review queued",
        description: `One agent review over ${prs.map((pr) => `#${pr.number}`).join(", ")}, filed under #${prs.at(-1)?.number}.`,
      });
    } catch (cause) {
      toastFailure(cause);
    }
  }

  /** Run one stack operation with the page busy, then refresh. */
  async function work(label: string, task: () => Promise<unknown>): Promise<boolean> {
    busy = label;
    try {
      await task();
      return true;
    } catch (cause) {
      toastFailure(cause);
      return false;
    } finally {
      busy = null;
      await refresh();
    }
  }

  /** The viewer wrote this PR: pushing to its git branch needs no asking. */
  function isMine(pr: StackInputPr): boolean {
    return !!inbox && prByNumber.get(pr.number)?.author === inbox.viewer;
  }

  /** Someone else's stack: the page leads with reviewing it, not with editing it. */
  const theirs = $derived(openPrs.length > 0 && !openPrs.some(isMine));

  function targetFor(pr: StackInputPr): CheckoutTarget {
    return checkoutTarget(
      pr,
      isMine(pr),
      input ? { name: input.name, branches: openPrs.map((open) => open.headRef) } : null,
    );
  }

  // Where each open PR is on disk, for the tree's worktree icons.
  const checkoutsQuery = query("reviewPrCheckouts", () =>
    openPrs.length > 0 ? [repoDir, openPrs.map(targetFor)] : null,
  );
  const checkouts = $derived(new Map((checkoutsQuery.value ?? []).map((checkout) => [checkout.number, checkout])));
  /** Open PRs with no local git branch yet. */
  const missingBranches = $derived((overview?.branches ?? []).filter((branch) => !branch.local).length);

  /** A worktree for the whole stack and a local git branch for each PR, or just
   *  what is missing. */
  async function checkOutWholeStack() {
    const top = openPrs.at(-1);
    if (!input || !top) return;
    const stackInput = input;
    await work("checkout", () => checkOutStack({ repoDir, repoName, input: stackInput, target: targetFor(top) }));
  }

  /** The stack's worktree, creating one on `pr`'s git branch if it has none. */
  async function ensureWorktree(pr: StackInputPr | undefined): Promise<string | null> {
    if (worktree) return worktree.path;
    if (!pr) return null;
    const checkout = await stepUpFor({ repoDir, repoName, target: targetFor(pr), action: "restack" });
    if (!checkout?.path) return null;
    await loadOverview();
    return checkout.path;
  }

  /** Threads running now in the worktree at `path`. */
  async function runningIn(path: string): Promise<number> {
    const active = new Set(await commands.threadsWithActiveTurns().catch(() => [] as string[]));
    const prefix = path.replace(/\/+$/, "");
    return projects()
      .flatMap((project) => project.threads)
      .filter((thread) => active.has(thread.id) && (thread.cwd === prefix || thread.cwd.startsWith(`${prefix}/`)))
      .length;
  }

  /** Restack, confirming the plan first. True once the stack sits on the
   *  base: restacked, or already there. */
  async function restack(from: number | null = null): Promise<boolean> {
    if (!input) return false;
    const stackInput = input;
    try {
      let plan = await commands.reviewRestackPlan(repoDir, stackInput, from);
      if (!plan.worktree) {
        if (!(await ensureWorktree(openPrs.at(-1)))) return false;
        plan = await commands.reviewRestackPlan(repoDir, stackInput, from);
      }
      if (plan.steps.every((step) => step.unchanged)) {
        toaster.info({ title: `Already on ${stackInput.baseRef}`, description: "Nothing to restack." });
        return true;
      }
      const ok = await openDialog(RestackDialog, {
        plan,
        title: from === null ? `Restack onto ${stackInput.baseRef}` : `Restack from #${from}`,
        confirmLabel: "Restack",
        running: plan.worktree ? await runningIn(plan.worktree) : 0,
      });
      if (!ok) return false;
      busy = "restack";
      progress = await commands.reviewRestack(repoDir, stackInput, from);
      if (progress.done) {
        toaster.success({
          title: `Restacked onto ${stackInput.baseRef}`,
          description: "Push the stack to update GitHub.",
        });
      }
      return progress.done;
    } catch (cause) {
      toastFailure(cause);
      return false;
    } finally {
      busy = null;
      await loadOverview();
    }
  }

  async function continueRestack() {
    busy = "restack";
    try {
      progress = await commands.reviewContinueRestack(repoDir, stackNumber);
      const left = progress.branches.find((branch) => branch.status === "conflict");
      if (left) toastError(`Conflict markers are still in ${left.conflicts.join(", ")}`);
      else if (progress.done)
        toaster.success({ title: "Restack finished", description: "Push the stack to update GitHub." });
    } catch (cause) {
      toastFailure(cause);
    } finally {
      busy = null;
      await loadOverview();
    }
  }

  async function abortRestack() {
    busy = "restack";
    try {
      await commands.reviewAbortRestack(repoDir, stackNumber);
      progress = null;
      toaster.info({ title: "Restack aborted", description: "Every git branch is back where it was." });
    } catch (cause) {
      toastFailure(cause);
    } finally {
      busy = null;
      await loadOverview();
    }
  }

  async function resolveInThread() {
    if (!conflict || !progress) return;
    const pr = openPrs.find((candidate) => candidate.number === conflict.number);
    if (!pr) return;
    try {
      const { thread } = await startThreadWithTurn(
        progress.worktree,
        [{ type: "text", text: conflictPrompt(conflict.headRef, progress.ontoRef, conflict.conflicts) }],
        {
          beforeTurn: async (started) => {
            rememberPrLink(await commands.reviewLinkThread(repoDir, started.id, targetFor(pr)));
          },
        },
      );
      openThreadInCwd(thread.id, progress.worktree);
    } catch (cause) {
      toastFailure(cause);
    }
  }

  async function pushStack(only: number | null = null) {
    if (!input) return;
    const stackInput = input;
    const pushing = openPrs.filter((candidate) => only === null || candidate.number === only);
    const pr = pushing.at(-1);
    if (!pr) return;
    if (!worktree) {
      // Steps up to a worktree, asking first about someone else's git branch.
      if (!(await stepUpFor({ repoDir, repoName, target: targetFor(pr), action: "push" }))) return;
    } else {
      // The stack's worktree holds every one of its git branches, whichever
      // is checked out. Pushing to someone else's is still asked every time.
      const other = pushing.find((candidate) => !isMine(candidate));
      if (
        other &&
        !(await openDialog(CheckoutStepDialog, {
          confirm: "pushToTheirBranch" as const,
          action: "push" as const,
          target: targetFor(other),
          repoName,
          worktreePath: worktree.path,
        }))
      )
        return;
    }
    await work("push", async () => {
      const report = await commands.reviewPushStack(repoDir, stackInput, only);
      toaster.success({
        title:
          report.pushed.length === 0
            ? "GitHub is up to date"
            : `Pushed ${report.pushed.length} git branch${report.pushed.length === 1 ? "" : "es"}`,
        description: report.pushed.join(", ") || undefined,
      });
      // The finished restack's card offered this push; it has done its job.
      if (only === null && progress?.done) progress = null;
    });
  }

  /** Confirm and apply a new order (bottom first). */
  async function reorder(next: number[] | null) {
    if (!next || !input) return;
    const stackInput = input;
    const lines = next
      .map((number, index) => {
        const pr = openPrs.find((candidate) => candidate.number === number);
        const below =
          index === 0 ? stackInput.baseRef : openPrs.find((candidate) => candidate.number === next[index - 1])?.headRef;
        return `#${number} ${pr?.headRef ?? ""} onto ${below}`;
      })
      .reverse();
    const ok = await openDialog(ConfirmDialog, {
      title: "Reorder the stack",
      body: [
        "Each git branch is rebased onto the one now below it, the stack is force-pushed with leases, and the PRs are retargeted.",
        "This rewrites pushed commits.",
      ],
      detail: lines,
      confirmLabel: "Reorder",
      tone: "warning" as const,
    });
    if (!ok) return;
    await work("reorder", () => commands.reviewReorderStack(repoDir, stackInput, next));
  }

  async function addBranch(below: number | null) {
    if (!input) return;
    const stackInput = input;
    const name = await openDialog(BranchNameDialog, {
      title: below === null ? "Add a git branch on top" : `Insert a git branch below #${below}`,
      subtitle:
        below === null
          ? `It starts at ${openPrs.at(-1)?.headRef ?? stackInput.baseRef}. Commit there, then open its PR.`
          : `#${below} is retargeted onto it.`,
      confirmLabel: below === null ? "Add git branch" : "Insert git branch",
    });
    if (!name) return;
    await work("add", () => commands.reviewAddStackBranch(repoDir, stackInput, name, below));
  }

  async function runBranch(id: string, pr: StackInputPr) {
    if (!input) return;
    const stackInput = input;
    switch (id) {
      case "checkout": {
        const path = await ensureWorktree(pr);
        if (!path || worktree?.branch === pr.headRef) return;
        await work("checkout", () => commands.gitCheckoutBranch(path, pr.headRef, false));
        return;
      }
      case "restackFrom":
        return void restack(pr.number);
      case "push":
        return void pushStack(pr.number);
      case "moveUp":
        return void reorder(moved(order, pr.number, "up"));
      case "moveDown":
        return void reorder(moved(order, pr.number, "down"));
      case "insertBelow":
        return void addBranch(pr.number);
      case "split": {
        const choice = await openDialog(SplitDialog, {
          number: pr.number,
          headRef: pr.headRef,
          commits: branchByNumber.get(pr.number)?.commitLines ?? [],
        });
        if (!choice) return;
        await work("split", async () => {
          const created = await commands.reviewSplitBranch(repoDir, stackInput, pr.number, choice);
          toaster.success({ title: `Opened #${created} below #${pr.number}` });
        });
        return;
      }
      case "fold": {
        const index = order.indexOf(pr.number);
        const parent = openPrs[index - 1];
        if (!parent) return;
        const ok = await openDialog(ConfirmDialog, {
          title: `Fold #${pr.number} into #${parent.number}`,
          body: `Squash ${pr.headRef} into ${parent.headRef} as one commit, replay the git branches above onto it, push, and close #${pr.number}.`,
          detail: `#${pr.number} ${pr.title}`,
          confirmLabel: "Fold and close",
        });
        if (ok) await work("fold", () => commands.reviewFoldIntoParent(repoDir, stackInput, pr.number));
        return;
      }
      case "rename": {
        const to = await openDialog(BranchNameDialog, {
          title: `Rename ${pr.headRef}`,
          subtitle: `#${pr.number} and the PR above it follow the new name on GitHub.`,
          initial: pr.headRef,
          confirmLabel: "Rename",
        });
        if (to) await work("rename", () => commands.reviewRenameStackBranch(repoDir, stackInput, pr.number, to));
        return;
      }
      case "remove": {
        const ok = await openDialog(ConfirmDialog, {
          title: `Remove #${pr.number} from the stack`,
          body: `#${pr.number} is rebased onto ${stackInput.baseRef} and retargeted there, and stays open. The PR above it moves onto the one below.`,
          detail: `#${pr.number} ${pr.title}`,
          confirmLabel: "Remove from stack",
        });
        if (ok) await work("remove", () => commands.reviewRemoveFromStack(repoDir, stackInput, pr.number));
        return;
      }
    }
  }

  function later() {
    if (input) putOffFollowUp(followUpKey(input));
  }

  async function restackAndCleanUp() {
    if (!input) return;
    const stackInput = input;
    if (await restack()) {
      await work("cleanup", async () => {
        const deleted = await commands.reviewDeleteMergedBranches(repoDir, stackInput);
        if (deleted.length > 0) toaster.success({ title: `Deleted ${deleted.join(", ")}` });
      });
    }
  }

  async function removeWorktree() {
    if (!worktree) return;
    const path = worktree.path;
    const threads = projects()
      .flatMap((project) => project.threads)
      .filter((thread) => thread.cwd === path || thread.cwd.startsWith(`${path}/`));
    const ok = await openDialog(ConfirmDialog, {
      title: `Remove ${basename(path)}`,
      body: `Every PR in this stack has merged. Remove the worktree and archive its ${plural(threads.length, "thread")}?`,
      detail: path,
      confirmLabel: "Remove and archive",
    });
    if (!ok) return;
    await work("cleanup", async () => {
      for (const thread of threads) await commands.archiveThread(thread.id);
      await commands.gitWorktreeRemove(repoDir, path, false);
    });
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
    void reorder(dropped(order, dragged, target === "base" ? null : target));
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
        class="btn btn-sm text-xs {theirs ? "preset-filled-primary-500" : "hover:preset-tonal"}"
        disabled={openPrs.length === 0 || busy !== null}
        onclick={() => void reviewWholeStack()}
      >
        <Sparkles size={14} aria-hidden="true" />Review whole stack
      </button>
      <button
        type="button"
        class="btn btn-sm text-xs preset-tonal"
        disabled={!input || busy !== null || restacking}
        onclick={() => void pushStack()}
      >
        Push stack
      </button>
      <button
        type="button"
        class="btn btn-sm text-xs {theirs ? "hover:preset-tonal" : "preset-filled-primary-500"}"
        disabled={!input || busy !== null || restacking}
        onclick={() => void restack()}
      >
        <RefreshIcon spinning={busy === "restack"} size={14} />Restack onto
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
          {#if followUp}
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

          {#if followUp}
            <MergeFollowUpCard
              {followUp}
              baseRef={input.baseRef}
              {worktreeName}
              busy={busy !== null}
              onRestackAndCleanUp={restackAndCleanUp}
              onRemoveWorktree={removeWorktree}
              onLater={later}
            />
          {/if}

          {#if progress}
            <RestackProgressCard
              {progress}
              {conflict}
              busy={busy !== null}
              onResolve={resolveInThread}
              onContinue={continueRestack}
              onAbort={abortRestack}
              onPush={() => void pushStack()}
              onDismiss={() => (progress = null)}
            />
          {/if}

          {#if overviewError}
            <div class="error-note">{overviewError}</div>
          {/if}

          <div class="flex flex-col" role="list" aria-label="Git branches, top first">
            {#each topDown as pr, index (pr.number)}
              {@const inboxPr = prByNumber.get(pr.number)}
              <!-- The bottom PR's lead is the base row's "N commits ahead". -->
              {@const drift =
                inboxPr && inboxPr.baseRef !== input.baseRef ? driftLabel(inboxPr, behind.get(pr.number), inbox) : null}
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
                  checkout={checkouts.get(pr.number) ?? null}
                  checkoutKnown={checkoutsQuery.value !== undefined}
                  entries={branchMenu(input, pr.number, { worktreeName, canRestack: !restacking })}
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
              {preview}
              dropHint={dropTarget === "base" ? dropHint : null}
              {onDragOver}
              {onDrop}
            />
          </div>

          <div class="flex flex-wrap gap-2">
            <button
              type="button"
              class="btn btn-sm text-xs hover:preset-tonal"
              disabled={busy !== null || restacking}
              onclick={() => void addBranch(null)}
            >
              Add branch on top
            </button>
            {#if selected !== null && order.includes(selected)}
              <button
                type="button"
                class="btn btn-sm text-xs hover:preset-tonal"
                disabled={busy !== null || restacking}
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
            busy={busy !== null}
            {restacking}
            onCheckOut={checkOutWholeStack}
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
