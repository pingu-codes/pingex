// One Stack open on the Stack page, from reading it to restacking, pushing
// and editing it: the stack under work. It owns the order each operation's
// steps run in, so the page that shows it only lays it out and answers its
// questions.
//
// - The stack is GitHub's, from the inbox store; what is on disk (git
//   branches, the base's lead, the Stack worktree, a restack in progress) is
//   `reviewStackOverview`. The restack preview is read only when the base
//   moved or a PR left the head of the one below.
// - Nothing that changes the stack runs before `confirm` says yes. Each
//   operation asks at most once, with what the answer depends on (the plan,
//   the threads running in the worktree), and runs nothing when declined.
// - A restack stops at a conflict until continued or aborted. A finished one
//   stays up, offering Push stack, until that push or a dismissal.
// - While an operation runs, `busy` names it (the page disables the rest). It
//   says why it failed in a toast, then re-reads the inbox and what is on
//   disk, whether it failed or not. Restack, continue and abort re-read only
//   what is on disk.
// - Once a settled read shows a merged PR with nothing left to offer, the
//   follow-up is put off, which settles the sidebar's dot.
//
// Construct it during component setup: it reads through `query`, whose
// effects belong to the component.

import { untrack } from "svelte";
import { projects } from "$lib/app/appData.svelte";
import { inboxes, resyncInbox, syncInbox } from "$lib/inbox/inboxStore.svelte";
import { startStackReview } from "$lib/review/agentReviews.svelte";
import { checkOutStack, stepUpFor } from "$lib/review/checkoutSteps";
import { checkoutTarget } from "$lib/review/checkoutTarget";
import { rememberPrLink } from "$lib/review/prLinks.svelte";
import { startThreadWithTurn } from "$lib/services/api";
import { commands } from "$lib/services/homeRouting";
import { query, readFresh } from "$lib/services/queryCache.svelte";
import { followUps, putOffFollowUp } from "$lib/stack/followUps.svelte";
import {
  baseInput,
  conflictPrompt,
  followUpKey,
  type MergeFollowUp,
  mergeFollowUp,
  stackInputFor,
} from "$lib/stack/stack";
import { toastError, toaster, toastFailure } from "$lib/toaster";
import type {
  CheckoutTarget,
  Inbox,
  PrCheckout,
  RestackPlan,
  RestackProgress,
  SplitRequest,
  StackInput,
  StackInputPr,
  StackOverview,
} from "$lib/types";

/** What the stack is doing, while it does it. */
export type StackWork =
  | "restack"
  | "push"
  | "checkout"
  | "reorder"
  | "add"
  | "split"
  | "fold"
  | "rename"
  | "remove"
  | "cleanup";

/** What the stack asks before it changes anything; the page answers each. */
export type StackQuestion =
  /** Run this restack, which waits for `running` threads in its worktree. */
  | { kind: "restack"; plan: RestackPlan; from: number | null; running: number }
  /** Push to someone else's git branch from the stack's worktree. */
  | { kind: "pushToTheirBranch"; target: CheckoutTarget; worktreePath: string }
  /** Rebase into `order` (bottom first), force-push and retarget. */
  | { kind: "reorder"; order: number[] }
  /** Squash `pr` into `parent` and close it. */
  | { kind: "fold"; pr: StackInputPr; parent: StackInputPr }
  /** Take `pr` out of the stack, onto the base. */
  | { kind: "remove"; pr: StackInputPr }
  /** Every PR merged: remove the worktree and archive its `threads`. */
  | { kind: "removeWorktree"; path: string; threads: number };

/** A change to the stack's shape. The page gathers names and split points;
 *  the stack asks before a reorder, fold or remove. */
export type StackEdit =
  | { kind: "reorder"; order: number[] }
  /** A new git branch on top, or below `below`. */
  | { kind: "add"; name: string; below: number | null }
  | { kind: "split"; number: number; split: SplitRequest }
  | { kind: "fold"; number: number }
  | { kind: "rename"; number: number; to: string }
  | { kind: "remove"; number: number };

export class StackUnderWork {
  readonly repoDir: string;
  readonly repoName: string;
  readonly number: number;

  readonly #confirm: (question: StackQuestion) => Promise<boolean>;
  #busy = $state<StackWork | null>(null);
  #progress = $state<RestackProgress | null>(null);
  /** The last overview, kept up while one for new input is read. */
  #lastOverview: StackOverview | null = null;

