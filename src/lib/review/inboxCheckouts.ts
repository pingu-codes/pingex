// The inbox actions that put a PR's code on disk: Fix CI in thread, Create
// worktrees and Open in worktree (`InboxHandlers`). Each goes up only to
// the step its action needs, through `stepUpFor`.

import { newThreadInDir, openThreadInCwd } from "$lib/app/navigation.svelte";
import type { InboxHandlers } from "$lib/inbox/handlers";
import { inboxes } from "$lib/inbox/inboxStore.svelte";
import { checkOutStack, stepUpFor, workOnPr } from "$lib/review/checkoutSteps";
import { checkoutTarget } from "$lib/review/checkoutTarget";
import { expectPrThread } from "$lib/review/prLinks.svelte";
import { inboxPrTarget, stackOfPr } from "$lib/review/prTarget";
import { startPrThread } from "$lib/review/prThreads";
import { commands } from "$lib/services/homeRouting";
import { stackInputFor } from "$lib/stack/stack";
import { toastError, toaster, toastFailure } from "$lib/toaster";
import type { InboxPr } from "$lib/types";
import { errorMessage } from "$lib/utils/errors";
import { basename } from "$lib/utils/path";

/** The first message of a "Fix CI in thread" thread: the failing checks' logs. */
export function fixCiPrompt(pr: InboxPr, logs: { name: string; log: string; truncated: boolean }[]): string {
  const sections = logs.map(
    (check) => `### ${check.name}${check.truncated ? " (log cut short)" : ""}\n\n\`\`\`\n${check.log.trim()}\n\`\`\``,
  );
  return [
    `CI is failing on #${pr.number} "${pr.title}" (${pr.headRef}). Find the cause and fix it on this branch.`,
    sections.length > 0 ? sections.join("\n\n") : "No failing check logs could be read; run the checks locally.",
  ].join("\n\n");
}

export function checkoutHandlers(
  repoName: (repoDir: string) => string,
): Required<Pick<InboxHandlers, "fixCi" | "createWorktrees" | "openInWorktree" | "newThread" | "checkOutStack">> {
  return {
    async newThread(repoDir, pr) {
      await workOnPr({ repoDir, repoName: repoName(repoDir), target: inboxPrTarget(repoDir, pr), action: "workOnPr" });
    },
    async fixCi(repoDir, pr) {
      const target = inboxPrTarget(repoDir, pr);
      const checkout = await stepUpFor({ repoDir, repoName: repoName(repoDir), target, action: "fixCi" });
      if (!checkout?.path) return;
      try {
        const logs = await commands.reviewCheckLogs(repoDir, pr.number).catch(() => []);
        const { thread } = await startPrThread({
          repoDir,
          target,
          input: [{ type: "text", text: fixCiPrompt(pr, logs) }],
        });
        openThreadInCwd(thread.id, checkout.path);
      } catch (cause) {
        toastFailure(cause);
      }
    },
    async createWorktrees(repoDir, prs) {
      for (const pr of prs) {
        try {
          const target = inboxPrTarget(repoDir, pr);
          const current = await commands.reviewCheckoutState(repoDir, target);
          if (current.step === "worktree") continue;
          const checkout = await commands.reviewEnsurePrWorktree(repoDir, target);
          toaster.success({ title: `Created worktree ${basename(checkout.path ?? "")} for #${pr.number}` });
          if (checkout.setupPending) await commands.reviewRunCheckoutSetup(repoDir, pr.number);
        } catch (cause) {
          toastError(`#${pr.number}: ${errorMessage(cause)}`);
        }
      }
    },
    async checkOutStack(repoDir, group) {
      const inbox = inboxes[repoDir]?.inbox;
      if (!inbox) return;
      const input = stackInputFor(group.stack, inbox);
      const top = input.prs.filter((pr) => pr.open && !pr.merged).at(-1);
      if (!top) return;
      const stack = stackOfPr(inboxes[repoDir]?.stacks, top.number);
      const mine = group.author === inbox.viewer;
      await checkOutStack({
        repoDir,
        repoName: repoName(repoDir),
        input,
        target: checkoutTarget(top, mine, stack),
      });
    },
    async openInWorktree(repoDir, pr, worktreePath) {
      const target = inboxPrTarget(repoDir, pr);
      if (worktreePath) {
        expectPrThread(worktreePath, repoDir, target);
        newThreadInDir(worktreePath);
        return;
      }
      await workOnPr({ repoDir, repoName: repoName(repoDir), target, action: "workOnPr" });
    },
  };
}
