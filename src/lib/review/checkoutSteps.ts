// Moving a PR up to a worktree from the frontend: ask the backend for the step
// plan, confirm what it says to confirm, make the worktree, and show the toasts
// (Undo, setup). Buttons call `stepUpFor` with the action they perform, then do
// their work in the checkout it returns.

import { openDialog } from "$lib/app/dialogs.svelte";
import { newThreadInDir } from "$lib/app/navigation.svelte";
import CheckoutStepDialog from "$lib/review/CheckoutStepDialog.svelte";
import { expectPrThread } from "$lib/review/prLinks.svelte";
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
    const ask = (confirm: NonNullable<typeof plan.confirm>) =>
      openDialog(CheckoutStepDialog, {
        confirm,
        action,
        target,
        repoName,
        worktreePath: plan.worktreePath,
      });
    if (plan.confirm) {
      const choice = await ask(plan.confirm);
      if (!choice) return null;
      if (choice.dontAskAgain) await commands.reviewSetCheckoutPolicy(repoDir, "ask", true);
    }
    // Pushing to someone else's git branch is asked every time, also when
    // the first question was about the step up; before anything is created.
    if (plan.confirmPush && plan.confirm !== "pushToTheirBranch" && !(await ask("pushToTheirBranch"))) return null;
    if (plan.stepUp === "worktree") {
      // An editor can open before dependencies finish; an agent shouldn't start.
      return await createPrWorktree(repoDir, target, { awaitSetup: action !== "openInEditor" });
    }
    return plan.current;
  } catch (cause) {
    toastError(errorMessage(cause));
    return null;
  }
}

/** Make `target`'s worktree without asking, with the Undo toast and its setup
 *  command: for callers that already showed the path, as Start review does.
 *  Resolves once setup has finished, so nothing starts before dependencies are
 *  in, unless the caller says otherwise. */
export async function createPrWorktree(
  repoDir: string,
  target: CheckoutTarget,
  { awaitSetup = true }: { awaitSetup?: boolean } = {},
): Promise<PrCheckout> {
  const checkout = await commands.reviewEnsurePrWorktree(repoDir, target);
  const path = checkout.path ?? "";
  const name = basename(path);
  toastUndo(
    `Created worktree ${name}`,
    () =>
      undo(
        checkout.managed
          ? commands.reviewRemovePrWorktree(repoDir, target.number)
          : commands.gitWorktreeRemove(repoDir, path, false),
      ),
    `#${target.number} at ${(checkout.headSha ?? target.headSha).slice(0, 7)}`,
  );
  if (checkout.setupPending) {
    const setup = runSetup(repoDir, target.number, name, checkout.setupPending);
    if (awaitSetup) await setup;
  }
  return checkout;
}

/** Get `target` a worktree and open a draft thread there, filed under the PR
 *  when it starts. */
export async function workOnPr(request: StepRequest): Promise<PrCheckout | null> {
  const checkout = await stepUpFor(request);
  if (!checkout?.path) return null;
  expectPrThread(checkout.path, request.repoDir, request.target);
  newThreadInDir(checkout.path);
  return checkout;
}

/** The setup command, with a toast while it runs. */
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
