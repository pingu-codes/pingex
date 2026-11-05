// Moving a PR up the checkout steps from the frontend: ask the backend for
// the step plan, confirm what it says to confirm, run the step, and show the
// toasts (Undo, setup in the background). Buttons call `stepUpFor` with the
// action they perform, then do their work in the checkout it returns.

import { openDialog } from "$lib/app/dialogs.svelte";
import CheckoutStepDialog from "$lib/review/CheckoutStepDialog.svelte";
import { commands } from "$lib/services/homeRouting";
import { toastError, toaster, toastProgress, toastUndo } from "$lib/toaster";
import type { CheckoutTarget, DeskAction, PrCheckout } from "$lib/types";
import { errorMessage } from "$lib/utils/errors";
import { basename } from "$lib/utils/path";

export interface StepRequest {
  /** The repository's clone, as the sidebar project has it. */
  repoDir: string;
  /** The repository's name, for "Don't ask again for <repository>". */
  repoName: string;
  target: CheckoutTarget;
  action: DeskAction;
}

/**
 * Get `target` to the checkout step `action` needs, and no further. Resolves
 * with the PR's checkout, or `null` when the user cancelled or the
 * repository's policy refused.
 */
export async function stepUpFor(request: StepRequest): Promise<PrCheckout | null> {
  const { repoDir, repoName, target, action } = request;
  try {
    const plan = await commands.reviewPlanStep(repoDir, target, action);
    if (plan.refused) {
      toastError(plan.refused);
      return null;
    }
    let way: "reviewCheckout" | "promote" | "worktree" | "proceed" | null = null;
    if (plan.confirm) {
      const choice = await openDialog(CheckoutStepDialog, {
        confirm: plan.confirm,
        action,
        target,
        repoName,
        reviewCheckoutPath: plan.current.path ?? plan.reviewCheckoutPath,
      });
      if (!choice) return null;
      way = choice.way;
      if (choice.dontAskAgain) await commands.reviewSetCheckoutPolicy(repoDir, "ask", true);
    }
    if (plan.stepUp === "reviewCheckout") return await createReviewCheckout(repoDir, target);
    if (plan.stepUp === "worktree") {
      return way === "promote"
        ? await commands.reviewPromoteCheckout(repoDir, target)
        : await createWorktree(repoDir, target);
    }
    return plan.current;
  } catch (cause) {
    toastError(errorMessage(cause));
    return null;
  }
}

/** Create a review checkout of `target` without asking, with the Undo toast
 *  and setup in the background: for callers that already showed the path,
 *  as Start review does. */
export async function createReviewCheckout(repoDir: string, target: CheckoutTarget): Promise<PrCheckout> {
  const checkout = await commands.reviewEnsureReviewCheckout(repoDir, target);
  const name = basename(checkout.path ?? "");
  toastUndo(
    `Created review checkout ${name}`,
    () => undo(commands.reviewRemoveReviewCheckout(repoDir, target.number)),
    `#${target.number} at ${(checkout.headSha ?? target.headSha).slice(0, 7)}`,
  );
  if (checkout.setupPending) void runSetup(repoDir, target.number, name, checkout.setupPending);
  return checkout;
}

async function createWorktree(repoDir: string, target: CheckoutTarget): Promise<PrCheckout> {
  const checkout = await commands.reviewCreatePrWorktree(repoDir, target, null);
  const path = checkout.path ?? "";
  toastUndo(`Created worktree ${basename(path)} from ${target.headRef}`, () =>
    undo(commands.gitWorktreeRemove(repoDir, path, false)),
  );
  return checkout;
}

/** The setup command, in the background, with a toast while it runs. */
async function runSetup(repoDir: string, number: number, name: string, command: string): Promise<void> {
  const toast = toastProgress(`Installing dependencies in ${name}`, command);
  try {
    await commands.reviewRunCheckoutSetup(repoDir, number);
  } catch (cause) {
    toastError(errorMessage(cause));
  } finally {
    toaster.dismiss(toast);
  }
}

async function undo(work: Promise<unknown>): Promise<void> {
  try {
    await work;
  } catch (cause) {
    toastError(errorMessage(cause));
  }
}
