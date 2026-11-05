// Starting a thread about a pull request: in the PR's checkout (or the
// repository's clone at No checkout), recorded with a PR link, and below
// the Worktree step with the read-only sandbox, so reviewing cannot change
// the PR. Agent reviews, "Fix CI in thread" and "New thread on this PR" all
// start here; callers step the PR up first (`stepUpFor`) when the action
// needs more than it has.

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
  /** Turn options from the composer prefs; the sandbox is set here. */
  options?: TurnOptions;
}

/** The read-only sandbox, replacing whatever permissions `options` carried. */
export function readOnly(options: TurnOptions | undefined): TurnOptions {
  const { mode: _mode, ...rest } = options ?? {};
  return { ...rest, approvalPolicy: "on-request", sandboxMode: "read-only" };
}

export async function startPrThread(
  request: PrThreadRequest,
): Promise<{ thread: StartedThread; turn: Turn; link: PrLink }> {
  const { repoDir, target, input, harness, options } = request;
  const checkout = await commands.reviewCheckoutState(repoDir, target);
  const editable = checkout.step === "worktree";
  if (!editable && !can(harness, "readOnlySandbox")) {
    throw new Error(`${harnessLabel(harness)} has no read-only sandbox, so it can't work on #${target.number} here`);
  }
  const recorded: { link?: PrLink } = {};
  const started = await startThreadWithTurn(checkout.path ?? repoDir, input, {
    harness,
    options: editable ? options : readOnly(options),
    beforeTurn: async (thread) => {
      recorded.link = await commands.reviewLinkThread(repoDir, thread.id, target);
      rememberPrLink(recorded.link);
    },
  });
  if (!recorded.link) throw new Error("The thread started without its PR link");
  return { ...started, link: recorded.link };
}
