// The inbox's "Start agent review": the Start review dialog for one PR, and
// a diff review per PR, without asking, for a multi-select batch
// (`InboxHandlers.startAgentReviews`). The dialog's choice of code step is
// the confirmation, so a review checkout is made without asking again.

import { openDialog } from "$lib/app/dialogs.svelte";
import type { InboxHandlers } from "$lib/inbox/handlers";
import { inboxes } from "$lib/inbox/inboxStore.svelte";
import { defaultRequest, enqueueAgentReview, reviewTarget } from "$lib/review/agentReviews.svelte";
import { createReviewCheckout, stepUpFor } from "$lib/review/checkoutSteps";
import StartReviewDialog from "$lib/review/StartReviewDialog.svelte";
import { commands } from "$lib/services/homeRouting";
import { toastError, toaster } from "$lib/toaster";
import type { InboxPr } from "$lib/types";
import { errorMessage } from "$lib/utils/errors";

export function agentReviewHandlers(
  repoName: (repoDir: string) => string,
  openPr: (repoDir: string, pr: InboxPr) => void,
): Required<Pick<InboxHandlers, "startAgentReviews">> {
  return {
    async startAgentReviews(repoDir, prs) {
      const viewer = inboxes[repoDir]?.inbox?.viewer ?? null;
      if (prs.length !== 1) {
        let queued = 0;
        for (const pr of prs) {
          try {
            await enqueueAgentReview(repoDir, defaultRequest([reviewTarget(pr, viewer)]));
            queued++;
          } catch (cause) {
            toastError(`#${pr.number}: ${errorMessage(cause)}`);
          }
        }
        if (queued > 0) toaster.success({ title: `Queued ${queued} agent review${queued === 1 ? "" : "s"}` });
        return;
      }
      const [pr] = prs;
      const target = reviewTarget(pr, viewer);
      try {
        const [plan, settings] = await Promise.all([
          commands.reviewPlanStep(repoDir, target, "agentReviewWithTests").catch(() => null),
          commands.reviewRepositorySettings(repoDir).catch(() => null),
        ]);
        const result = await openDialog(StartReviewDialog, {
          pr,
          viewer,
          reviewCheckoutPath: plan?.current.path ?? plan?.reviewCheckoutPath ?? "",
          setupCommand: settings?.setupCommand ?? null,
          refusedCheckout: plan?.refused ?? null,
        });
        if (!result) return;
        if (result.kind === "open") {
          openPr(repoDir, pr);
          return;
        }
        const { request } = result;
        if (request.step === "reviewCheckout" && plan?.current.step === "noCheckout") {
          await createReviewCheckout(repoDir, target);
        } else if (request.step === "worktree") {
          const checkout = await stepUpFor({ repoDir, repoName: repoName(repoDir), target, action: "addressReview" });
          if (!checkout) return;
        }
        await enqueueAgentReview(repoDir, request);
        toaster.success({ title: `Agent review queued on #${pr.number}` });
        openPr(repoDir, pr);
      } catch (cause) {
        toastError(errorMessage(cause));
      }
    },
  };
}
