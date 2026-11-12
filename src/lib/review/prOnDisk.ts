// A PR on disk, from the frontend: the one way a PR or a Stack gets checked
// out, the one way a stack's worktree switches PR, and the one way a thread
// starts on a PR.
//
// A PR is either not checked out or checked out in its worktree (a lone PR's
// own, or its Stack's). Only `checkOut` makes one; actions that need it read
// `checkoutOf` and stay disabled with `CHECK_OUT_FIRST` until it is set. The
// backend works out the PR from its number. `bun run check` holds the seam:
// the checkout, adopt and switch commands are called from here and the
// preview desk only (`scripts/check-seams.ts`).

import { quietRefresh } from "$lib/app/appData.svelte";
import { openDialog } from "$lib/app/dialogs.svelte";
import { syncInbox } from "$lib/inbox/inboxStore.svelte";
import AdoptWorktreeDialog from "$lib/review/AdoptWorktreeDialog.svelte";
import { linkOrDelete } from "$lib/review/prThreadLink";
import { type StartedThread, startThreadWithTurn } from "$lib/services/api";
import { commands } from "$lib/services/homeRouting";
import { query } from "$lib/services/queryCache.svelte";
import { nameFromOpening } from "$lib/thread/draftThread.svelte";
import { withWorktreeContext } from "$lib/thread/worktreeContext";
import { toastError, toaster, toastFailure, toastProgress, toastUndo } from "$lib/toaster";
import type {
  CheckOutOutcome,
  HarnessKind,
  PrCheckout,
  PrLink,
  Project,
  Turn,
  TurnOptions,
  UserInputPart,
} from "$lib/types";
import { basename } from "$lib/utils/path";

/** The hint on anything that needs the PR's worktree before it exists. */
export const CHECK_OUT_FIRST = "Check out first";

/** What to check out: a PR (a stack's PR checks out the whole stack), or a
 *  stack by GitHub's number. */
export type CheckOutWhat = { pr: number } | { stack: number };

type BranchInUse = Extract<CheckOutOutcome, { kind: "branchInUse" }>;
type CheckedOut = Extract<CheckOutOutcome, { kind: "checkedOut" }>;

/**
 * Check `what` out, with the Undo toast for a new worktree and the setup
 * command's progress. A git branch already checked out somewhere is offered
 * for adoption. Resolves the checkout once setup has finished (unless
 * `awaitSetup` is false), or `null` when cancelled or failed.
 */
export async function checkOut(
  repoDir: string,
  what: CheckOutWhat,
  { awaitSetup = true }: { awaitSetup?: boolean } = {},
): Promise<PrCheckout | null> {
  try {
    const outcome =
      "pr" in what
        ? await commands.reviewCheckOut(repoDir, what.pr)
        : await commands.reviewCheckOutStack(repoDir, what.stack);
    if (outcome.kind === "branchInUse") return await adopt(repoDir, outcome);
    announce(repoDir, outcome);
    const { checkout } = outcome;
    if (checkout.setupPending) {
      const setup = runSetup(repoDir, checkout.number, basename(checkout.path), checkout.setupPending);
      if (awaitSetup) await setup;
    }
    return checkout;
  } catch (cause) {
    toastFailure(cause);
    return null;
  }
}

/** Check each of `numbers` out, one after another; a stack is checked out
 *  once, whichever of its PRs comes first. Resolves how many are checked out. */
export async function checkOutAll(repoDir: string, numbers: number[]): Promise<number> {
  const done = new Set<number>();
  let checkedOut = 0;
  for (const number of numbers) {
    if (done.has(number)) continue;
    const checkout = await checkOut(repoDir, { pr: number }, { awaitSetup: false });
    if (!checkout) continue;
    checkedOut++;
    done.add(number);
    if (checkout.stack !== null) {
      const stack = await commands.reviewPrCheckouts(repoDir).catch(() => [] as PrCheckout[]);
      for (const member of stack) if (member.path === checkout.path) done.add(member.number);
    }
  }
  return checkedOut;
}

