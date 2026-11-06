<!--
  The Stack page (the canvas's "Stack" board): the git branch tree from the
  top PR down to the base, the stack's worktree, and the selected PR's review
  page beside it. Restack onto the base, push the stack, and every stack edit
  (add, insert, reorder by drag or ⌥↑/⌥↓, the git branch menu) run here over
  the backend's stack operations; after a PR merges it offers the follow-up.

  The stack is GitHub's, read from the inbox store; what is on disk comes
  from `reviewStackOverview`.
-->
<script lang="ts">
  import { Ellipsis, Folder, Sparkles } from "@lucide/svelte";
  import { onMount } from "svelte";
  import { projects } from "$lib/app/appData.svelte";
  import { openDialog } from "$lib/app/dialogs.svelte";
  import { openThreadInCwd } from "$lib/app/navigation.svelte";
  import ActionMenu from "$lib/components/ActionMenu.svelte";
  import ConfirmDialog from "$lib/components/ConfirmDialog.svelte";
  import RefreshIcon from "$lib/components/RefreshIcon.svelte";
  import Segmented from "$lib/components/Segmented.svelte";
  import { RING_CLASS, TONE_CLASS } from "$lib/inbox/badges";
  import { ciBadge, reviewBadge, stateTone } from "$lib/inbox/inbox";
  import { inboxes, syncInbox } from "$lib/inbox/inboxStore.svelte";
  import { startStackReview } from "$lib/review/agentReviews.svelte";
  import CheckoutStepDialog from "$lib/review/CheckoutStepDialog.svelte";
  import { stepUpFor } from "$lib/review/checkoutSteps";
  import { ensurePrLinks, prLinks, rememberPrLink } from "$lib/review/prLinks.svelte";
  import ReviewView from "$lib/review/ReviewView.svelte";
  import { startThreadWithTurn } from "$lib/services/api";
  import { commands } from "$lib/services/homeRouting";
  import { peekQuery, prefetch, readFresh } from "$lib/services/queryCache.svelte";
  import BranchNameDialog from "$lib/stack/BranchNameDialog.svelte";
  import { followUps, putOffFollowUp } from "$lib/stack/followUps.svelte";
  import RestackDialog from "$lib/stack/RestackDialog.svelte";
  import SplitDialog from "$lib/stack/SplitDialog.svelte";
  import {
    branchMenu,
    conflictPrompt,
    dropped,
    followUpKey,
    mergeFollowUp,
    moved,
    openOrder,
    stackInputFor,
  } from "$lib/stack/stack";
  import { toastError, toaster } from "$lib/toaster";
  import type {
    CheckoutTarget,
    RestackPlan,
    RestackProgress,
    StackInput,
    StackInputPr,
    StackOverview,
  } from "$lib/types";
  import { errorMessage } from "$lib/utils/errors";
  import { basename } from "$lib/utils/path";
  import { threadCountForPath } from "$lib/worktrees/worktrees";

  let {
    repoDir,
    repoName,
    stackNumber,
    onBack,
    onAskCodex,
    restackOnOpen = false,
  }: {
    repoDir: string;
    repoName: string;
    stackNumber: number;
    /** Back to the Pull requests inbox. */
    onBack: () => void;
    onAskCodex: (cwd: string, prompt: string) => void;
    /** Start a restack once the stack has loaded (the inbox's Restack). */
    restackOnOpen?: boolean;
  } = $props();

  let overview = $state<StackOverview | null>(null);
  let overviewError = $state<string | null>(null);
  let progress = $state<RestackProgress | null>(null);
  /** A restack that has not finished blocks other stack edits; a finished one only shows its result. */
  const restacking = $derived(progress !== null && !progress.done);
  let preview = $state<RestackPlan | null>(null);
  let selected = $state<number | null>(null);
  let busy = $state<string | null>(null);
  let view = $state<"stack" | "commits">("stack");
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

  function threadsOn(number: number): number {
    return Object.values(prLinks.byThread).filter(
      (link) => link.repoKey === repoDir && link.prNumber === number && link.closedAt === null,
    ).length;
  }

  function fail(cause: unknown) {
    toastError(errorMessage(cause));
  }

  function applyOverview(next: StackOverview) {
    overview = next;
    overviewError = null;
    // A finished restack stays up, offering Push stack, until dismissed.
    progress = next.restack ?? (progress?.done ? progress : null);
    selected ??= openPrs.find((pr) => pr.headRef === next.worktree?.branch)?.number ?? openPrs.at(-1)?.number ?? null;
  }

  /** Read the overview (and the restack preview when the base moved). Both
   *  are cached: a write through `commands` marks them stale, so after one
   *  this waits for a new read. */
  async function loadOverview() {
    if (!input) return;
    try {
      applyOverview(await readFresh("reviewStackOverview", repoDir, input));
      preview =
        overview && overview.baseAhead > 0
          ? await readFresh("reviewRestackPlan", repoDir, input, null).catch(() => null)
          : null;
      prefetchAround(selected);
    } catch (cause) {
      overviewError = errorMessage(cause);
    }
  }

  /** The cached overview, shown before the re-read lands. */
  function showCached() {
    if (!input) return;
    const cached = peekQuery("reviewStackOverview", repoDir, input);
    if (!cached) return;
    applyOverview(cached);
    preview = cached.baseAhead > 0 ? (peekQuery("reviewRestackPlan", repoDir, input, null) ?? null) : null;
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
    await syncInbox(repoDir);
    await loadOverview();
  }

  onMount(() => {
    void ensurePrLinks();
    showCached();
    const loading = inboxes[repoDir]?.inbox ? loadOverview() : refresh();
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
      fail(cause);
    }
  }

  /** Run one stack operation with the page busy, then refresh. */
  async function work(label: string, task: () => Promise<unknown>): Promise<boolean> {
    busy = label;
    try {
      await task();
      return true;
    } catch (cause) {
      fail(cause);
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
    return {
      number: pr.number,
      headRef: pr.headRef,
      headSha: pr.headSha,
      mine: isMine(pr),
      stack: input?.name ?? null,
    };
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
      fail(cause);
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
      fail(cause);
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
      fail(cause);
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
      fail(cause);
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
          reviewCheckoutPath: worktree.path,
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
      body: `Every PR in this stack has merged. Remove the worktree and archive its ${threads.length} thread${threads.length === 1 ? "" : "s"}?`,
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

{#snippet node(pr: StackInputPr, index: number)}
  {@const inboxPr = prByNumber.get(pr.number)}
  {@const branch = branchByNumber.get(pr.number)}
  {@const review = inboxPr ? reviewBadge(inboxPr) : null}
  {@const ci = inboxPr ? ciBadge(inboxPr) : null}
  {@const threads = threadsOn(pr.number)}
  {@const checkedOut = worktree?.branch === pr.headRef}
  <div class="flex gap-3">
    <div class="relative w-10 shrink-0" aria-hidden="true">
      <div class="absolute top-0 bottom-0 left-[27px] w-0.5 bg-surface-300-700" class:top-5={index === 0}></div>
      <div
        class="stack-ring {inboxPr ? RING_CLASS[stateTone(inboxPr)] : RING_CLASS.surface}"
        class:border-dashed={inboxPr?.isDraft}
        title="{openPrs.length - index} of {openPrs.length} in the stack"
      >
        {openPrs.length - index}
      </div>
    </div>
    <div class="mb-2.5 flex min-w-0 grow flex-col">
      {#if dropTarget === pr.number && dropHint}
        <div class="drop-line mb-1.5" aria-live="polite">{dropHint}</div>
      {/if}
      <ActionMenu
        entries={branchMenu(input as StackInput, pr.number, { worktreeName, canRestack: !restacking })}
        label="Branch {pr.headRef}"
        onSelect={(id) => void runBranch(id, pr)}
      >
        {#snippet row({ rowProps, buttonProps })}
          <div
            {...rowProps}
            role="button"
            aria-label="#{pr.number} {pr.headRef}"
            aria-pressed={selected === pr.number}
            class="stack-node"
            data-selected={selected === pr.number}
            data-dragging={dragging === pr.number}
            draggable="true"
            ondragstart={(event) => onDragStart(event, pr.number)}
            ondragover={(event) => onDragOver(event, pr.number)}
            ondrop={(event) => onDrop(event, pr.number)}
            ondragend={onDragEnd}
            onclick={() => {
              selected = pr.number;
              prefetchAround(pr.number);
            }}
          >
            <div class="flex items-center gap-2">
              <span class="min-w-0 truncate font-mono text-[11px]" class:font-semibold={checkedOut} title={pr.headRef}
                >{pr.headRef}</span
              >
              {#if checkedOut}
                <span class="status-badge preset-tonal-primary">Checked out</span>
              {/if}
              <span class="grow"></span>
              <button {...buttonProps} type="button" class="btn-icon btn-icon-sm hover:preset-tonal shrink-0">
                <Ellipsis size={14} aria-hidden="true" />
              </button>
            </div>
            <div class="flex min-w-0 items-center gap-2">
              <span class="font-mono text-[11px] text-surface-500">#{pr.number}</span>
              <span class="truncate" class:font-medium={selected === pr.number}>{pr.title}</span>
            </div>
            <div class="flex flex-wrap items-center gap-2 text-surface-500">
              {#if review}
                <span class="status-badge {TONE_CLASS[review.tone]}">{review.label}</span>
              {/if}
              {#if branch}
                <span>{branch.commits} commit{branch.commits === 1 ? "" : "s"}</span>
              {/if}
              {#if inboxPr && inboxPr.unresolvedThreads > 0}
                <span>·</span><span class="text-surface-950-50">{inboxPr.unresolvedThreads} unresolved</span>
              {:else if ci}
                <span>·</span><span>{ci.label}</span>
              {/if}
              {#if threads > 0}
                <span>·</span><span>{threads} thread{threads === 1 ? "" : "s"}</span>
              {/if}
              {#if branch?.unpushed}
                <span>·</span><span class="text-warning-600-400">Not pushed</span>
              {/if}
            </div>
            {#if view === "commits" && branch}
              <div class="flex flex-col gap-1 border-t border-surface-200-800 pt-1.5">
                {#each branch.commitLines as commit (commit.sha)}
                  <div class="flex gap-2">
                    <span class="font-mono text-[11px] text-surface-500">{commit.sha.slice(0, 7)}</span>
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
{/snippet}

<div class="flex h-full min-h-0 flex-col text-xs">
  <header class="flex flex-wrap items-center gap-2 border-b border-surface-200-800 px-5 py-3">
    <button type="button" class="text-surface-500 hover:underline" onclick={onBack}>Pull requests</button>
    <span class="text-surface-500">/</span>
    <span class="text-surface-500">{repoName}</span>
    <span class="text-surface-500">/</span>
    <h1 class="text-sm font-semibold">{input?.name ?? `Stack ${stackNumber}`}</h1>
    {#if input}
      <span class="status-badge preset-tonal"
        >{openPrs.length}
        PR{openPrs.length === 1 ? "" : "s"}{overview ? ` · ${totalCommits} commits` : ""}</span
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
    <div class="@container/stack flex min-h-0 grow flex-wrap items-stretch overflow-y-auto">
      <section
        aria-label="Stack tree"
        class="flex min-w-0 flex-[1_1_400px] flex-col gap-3 border-r border-surface-200-800 bg-surface-100-900 p-5 @min-[960px]/stack:h-full @min-[960px]/stack:overflow-y-auto"
      >
        {#if followUp?.kind === "restack"}
          <div class="card flex flex-col gap-2 preset-tonal-success px-3 py-2.5" role="status">
            <div class="font-medium">
              {followUp.merged.map((number) => `#${number}`).join(", ")}
              merged into {input.baseRef}
            </div>
            <div>
              Restack {followUp.rest.map((number) => `#${number}`).join(" and ")} onto {input.baseRef} and delete
              {followUp.branches.join(", ")}?
            </div>
            <div class="flex gap-1.5">
              <button
                type="button"
                class="btn btn-sm text-[11px] preset-filled-primary-500"
                disabled={busy !== null}
                onclick={() => void restackAndCleanUp()}
              >
                Restack and clean up
              </button>
              <button type="button" class="btn btn-sm text-[11px] hover:preset-tonal" onclick={later}>Later</button>
            </div>
          </div>
        {:else if followUp?.kind === "cleanup"}
          <div class="card flex flex-col gap-2 preset-tonal-success px-3 py-2.5" role="status">
            <div class="font-medium">Every PR in this stack merged</div>
            <div>Remove {worktreeName} and archive its threads?</div>
            <div class="flex gap-1.5">
              <button
                type="button"
                class="btn btn-sm text-[11px] preset-filled-primary-500"
                disabled={busy !== null}
                onclick={() => void removeWorktree()}
              >
                Remove worktree
              </button>
              <button type="button" class="btn btn-sm text-[11px] hover:preset-tonal" onclick={later}>Later</button>
            </div>
          </div>
        {/if}

        {#if progress}
          <div class="panel-card flex flex-col gap-2 px-3 py-2.5" aria-label="Restack progress">
            <div class="flex items-center gap-2">
              <span class="grow font-medium">{progress.done ? "Restacked" : "Restacking"} onto {progress.ontoRef}</span>
              <span class="text-surface-500">{basename(progress.worktree)}</span>
            </div>
            {#each progress.branches as entry (entry.number)}
              <div class="flex items-center gap-2" class:text-surface-500={entry.status === "waiting"}>
                {#if entry.status === "conflict"}
                  <span class="size-3 rounded-full bg-warning-500" aria-hidden="true"></span>
                {:else if entry.status === "waiting"}
                  <span
                    class="size-3 rounded-full border-2 border-dashed border-surface-400-600"
                    aria-hidden="true"
                  ></span>
                {:else}
                  <span class="size-3 rounded-full bg-success-500" aria-hidden="true"></span>
                {/if}
                <span class="grow font-mono text-[11px]">{entry.headRef}</span>
                {#if entry.status === "conflict"}
                  <span class="status-badge preset-tonal-warning"
                    >Conflict in {entry.conflicts.map(basename).join(", ")}</span
                  >
                {:else if entry.status === "waiting"}
                  <span>Waiting</span>
                {:else if entry.status === "unchanged"}
                  <span class="text-surface-500">Already in place</span>
                {:else}
                  <span class="text-surface-500">{entry.commits} commit{entry.commits === 1 ? "" : "s"} replayed</span>
                {/if}
              </div>
            {/each}
            {#if conflict}
              <div class="flex flex-wrap gap-1.5 border-t border-surface-200-800 pt-1.5">
                <button
                  type="button"
                  class="btn btn-sm text-[11px] preset-tonal"
                  onclick={() => void resolveInThread()}
                >
                  Resolve in a thread
                </button>
                <button
                  type="button"
                  class="btn btn-sm text-[11px] hover:preset-tonal"
                  onclick={() => progress && void commands.openInZed(progress.worktree).catch(fail)}
                >
                  Open in editor
                </button>
                <button
                  type="button"
                  class="btn btn-sm text-[11px] hover:preset-tonal"
                  disabled={busy !== null}
                  onclick={() => void continueRestack()}
                >
                  Continue
                </button>
                <span class="grow"></span>
                <button
                  type="button"
                  class="btn btn-sm text-[11px] text-error-600-400 hover:preset-tonal"
                  disabled={busy !== null}
                  onclick={() => void abortRestack()}
                >
                  Abort restack
                </button>
              </div>
            {:else if progress.done}
              <div class="flex gap-1.5 border-t border-surface-200-800 pt-1.5">
                <button type="button" class="btn btn-sm text-[11px] preset-tonal" onclick={() => void pushStack()}>
                  Push stack
                </button>
                <button
                  type="button"
                  class="btn btn-sm text-[11px] hover:preset-tonal"
                  onclick={() => (progress = null)}
                >
                  Dismiss
                </button>
              </div>
            {/if}
          </div>
        {/if}

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
        </div>

        {#if overviewError}
          <div class="error-note">{overviewError}</div>
        {/if}

        <div class="flex flex-col" role="list" aria-label="Git branches, top first">
          {#each topDown as pr, index (pr.number)}
            <div role="listitem">{@render node(pr, index)}</div>
          {/each}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class="flex gap-3"
            ondragover={(event) => onDragOver(event, "base")}
            ondrop={(event) => onDrop(event, "base")}
          >
            <div class="relative w-10 shrink-0" aria-hidden="true">
              <div class="absolute top-0 left-[27px] h-3.5 w-0.5 bg-surface-300-700"></div>
              <div class="absolute top-3 left-[21px] size-3.5 rounded-full bg-surface-500"></div>
            </div>
            <div class="flex min-w-0 grow flex-col gap-2 py-2.5" aria-label="Base {input.baseRef}">
              {#if dropTarget === "base" && dropHint}
                <div class="drop-line" aria-live="polite">{dropHint}</div>
              {/if}
              <div class="flex items-center gap-2">
                <span class="font-mono text-[11px] font-semibold">{input.baseRef}</span>
                {#if overview && overview.baseAhead > 0}
                  <span class="status-badge preset-tonal-warning"
                    >{overview.baseAhead}
                    commit{overview.baseAhead === 1 ? "" : "s"}
                    ahead of stack base</span
                  >
                {:else if overview}
                  <span class="text-surface-500">The stack is on its tip</span>
                {/if}
              </div>
              {#if overview && overview.baseCommits.length > 0}
                <div class="panel-card flex flex-col gap-1.5 bg-surface-50-950 px-3 py-2.5">
                  {#each overview.baseCommits as commit (commit.sha)}
                    <div class="flex gap-2">
                      <span class="font-mono text-[11px] text-surface-500">{commit.sha.slice(0, 7)}</span>
                      <span class="truncate">{commit.subject}</span>
                    </div>
                  {/each}
                  {#if preview}
                    {@const expected = preview.steps.flatMap((step) => step.conflicts)}
                    <div class="border-t border-surface-200-800 pt-1 text-surface-500">
                      Restack rebases all {preview.steps.filter((step) => !step.unchanged).length} git branches in the
                      worktree.
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
        {#if worktree}
          <div
            class="panel-card flex flex-col gap-1.5 bg-surface-50-950 px-3 py-2.5"
            aria-label="Worktree {worktreeName}"
          >
            <div class="flex items-center gap-2">
              <Folder size={14} class="text-surface-500" aria-hidden="true" />
              <span class="font-semibold">{worktreeName}</span>
              <span class="grow"></span>
              <span class="status-badge {worktree.clean ? "preset-tonal-success" : "preset-tonal-warning"}"
                >{worktree.clean ? "Clean" : "Uncommitted changes"}</span
              >
            </div>
            <div class="truncate font-mono text-[11px] text-surface-500">{worktree.path}</div>
            <div class="text-surface-500">
              {threadCountForPath(projects(), worktree.path)}
              thread{threadCountForPath(projects(), worktree.path) === 1 ? "" : "s"}
              here. Switching PR checks out its git branch; threads stay with the PR they were started on.
            </div>
          </div>
        {:else if overview}
          <div class="empty-state">
            No worktree has this stack's git branches yet. Restacking or pushing creates one.
          </div>
        {/if}
      </section>

      <section
        aria-label="Pull request {selected ?? ""}"
        class="flex h-full min-h-[480px] min-w-0 flex-[999_1_560px] flex-col"
      >
        {#if selected !== null}
          {#key selected}
            <ReviewView {repoDir} {repoName} initialPr={selected} {onAskCodex} />
          {/key}
        {:else}
          <div class="p-6"><div class="empty-state">Pick a git branch to see its PR.</div></div>
        {/if}
      </section>
    </div>
  {/if}
</div>
