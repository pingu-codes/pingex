// Starting a thread about a pull request: in the PR's worktree, recorded with
// a PR link. A worktree is a place to build, run and commit, so the thread gets
// the permissions it was asked for. Only at No checkout is it read-only, because
// it then runs in the user's own clone. Agent reviews and "Fix CI in thread"
// start here; callers step the PR up first (`stepUpFor`) when the action needs
// more than it has.

import { rememberPrLink } from "$lib/review/prLinks.svelte";
import { type StartedThread, startThreadWithTurn } from "$lib/services/api";
import { commands } from "$lib/services/homeRouting";
import type { CheckoutTarget, HarnessKind, PrLink, Turn, TurnOptions, UserInputPart } from "$lib/types";
import { can, harnessLabel } from "$lib/utils/capabilities";

export interface PrThreadRequest {
  repoDir: string;
  target: CheckoutTarget;
  /** The first message. */
  input: UserInputPart[];
  harness?: HarnessKind | null;
  /** Turn options from the composer prefs. Without them, a worktree thread
   *  edits its worktree and asks before anything else (`workspaceWrite`). */
  options?: TurnOptions;
}

/** The read-only sandbox, replacing whatever permissions `options` carried. */
export function readOnly(options: TurnOptions | undefined): TurnOptions {
  const { mode: _mode, ...rest } = options ?? {};
  return { ...rest, approvalPolicy: "on-request", sandboxMode: "read-only" };
}

/** A run nobody is watching, in a PR's worktree: it can build, test and write
 *  there, and asks before reaching further. It never inherits Full Access. */
export function workspaceWrite(): TurnOptions {
  return { approvalPolicy: "on-request", sandboxMode: "workspace-write" };
}

export async function startPrThread(
  request: PrThreadRequest,
): Promise<{ thread: StartedThread; turn: Turn; link: PrLink }> {
  const { repoDir, target, input, harness, options } = request;
  const checkout = await commands.reviewCheckoutState(repoDir, target);
  const inWorktree = checkout.path !== null;
  if (!inWorktree && !can(harness, "readOnlySandbox")) {
    throw new Error(`${harnessLabel(harness)} has no read-only sandbox, so it can't work on #${target.number} here`);
  }
  const recorded: { link?: PrLink } = {};
  const started = await startThreadWithTurn(checkout.path ?? repoDir, input, {
    harness,
    options: inWorktree ? (options ?? workspaceWrite()) : readOnly(options),
    beforeTurn: async (thread) => {
      recorded.link = await commands.reviewLinkThread(repoDir, thread.id, target);
      rememberPrLink(recorded.link);
    },
  });
  if (!recorded.link) throw new Error("The thread started without its PR link");
  return { ...started, link: recorded.link };
}
