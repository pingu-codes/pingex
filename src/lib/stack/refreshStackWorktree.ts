// Bringing a Stack worktree up to date with GitHub as a draft opens there, so
// the Worktree context the agent gets lists the stack as it is now, every
// open PR has its local git branch, and `gh stack` tracks the stack.
//
// A worktree can be labelled a Stack worktree without "Check out stack"
// having run: a Stack page read finds any worktree on one of its branches,
// such as one "Work on PR" made for a single PR. This catches it up.

import { inboxes, syncInbox } from "$lib/inbox/inboxStore.svelte";
import { commands } from "$lib/services/homeRouting";
import { stackInputFor } from "$lib/stack/stack";

/** Read the stack again and check it out where it already is: what is missing
 *  is made, nothing is moved. False when the stack is gone or has no open
 *  PRs, and when the checkout fails (a stopped restack refuses it). */
export async function refreshStackWorktree(repoDir: string, number: number): Promise<boolean> {
  try {
    await syncInbox(repoDir);
    const repo = inboxes[repoDir];
    const stack = repo?.stacks?.stacks.find((candidate) => candidate.number === number && candidate.open);
    if (!repo?.inbox || !stack) return false;
    const input = stackInputFor(stack, repo.inbox);
    if (!input.prs.some((pr) => pr.open && !pr.merged)) return false;
    await commands.reviewEnsureStackWorktree(repoDir, input);
    return true;
  } catch {
    return false;
  }
}
