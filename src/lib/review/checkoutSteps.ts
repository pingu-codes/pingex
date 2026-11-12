// Moving a PR up to a worktree from the frontend: ask the backend for the step
// plan, confirm what it says to confirm, make the worktree, and show the toasts
// (Undo, setup). Buttons call `stepUpFor` with the action they perform, then do
// their work in the checkout it returns.

import { openDialog } from "$lib/app/dialogs.svelte";
import { newThreadInDir } from "$lib/app/navigation.svelte";
import CheckoutStepDialog from "$lib/review/CheckoutStepDialog.svelte";
import { expectPrThread } from "$lib/review/prLinks.svelte";
import { commands } from "$lib/services/homeRouting";
import { toastError, toaster, toastFailure, toastProgress, toastUndo } from "$lib/toaster";
import type { CheckoutTarget, DeskAction, PrCheckout, StackCheckout, StackInput } from "$lib/types";
import { plural, shortSha } from "$lib/utils/format";
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
        fromBranch: plan.current.gitBranch,
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
    if (plan.switchBranch) {
      const checkout = await commands.reviewEnsurePrWorktree(repoDir, target);
      toaster.success({ title: `Switched ${basename(plan.worktreePath)} to ${target.headRef}` });
      return checkout;
    }
    return plan.current;
  } catch (cause) {
    toastFailure(cause);
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
    `#${target.number} at ${shortSha(checkout.headSha ?? target.headSha)}`,
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

export interface StackCheckoutRequest {
  repoDir: string;
  repoName: string;
  input: StackInput;
  /** The stack's top PR. */
  target: CheckoutTarget;
}

/**
 * A worktree for the whole stack on its top PR's git branch, and a local git
 * branch for each open PR. Asks only when a worktree will be made; a stack that
 * has one just gets what is missing. Resolves with `null` when cancelled or
 * refused.
 */
export async function checkOutStack(request: StackCheckoutRequest): Promise<StackCheckout | null> {
  const { repoDir, repoName, input, target } = request;
  const open = input.prs.filter((pr) => pr.open && !pr.merged).length;
  try {
    const plan = await commands.reviewPlanStep(repoDir, target, "workOnPr");
    if (plan.stepUp === "worktree") {
      const choice = await openDialog(CheckoutStepDialog, {
        confirm: "createWorktree" as const,
        action: "workOnPr" as const,
        target,
        repoName,
        worktreePath: plan.worktreePath,
        stackSize: open,
      });
      if (!choice) return null;
    }
    const result = await commands.reviewEnsureStackWorktree(repoDir, input);
    announceStackCheckout(repoDir, result, open);
    if (result.checkout.setupPending) {
      await runSetup(repoDir, target.number, basename(result.checkout.path ?? ""), result.checkout.setupPending);
    }
    return result;
  } catch (cause) {
    toastFailure(cause);
    return null;
  }
}

function announceStackCheckout(repoDir: string, result: StackCheckout, open: number): void {
  const path = result.checkout.path ?? "";
  const title = `Checked out ${plural(open, "PR")} in ${basename(path)}`;
  const made = result.createdBranches.length;
  const description = made > 0 ? `${made} new local branch${made === 1 ? "" : "es"}` : undefined;
  if (result.created) {
    // The local branches stay: they hold nothing GitHub doesn't.
    toastUndo(title, () => undo(commands.gitWorktreeRemove(repoDir, path, false)), description);
  } else {
    toaster.success({ title, description });
  }
  for (const { branch, reason } of result.kept) {
    toaster.warning({ title: `${branch} left as it is`, description: reason });
  }
  if (result.localStackWarning) {
    toaster.warning({ title: "gh stack doesn't track the stack", description: result.localStackWarning });
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
  } catch (cause) {
    toastFailure(cause);
  }
}