async function adopt(repoDir: string, found: BranchInUse): Promise<PrCheckout | null> {
  const choice = await openDialog(AdoptWorktreeDialog, {
    number: found.number,
    branch: found.branch,
    path: found.path,
    mainClone: found.mainClone,
  });
  if (!choice) return null;
  const checkout = await commands.reviewAdoptWorktree(repoDir, found.number, found.path);
  toaster.success({
    title: `#${found.number} is checked out in ${found.mainClone ? "the main clone" : basename(found.path)}`,
    description: "Pingex leaves this worktree where it is: it never switches, updates or removes it.",
  });
  return checkout;
}

function announce(repoDir: string, outcome: CheckedOut): void {
  const { checkout, created, createdBranches, kept, localStackWarning } = outcome;
  const name = basename(checkout.path);
  const title =
    checkout.stack === null ? `Checked out #${checkout.number} in ${name}` : `Checked out the stack in ${name}`;
  const made = createdBranches.length;
  const description = made > 0 ? `${made} new local branch${made === 1 ? "" : "es"}` : undefined;
  if (created) {
    // A stack's local branches stay: they hold nothing GitHub doesn't.
    toastUndo(title, () => undo(commands.reviewRemovePrWorktree(repoDir, checkout.number)), description);
  } else {
    toaster.success({ title, description });
  }
  for (const { branch, reason } of kept) toaster.warning({ title: `${branch} left as it is`, description: reason });
  if (localStackWarning) {
    toaster.warning({ title: "gh stack doesn't track the stack", description: localStackWarning });
  }
}

/** The setup command, with a toast while it runs. */
async function runSetup(repoDir: string, number: number, name: string, command: string): Promise<void> {
  const toast = toastProgress(`Installing dependencies in ${name}`, command);
  try {
    await commands.reviewRunCheckoutSetup(repoDir, number);
  } catch (cause) {
    toastFailure(cause);
  } finally {
    toaster.dismiss(toast);
  }
}

async function undo(work: Promise<unknown>): Promise<void> {
  try {
    await work;
    void quietRefresh();
  } catch (cause) {
    toastFailure(cause);
  }
}

/** Catch a checked-out stack's worktree up with GitHub where it is: every
 *  open PR gets its local git branch and its row, nothing moves. False when
 *  the stack is gone or refused (a stopped restack refuses it). */
export async function refreshStackCheckout(repoDir: string, stack: number): Promise<boolean> {
  try {
    // The desk reads the PRs' heads from the inbox's last sync.
    await syncInbox(repoDir);
    const outcome = await commands.reviewCheckOutStack(repoDir, stack);
    return outcome.kind === "checkedOut";
  } catch {
    return false;
  }
}

/** How long a stack switch waits between looks at the running threads. */
const WAIT_MS = 2000;

/** Threads with a turn running now in the worktree at `path`. */
export async function runningIn(projects: Project[], path: string): Promise<string[]> {
  const active = new Set(await commands.threadsWithActiveTurns());
  const prefix = path.replace(/\/+$/, "");
  return projects
    .flatMap((project) => project.threads)
    .filter((thread) => active.has(thread.id) && (thread.cwd === prefix || thread.cwd.startsWith(`${prefix}/`)))
    .map((thread) => thread.id);
}

/**
 * Put the stack worktree PR `number` is checked out in on its git branch,
 * once no thread is running there. Resolves the checkout, which is the PR's
 * already when it is on its branch or not a stack's; `null` when it is not
 * checked out, the wait was stopped, or the switch was refused (changes, a
 * rebase, an adopted worktree).
 */
