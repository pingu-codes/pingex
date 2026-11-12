// The inbox actions that work on a PR's code: Check out (a row, a stack card,
// the bulk bar), Work on this PR and Fix CI in thread (`InboxHandlers`). Each
// goes through `prOnDisk.ts`; the ones that need the worktree are disabled in
// the inbox until the PR is checked out.

import { projects } from "$lib/app/appData.svelte";
import { newThreadInDir, openThreadInCwd } from "$lib/app/navigation.svelte";
import type { InboxHandlers } from "$lib/inbox/handlers";
import { checkOut, checkOutAll, startPrThread, switchToPr } from "$lib/review/prOnDisk";
import { commands } from "$lib/services/homeRouting";
import { toaster, toastFailure } from "$lib/toaster";
import type { InboxPr } from "$lib/types";

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

export function workHandlers(): Required<Pick<InboxHandlers, "fixCi" | "checkOut" | "newThread" | "checkOutStack">> {
  return {
    async newThread(repoDir, pr) {
      // A stack's worktree on another PR's branch switches first.
      const checkout = await switchToPr(repoDir, pr.number, projects);
      if (checkout) newThreadInDir(checkout.path);
    },
    async fixCi(repoDir, pr) {
      try {
        const logs = await commands.reviewCheckLogs(repoDir, pr.number).catch(() => []);
        const { thread, link } = await startPrThread(repoDir, pr.number, {
          prompt: [{ type: "text", text: fixCiPrompt(pr, logs) }],
          projects,
        });
        openThreadInCwd(thread.id, link.checkoutPath ?? repoDir);
      } catch (cause) {
        toastFailure(cause);
      }
    },
    async checkOut(repoDir, prs) {
      if (prs.length === 1) {
        await checkOut(repoDir, { pr: prs[0].number });
        return;
      }
      const done = await checkOutAll(
        repoDir,
        prs.map((pr) => pr.number),
      );
      if (done > 1) toaster.success({ title: `Checked out ${done} of ${prs.length}` });
    },
    async checkOutStack(repoDir, group) {
      await checkOut(repoDir, { stack: group.stack.number });
    },
  };
}
