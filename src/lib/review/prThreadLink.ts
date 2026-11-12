// Filing a thread under its PR the moment it starts. Every PR thread starts
// in the PR's worktree, so the worktree says which PR it is: a PR worktree's
// own, or, in a Stack worktree, the PR whose git branch it is on. A thread
// whose link can't be written is deleted before its first turn, so no PR
// thread runs unfiled.

import { quietRefresh } from "$lib/app/appData.svelte";
import { rememberPrLink } from "$lib/review/prLinks.svelte";
import { commands } from "$lib/services/homeRouting";
import type { PrLink, WorktreeContext } from "$lib/types";
import { errorMessage } from "$lib/utils/errors";

/** The PR a worktree with `context` is for now, if any. */
export function prOfContext(context: WorktreeContext | null): { repoDir: string; number: number } | null {
  const purpose = context?.purpose;
  if (!purpose) return null;
  if (purpose.kind === "managedPr") return { repoDir: purpose.repoDir, number: purpose.number };
  const pr = context.position ? purpose.prs[context.position.position - 1] : undefined;
  return pr ? { repoDir: purpose.repoDir, number: pr.number } : null;
}

/** File the thread just started as `threadId` under PR `number`. When that
 *  fails the thread, which has no turn yet, is deleted and the failure
 *  thrown. */
export async function linkOrDelete(repoDir: string, threadId: string, number: number): Promise<PrLink> {
  try {
    const link = await commands.reviewLinkThread(repoDir, threadId, number);
    rememberPrLink(link);
    return link;
  } catch (cause) {
    await commands.deleteThread(threadId).catch(() => null);
    void quietRefresh();
    throw new Error(`The thread couldn't be filed under #${number}, so it was deleted: ${errorMessage(cause)}`);
  }
}

/** File a thread just started in `cwd` under the PR whose worktree `cwd` is,
 *  if it is one. Resolves the link, or `null` for any other directory. */
export async function linkThreadStartedIn(threadId: string, cwd: string): Promise<PrLink | null> {
  const context = await commands.reviewWorktreeContext(cwd).catch(() => null);
  const pr = prOfContext(context);
  return pr ? linkOrDelete(pr.repoDir, threadId, pr.number) : null;
}
