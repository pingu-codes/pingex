// What the inbox asks the app to do. The view runs the actions that need
// no other screen itself (merge, close, re-run checks, copy, mark seen,
// mute); everything that opens another screen or starts work comes in
// here. Optional handlers belong to later slices: without one, its buttons
// and menu items show disabled.

import type { StackGroup } from "$lib/inbox/inbox";
import type { InboxPr } from "$lib/types";

export type InboxHandlers = {
  /** Open the PR for review; `changesSince` asks for only what changed
   *  since my last review. */
  openPr: (repoDir: string, pr: InboxPr, options: { changesSince?: boolean }) => void;
  /** A new thread about the PR. */
  newThread: (repoDir: string, pr: InboxPr) => void;
  /** Queue an agent review per PR (the Start review dialog for one). */
  startAgentReviews?: (repoDir: string, prs: InboxPr[]) => void;
  /** A worktree per PR on its git branch. */
  createWorktrees?: (repoDir: string, prs: InboxPr[]) => void;
  /** Open the PR in an existing worktree, or in a new one (`null`). */
  openInWorktree?: (repoDir: string, pr: InboxPr, worktreePath: string | null) => void;
  /** Start a thread at the worktree step seeded with the failing checks. */
  fixCi?: (repoDir: string, pr: InboxPr) => void;
  /** The Stack page for a stack card. */
  openStack?: (repoDir: string, group: StackGroup) => void;
  restack?: (repoDir: string, group: StackGroup) => void;
};
