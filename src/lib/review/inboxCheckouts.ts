// The inbox actions that put a PR's code on disk: Fix CI in thread, Create
// worktrees and Open in worktree (`InboxHandlers`). Each goes up only to
// the step its action needs, through `stepUpFor`.

import { newThreadInDir, openThreadInCwd } from "$lib/app/navigation.svelte";
import type { InboxHandlers } from "$lib/inbox/handlers";
import { inboxes } from "$lib/inbox/inboxStore.svelte";
import { stepUpFor } from "$lib/review/checkoutSteps";
import { startPrThread } from "$lib/review/prThreads";
import { commands } from "$lib/services/homeRouting";
import { toastError, toaster } from "$lib/toaster";
import type { CheckoutTarget, InboxPr } from "$lib/types";
import { errorMessage } from "$lib/utils/errors";
import { basename } from "$lib/utils/path";

/** The checkout target for an inbox row. */
export function targetFor(repoDir: string, pr: InboxPr): CheckoutTarget {
  const viewer = inboxes[repoDir]?.inbox?.viewer;
  return { number: pr.number, headRef: pr.headRef, headSha: pr.headSha, mine: pr.author === viewer, stack: null };
}

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
): Required<Pick<InboxHandlers, "fixCi" | "createWorktrees" | "openInWorktree">> {
  return {
    async fixCi(repoDir, pr) {
      const target = targetFor(repoDir, pr);
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
        toastError(errorMessage(cause));
      }
    },
    async createWorktrees(repoDir, prs) {
      for (const pr of prs) {
        try {
          const target = targetFor(repoDir, pr);
          const current = await commands.reviewCheckoutState(repoDir, target);
          if (current.step === "worktree") continue;
          const checkout = await commands.reviewCreatePrWorktree(repoDir, target, null);
          toaster.success({ title: `Created worktree ${basename(checkout.path ?? "")} from ${pr.headRef}` });
        } catch (cause) {
          toastError(`#${pr.number}: ${errorMessage(cause)}`);
        }
      }
    },
    async openInWorktree(repoDir, pr, worktreePath) {
      if (worktreePath) {
        newThreadInDir(worktreePath);
        return;
      }
      const target = targetFor(repoDir, pr);
      const checkout = await stepUpFor({ repoDir, repoName: repoName(repoDir), target, action: "fixInThread" });
      if (checkout?.path) newThreadInDir(checkout.path);
    },
  };
}
