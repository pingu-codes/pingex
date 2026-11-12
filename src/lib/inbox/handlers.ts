// What the inbox asks the app to do. The view runs the actions that need
// no other screen itself (merge, close, re-run checks, copy, mark seen,
// mute); everything that opens another screen or starts work comes in
// here.

import type { StackGroup } from "$lib/inbox/inbox";
import type { InboxPr } from "$lib/types";

export type InboxHandlers = {
  /** Open the PR for review; `changesSince` asks for only what changed
   *  since my last review. */
  openPr: (repoDir: string, pr: InboxPr, options: { changesSince?: boolean }) => void;
  /** Work on this PR: a draft thread in its worktree. Needs it checked out. */
  newThread: (repoDir: string, pr: InboxPr) => void;
  /** Queue an agent review per PR (the Start review dialog for one). */
  startAgentReviews: (repoDir: string, prs: InboxPr[]) => void;
  /** Check each PR out (a stack's PR checks out its stack). */
  checkOut: (repoDir: string, prs: InboxPr[]) => void;
  /** Start a thread in the PR's worktree seeded with the failing checks. */
  fixCi: (repoDir: string, pr: InboxPr) => void;
  /** The Stack page for a stack card. */
  openStack: (repoDir: string, group: StackGroup) => void;
  restack: (repoDir: string, group: StackGroup) => void;
  /** Check the stack out: one worktree, a local git branch per PR. */
  checkOutStack: (repoDir: string, group: StackGroup) => void;
};