  readonly #inbox = $derived.by<Inbox | null>(() => inboxes[this.repoDir]?.inbox ?? null);
  readonly #input = $derived.by<StackInput | null>(() => {
    const stack = inboxes[this.repoDir]?.stacks?.stacks.find((candidate) => candidate.number === this.number);
    return stack && this.#inbox ? stackInputFor(stack, this.#inbox) : null;
  });
  readonly #openPrs = $derived(this.#input ? this.#input.prs.filter((pr) => pr.open && !pr.merged) : []);
  readonly #prByNumber = $derived(new Map((this.#inbox?.prs ?? []).map((pr) => [pr.number, pr])));
  /** Each open PR as GitHub has it, for how far behind its base it is. */
  readonly #behindInputs = $derived(
    this.#openPrs.flatMap((pr) => {
      const found = this.#prByNumber.get(pr.number);
      return found ? [baseInput(found)] : [];
    }),
  );

  // Each re-reads when the input changes or a write marks it stale, so a
  // merge in the nested review pane shows here.
  readonly #overviewQuery = query("reviewStackOverview", () => (this.#input ? [this.repoDir, this.#input] : null));
  readonly #overview = $derived((this.#lastOverview = this.#overviewQuery.value ?? this.#lastOverview));
  readonly #behindQuery = query("reviewPrsBehind", () => (this.#input ? [this.repoDir, this.#behindInputs] : null));
  readonly #behind = $derived(new Map((this.#behindQuery.value ?? []).map((row) => [row.number, row.behind])));
  readonly #planQuery = query("reviewRestackPlan", () =>
    this.#input && this.#overview && needsRestack(this.#overview, this.#behind)
      ? [this.repoDir, this.#input, null]
      : null,
  );
  readonly #checkoutsQuery = query("reviewPrCheckouts", () =>
    this.#openPrs.length > 0 ? [this.repoDir, this.#openPrs.map((pr) => this.#targetFor(pr))] : null,
  );
  readonly #checkouts = $derived(
    this.#checkoutsQuery.value
      ? new Map(this.#checkoutsQuery.value.map((checkout) => [checkout.number, checkout]))
      : null,
  );
  readonly #followUp = $derived(
    this.#input && this.#overview
      ? mergeFollowUp(
          this.#input,
          this.#overview.mergedBranches,
          this.#overview.worktree !== null,
          new Set(followUps.later),
        )
      : null,
  );

  constructor(options: {
    repoDir: string;
    repoName: string;
    /** The stack's number in the inbox's stack listing. */
    number: number;
    /** Ask the person; nothing runs until this resolves true. */
    confirm: (question: StackQuestion) => Promise<boolean>;
  }) {
    this.repoDir = options.repoDir;
    this.repoName = options.repoName;
    this.number = options.number;
    this.#confirm = options.confirm;

    $effect(() => {
      const next = this.#overviewQuery.value;
      if (!next) return;
      // A finished restack stays up, offering Push stack, until dismissed.
      untrack(() => (this.#progress = next.restack ?? (this.#progress?.done ? this.#progress : null)));
    });
    // Merged PRs with nothing left to offer (restacked, or cleaned up
    // elsewhere) are settled, or the sidebar's dot stays with no card to
    // clear it from. Only from a settled read: an older one hasn't seen the merge.
    $effect(() => {
      const settled = this.#overviewQuery.value && !this.#overviewQuery.refreshing;
      if (settled && this.#input?.prs.some((pr) => pr.merged) && !this.#followUp && !this.restacking)
        untrack(() => this.putOffFollowUp());
    });
  }

  /** The stack as GitHub has it, with what the inbox knows of each PR;
   *  `null` until the inbox is read, or once GitHub no longer has it. */
  get input(): StackInput | null {
    return this.#input;
  }

  /** The open PRs, bottom first. */
  get openPrs(): StackInputPr[] {
    return this.#openPrs;
  }

  /** What is on disk; the last one stays while a new one is read. */
  get overview(): StackOverview | null {
    return this.#overview;
  }

  /** Why what is on disk couldn't be read. */
  get overviewError(): string | null {
    return this.#overviewQuery.error;
  }

  /** Commits each open PR's base has that GitHub's head lacks. */
  get behind(): Map<number, number | null> {
    return this.#behind;
  }

  /** The restack preview, when the base moved or a PR left the head below it. */
  get preview(): RestackPlan | null {
    return this.#planQuery.value ?? null;
  }

  /** Where each open PR is on disk; `null` until read. */
  get checkouts(): Map<number, PrCheckout> | null {
    return this.#checkouts;
  }

  /** The restack in progress or just finished. */
  get progress(): RestackProgress | null {
    return this.#progress;
  }

  /** A restack that has not finished blocks other edits; a finished one only shows its result. */
  get restacking(): boolean {
    return this.#progress !== null && !this.#progress.done;
  }

  /** The git branch the restack stopped at. */
  get conflict(): RestackProgress["branches"][number] | null {
    return this.#progress?.branches.find((branch) => branch.status === "conflict") ?? null;
  }

  get busy(): StackWork | null {
    return this.#busy;
  }

  /** What to offer after a PR merged, unless put off. */
  get followUp(): MergeFollowUp | null {
    return this.#followUp;
  }

  /** Someone else's stack: the page leads with reviewing it, not editing it. */
  get theirs(): boolean {
    return this.#openPrs.length > 0 && !this.#openPrs.some((pr) => this.#isMine(pr));
  }

  /** Read the inbox if it hasn't been, then what is on disk; `restack`
   *  starts one then unless one is already under way. */
  async load({ restack = false }: { restack?: boolean } = {}): Promise<void> {
    await (inboxes[this.repoDir]?.inbox ? this.#reload() : syncInbox(this.repoDir).then(() => this.#reload()));
    if (restack && !this.#progress) await this.restack();
  }

  /** Restack from `from` (or the bottom) onto the base, making the stack's
   *  worktree first if it has none. True once the stack sits on the base:
   *  restacked, or already there. */
  async restack(from: number | null = null): Promise<boolean> {
    const input = this.#input;
    if (!input) return false;
    try {
      let plan = await commands.reviewRestackPlan(this.repoDir, input, from);
      if (!plan.worktree) {
        if (!(await this.#ensureWorktree(this.#openPrs.at(-1)))) return false;
        plan = await commands.reviewRestackPlan(this.repoDir, input, from);
      }
      if (plan.steps.every((step) => step.unchanged)) {
        toaster.info({ title: `Already on ${input.baseRef}`, description: "Nothing to restack." });
        return true;
      }
      const running = plan.worktree ? await this.#runningIn(plan.worktree) : 0;
      if (!(await this.#confirm({ kind: "restack", plan, from, running }))) return false;
      this.#busy = "restack";
      const progress = await commands.reviewRestack(this.repoDir, input, from);
      this.#progress = progress;
      if (progress.done)
        toaster.success({ title: `Restacked onto ${input.baseRef}`, description: "Push the stack to update GitHub." });
      return progress.done;
    } catch (cause) {
      toastFailure(cause);
      return false;
    } finally {
      this.#busy = null;
      await this.#reload();
    }
  }

  /** Carry on a restack stopped at a conflict, once it is resolved. */
  async continueRestack(): Promise<void> {
    this.#busy = "restack";
    try {
      const progress = await commands.reviewContinueRestack(this.repoDir, this.number);
      this.#progress = progress;
      const left = this.conflict;
      if (left) toastError(`Conflict markers are still in ${left.conflicts.join(", ")}`);
      else if (progress.done)
        toaster.success({ title: "Restack finished", description: "Push the stack to update GitHub." });
    } catch (cause) {
      toastFailure(cause);
    } finally {
      this.#busy = null;
      await this.#reload();
    }
  }

  /** Put every git branch back where it was before the restack. */
  async abortRestack(): Promise<void> {
    this.#busy = "restack";
    try {
      await commands.reviewAbortRestack(this.repoDir, this.number);
      this.#progress = null;
      toaster.info({ title: "Restack aborted", description: "Every git branch is back where it was." });
    } catch (cause) {
      toastFailure(cause);
    } finally {
      this.#busy = null;
      await this.#reload();
    }
  }

  /** Stop showing the restack's result. An unfinished one comes back with
   *  the next read of what is on disk. */
  dismissProgress(): void {
    this.#progress = null;
  }

  /** Start a thread in the worktree, linked to the conflicted PR, that
   *  resolves the conflict; the thread and where it runs, to open. */
  async resolveInThread(): Promise<{ threadId: string; cwd: string } | null> {
    const conflict = this.conflict;
    const progress = this.#progress;
    const pr = conflict && this.#openPrs.find((candidate) => candidate.number === conflict.number);
    if (!conflict || !progress || !pr) return null;
    // Not `startPrThread`: mid-rebase the worktree is on no git branch, so
    // that would start a read-only thread, which can't resolve anything.
    try {
      const { thread } = await startThreadWithTurn(
        progress.worktree,
        [{ type: "text", text: conflictPrompt(conflict.headRef, progress.ontoRef, conflict.conflicts) }],
        {
          beforeTurn: async (started) => {
            rememberPrLink(await commands.reviewLinkThread(this.repoDir, started.id, this.#targetFor(pr)));
          },
        },
      );
      return { threadId: thread.id, cwd: progress.worktree };
    } catch (cause) {
      toastFailure(cause);
      return null;
    }
  }

  /** Push every open git branch, or only PR `only`'s. Without a worktree it
   *  steps up to one first; from one, pushing to someone else's git branch
   *  is asked every time. */
  async push(only: number | null = null): Promise<boolean> {
    const input = this.#input;
    const pushing = this.#openPrs.filter((candidate) => only === null || candidate.number === only);
    const pr = pushing.at(-1);
    if (!input || !pr) return false;
    const worktree = this.#overview?.worktree;
    if (!worktree) {
      // Steps up to a worktree, asking first about someone else's git branch.
      if (
        !(await stepUpFor({
          repoDir: this.repoDir,
          repoName: this.repoName,
          target: this.#targetFor(pr),
          action: "push",
        }))
      )
        return false;
    } else {
      // The stack's worktree holds every one of its git branches, whichever
      // is checked out.
      const other = pushing.find((candidate) => !this.#isMine(candidate));
      if (
        other &&
        !(await this.#confirm({
          kind: "pushToTheirBranch",
          target: this.#targetFor(other),
          worktreePath: worktree.path,
        }))
      )
        return false;
    }
    return this.#work("push", async () => {
      const report = await commands.reviewPushStack(this.repoDir, input, only);
      toaster.success({
        title:
          report.pushed.length === 0
            ? "GitHub is up to date"
            : `Pushed ${report.pushed.length} git branch${report.pushed.length === 1 ? "" : "es"}`,
        description: report.pushed.join(", ") || undefined,
      });
      // The finished restack's card offered this push; it has done its job.
      if (only === null && this.#progress?.done) this.#progress = null;
    });
  }

  /** Change the stack's shape, asking first where `StackQuestion` says.
   *  True when it ran. */
  async edit(change: StackEdit): Promise<boolean> {
    const input = this.#input;
    if (!input) return false;
    const { repoDir } = this;
    switch (change.kind) {
      case "reorder":
        if (!(await this.#confirm({ kind: "reorder", order: change.order }))) return false;
        return this.#work("reorder", () => commands.reviewReorderStack(repoDir, input, change.order));
      case "add":
        return this.#work("add", () => commands.reviewAddStackBranch(repoDir, input, change.name, change.below));
      case "split":
        return this.#work("split", async () => {
          const created = await commands.reviewSplitBranch(repoDir, input, change.number, change.split);
          toaster.success({ title: `Opened #${created} below #${change.number}` });
        });
      case "fold": {
        const at = this.#openPrs.findIndex((pr) => pr.number === change.number);
        const [parent, pr] = [this.#openPrs[at - 1], this.#openPrs[at]];
        if (!pr || !parent || !(await this.#confirm({ kind: "fold", pr, parent }))) return false;
        return this.#work("fold", () => commands.reviewFoldIntoParent(repoDir, input, pr.number));
      }
      case "rename":
        return this.#work("rename", () => commands.reviewRenameStackBranch(repoDir, input, change.number, change.to));
      case "remove": {
        const pr = this.#openPrs.find((candidate) => candidate.number === change.number);
        if (!pr || !(await this.#confirm({ kind: "remove", pr }))) return false;
        return this.#work("remove", () => commands.reviewRemoveFromStack(repoDir, input, pr.number));
      }
    }
  }

  /** Put PR `number`'s git branch in the stack's worktree, making the
   *  worktree if there is none. Without a PR: a worktree for the whole stack
   *  and a local git branch for each PR, or just what is missing. */
  async checkOut(number: number | null = null): Promise<boolean> {
    const input = this.#input;
    if (!input) return false;
    if (number === null) {
      const top = this.#openPrs.at(-1);
      if (!top) return false;
      const request = { repoDir: this.repoDir, repoName: this.repoName, input, target: this.#targetFor(top) };
      return this.#work("checkout", () => checkOutStack(request));
    }
    const pr = this.#openPrs.find((candidate) => candidate.number === number);
    const path = await this.#ensureWorktree(pr);
    if (!pr || !path || this.#overview?.worktree?.branch === pr.headRef) return false;
    return this.#work("checkout", () => commands.gitCheckoutBranch(path, pr.headRef, false));
  }

  /** One agent review over the open PRs, bottom first, filed under the top one. */
  async reviewWhole(): Promise<void> {
    const prs = this.#openPrs.flatMap((pr) => this.#prByNumber.get(pr.number) ?? []);
    if (prs.length === 0) return;
    try {
      await startStackReview(this.repoDir, prs, { viewer: this.#inbox?.viewer ?? null });
      toaster.success({
        title: "Stack review queued",
        description: `One agent review over ${prs.map((pr) => `#${pr.number}`).join(", ")}, filed under #${prs.at(-1)?.number}.`,
      });
    } catch (cause) {
      toastFailure(cause);
    }
  }

  /** The follow-up after a merge: restack the rest onto the base, then
   *  delete the merged git branches. Nothing is deleted unless the stack
   *  ends up on the base. */
  async cleanUpMerged(): Promise<boolean> {
    const input = this.#input;
    if (!input || !(await this.restack())) return false;
    return this.#work("cleanup", async () => {
      const deleted = await commands.reviewDeleteMergedBranches(this.repoDir, input);
      if (deleted.length > 0) toaster.success({ title: `Deleted ${deleted.join(", ")}` });
    });
  }

  /** The follow-up once every PR merged: archive the worktree's threads,
   *  then remove it. */
  async removeWorktree(): Promise<boolean> {
    const path = this.#overview?.worktree?.path;
    if (!path) return false;
    const threads = projects()
      .flatMap((project) => project.threads)
      .filter((thread) => thread.cwd === path || thread.cwd.startsWith(`${path}/`));
    if (!(await this.#confirm({ kind: "removeWorktree", path, threads: threads.length }))) return false;
    return this.#work("cleanup", async () => {
      for (const thread of threads) await commands.archiveThread(thread.id);
      await commands.gitWorktreeRemove(this.repoDir, path, false);
    });
  }

  /** "Later": the follow-up comes back when another PR merges. */
  putOffFollowUp(): void {
    if (this.#input) putOffFollowUp(followUpKey(this.#input));
  }

  /** Run one edit with the stack busy, then re-read the inbox and the disk. */
  async #work(label: StackWork, task: () => Promise<unknown>): Promise<boolean> {
    this.#busy = label;
    try {
      await task();
      return true;
    } catch (cause) {
      toastFailure(cause);
      return false;
    } finally {
      this.#busy = null;
      await resyncInbox(this.repoDir);
      await this.#reload();
    }
  }

  /** Wait for what a write changed: it marked these stale, so this waits
   *  for new reads, not the cached answers. */
  async #reload(): Promise<void> {
    const input = this.#input;
    if (!input) return;
    await Promise.all([
      readFresh("reviewStackOverview", this.repoDir, input).catch(() => null),
      readFresh("reviewPrsBehind", this.repoDir, this.#behindInputs).catch(() => null),
    ]);
  }

  /** The stack's worktree, creating one on `pr`'s git branch if it has none. */
  async #ensureWorktree(pr: StackInputPr | undefined): Promise<string | null> {
    const worktree = this.#overview?.worktree;
    if (worktree) return worktree.path;
    if (!pr) return null;
    const checkout = await stepUpFor({
      repoDir: this.repoDir,
      repoName: this.repoName,
      target: this.#targetFor(pr),
      action: "restack",
    });
    if (!checkout?.path) return null;
    await this.#reload();
    return checkout.path;
  }

  /** Threads running now in the worktree at `path`. */
  async #runningIn(path: string): Promise<number> {
    const active = new Set(await commands.threadsWithActiveTurns().catch(() => [] as string[]));
    const prefix = path.replace(/\/+$/, "");
    return projects()
      .flatMap((project) => project.threads)
      .filter((thread) => active.has(thread.id) && (thread.cwd === prefix || thread.cwd.startsWith(`${prefix}/`)))
      .length;
  }

  /** The viewer wrote this PR: pushing to its git branch needs no asking. */
  #isMine(pr: StackInputPr): boolean {
    return !!this.#inbox && this.#prByNumber.get(pr.number)?.author === this.#inbox.viewer;
  }

  #targetFor(pr: StackInputPr): CheckoutTarget {
    const input = this.#input;
    return checkoutTarget(
      pr,
      this.#isMine(pr),
      input ? { name: input.name, branches: this.#openPrs.map((open) => open.headRef) } : null,
    );
  }
}

/** The base moved, or a PR no longer sits on the head of the one below. */
function needsRestack(next: StackOverview, behind: Map<number, number | null>): boolean {
  return next.baseAhead > 0 || [...behind.values()].some((count) => (count ?? 0) > 0);
}