export async function switchToPr(
  repoDir: string,
  number: number,
  projects: () => Project[] = () => [],
): Promise<PrCheckout | null> {
  try {
    const checkout = await commands.reviewPrCheckout(repoDir, number);
    if (!checkout) {
      toastError(`Check out #${number} first`);
      return null;
    }
    if (checkout.onPrBranch || checkout.stack === null) return checkout;
    const name = basename(checkout.path);
    let waiting: string | null = null;
    let stopped = false;
    // A failed read says nothing, so it keeps waiting: switching under a
    // running thread is what this is here to avoid.
    while (!stopped && (await runningIn(projects(), checkout.path).catch(() => ["unknown"])).length > 0) {
      waiting ??= toastProgress(`Waiting to switch ${name}`, "Threads are still running here", {
        label: "Stop waiting",
        onClick: () => {
          stopped = true;
        },
      });
      await new Promise((resolve) => setTimeout(resolve, WAIT_MS));
    }
    if (waiting) toaster.dismiss(waiting);
    if (stopped) return null;
    const switched = await commands.reviewSwitchStackBranch(repoDir, number);
    toaster.success({ title: `${name} is on ${switched.gitBranch ?? `#${number}'s branch`}` });
    return switched;
  } catch (cause) {
    toastFailure(cause);
    return null;
  }
}

/** PR `number`'s worktree as a cached read, for a component: `value` is
 *  `null` when it is not checked out and `undefined` until read. Call during
 *  component setup. */
export function checkoutOf(repoDir: () => string, number: () => number | null) {
  return query("reviewPrCheckout", () => {
    const pr = number();
    return pr === null ? null : [repoDir(), pr];
  });
}

/** The read-only sandbox, replacing whatever permissions `options` carried:
 *  "Ask only" on one turn. */
export function readOnly(options: TurnOptions | undefined): TurnOptions {
  const { mode: _mode, ...rest } = options ?? {};
  return { ...rest, approvalPolicy: "on-request", sandboxMode: "read-only" };
}

/** A run nobody is watching, in a PR's worktree: it can build, test and write
 *  there, and asks before reaching further. It never inherits Full Access. */
export function workspaceWrite(): TurnOptions {
  return { approvalPolicy: "on-request", sandboxMode: "workspace-write" };
}

export interface PrThreadOptions {
  /** The first message, without Worktree context: that is added here. */
  prompt: UserInputPart[];
  harness?: HarnessKind | null;
  /** Turn options from the composer prefs. Without them the thread edits its
   *  worktree and asks before anything else (`workspaceWrite`). */
  options?: TurnOptions;
  /** Threads that could be running in the worktree, for a stack switch. */
  projects?: () => Project[];
}

/**
 * Start a thread on PR `number`, the only way one starts: in its worktree,
 * writable, titled from `prompt`, with the worktree's context, and filed
 * under the PR before its first turn. A stack's worktree on another PR's
 * branch switches first. Throws when the PR is not checked out; a thread whose
 * link can't be written is deleted before it runs.
 */
export async function startPrThread(
  repoDir: string,
  number: number,
  { prompt, harness, options, projects }: PrThreadOptions,
): Promise<{ thread: StartedThread; turn: Turn; link: PrLink }> {
  let checkout = await commands.reviewPrCheckout(repoDir, number);
  if (!checkout) throw new Error(`Check out #${number} first`);
  if (checkout.stack !== null && !checkout.onPrBranch) {
    checkout = await switchToPr(repoDir, number, projects);
    if (!checkout) throw new Error(`The stack's worktree is not on #${number}'s branch`);
  }
  const context = await commands.reviewWorktreeContext(checkout.path).catch(() => null);
  const input = context ? withWorktreeContext(prompt, context.text) : prompt;
  const recorded: { link?: PrLink } = {};
  const started = await startThreadWithTurn(checkout.path, input, {
    harness,
    options: options ?? workspaceWrite(),
    beforeTurn: async (thread) => {
      recorded.link = await linkOrDelete(repoDir, thread.id, number);
    },
  });
  nameFromOpening(started.thread.id, prompt);
  if (!recorded.link) throw new Error("The thread started without its PR link");
  return { ...started, link: recorded.link };
}
