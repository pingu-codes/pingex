// The inbox's "Start agent review": the Start review dialog for one PR, and
// a review per PR, without asking, for a multi-select batch
// (`InboxHandlers.startAgentReviews`). An agent review works in the PR's
// worktree, so the PR is checked out first; the inbox offers it only then.

import { openDialog } from "$lib/app/dialogs.svelte";
import type { InboxHandlers } from "$lib/inbox/handlers";
import { defaultRequest, enqueueAgentReview, reviewTarget } from "$lib/review/agentReviews.svelte";
import { CHECK_OUT_FIRST } from "$lib/review/prOnDisk";
import StartReviewDialog from "$lib/review/StartReviewDialog.svelte";
import { commands } from "$lib/services/homeRouting";
import { toastError, toaster, toastFailure } from "$lib/toaster";
import type { InboxPr } from "$lib/types";
import { errorMessage } from "$lib/utils/errors";

export function agentReviewHandlers(
  openPr: (repoDir: string, pr: InboxPr) => void,
): Required<Pick<InboxHandlers, "startAgentReviews">> {
  return {
    async startAgentReviews(repoDir, prs) {
      if (prs.length !== 1) {
        let queued = 0;
        for (const pr of prs) {
          try {
            await enqueueAgentReview(repoDir, defaultRequest([reviewTarget(repoDir, pr)]));
            queued++;
          } catch (cause) {
            toastError(`#${pr.number}: ${errorMessage(cause)}`);
          }
        }
        if (queued > 0) toaster.success({ title: `Queued ${queued} agent review${queued === 1 ? "" : "s"}` });
        return;
      }
      const [pr] = prs;
      try {
        const checkout = await commands.reviewPrCheckout(repoDir, pr.number);
        if (!checkout) {
          toastError(`${CHECK_OUT_FIRST}: #${pr.number} isn't checked out`);
          return;
        }
        const result = await openDialog(StartReviewDialog, {
          pr,
          target: reviewTarget(repoDir, pr),
          worktreePath: checkout.path,
        });
        if (!result) return;
        if (result.kind === "open") {
          openPr(repoDir, pr);
          return;
        }
        await enqueueAgentReview(repoDir, result.request);
        toaster.success({ title: `Agent review queued on #${pr.number}` });
        openPr(repoDir, pr);
      } catch (cause) {
        toastFailure(cause);
      }
    },
  };
}
