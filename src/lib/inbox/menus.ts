// The inbox's menus: a PR row's (Context menus board, "Pull request row")
// and the CI status badge's. Item ids are what `PullRequestsView` runs.

import type { MenuEntry } from "$lib/components/actionMenu";
import type { InboxPr, WorktreeEntry } from "$lib/types";

const LATER = "Not available yet";

export function prMenu(
  pr: InboxPr,
  context: { worktrees: WorktreeEntry[]; canOpenInWorktree: boolean; canStartAgentReview: boolean },
): MenuEntry[] {
  const worktrees: MenuEntry[] = context.worktrees.map((worktree) => ({
    id: `worktree:${worktree.path}`,
    label: worktree.isMain ? "Main checkout" : (worktree.path.split("/").pop() ?? worktree.path),
    hint: worktree.isMain && worktree.branch ? `on ${worktree.branch}` : undefined,
    checked: worktree.branch === pr.headRef,
  }));
  return [
    { id: "open", label: "Open", shortcut: "Enter" },
    {
      kind: "submenu",
      id: "openIn",
      label: "Open in worktree",
      disabled: !context.canOpenInWorktree,
      entries: [
        ...worktrees,
        ...(worktrees.length > 0 ? [{ kind: "separator" } as const] : []),
        { id: "worktree:new", label: `New worktree for #${pr.number}` },
      ],
    },
    { id: "newThread", label: "Work on this PR", shortcut: "T" },
    {
      id: "agentReview",
      label: "Start agent review",
      shortcut: "R",
      disabled: !context.canStartAgentReview,
      title: context.canStartAgentReview ? undefined : LATER,
    },
    { kind: "separator" },
    { id: "copyBranch", label: "Copy branch name", shortcut: "CmdOrCtrl+Shift+C" },
    { id: "copyLink", label: "Copy link" },
    { id: "openGitHub", label: "Open on GitHub", shortcut: "CmdOrCtrl+O" },
    { kind: "separator" },
    { id: "seen", label: "Mark as seen", shortcut: "E" },
    { id: "mute", label: "Mute until new commits" },
    { kind: "separator" },
    { id: "close", label: "Close pull request…", danger: true },
  ];
}

export function ciMenu(context: { canFixCi: boolean }): MenuEntry[] {
  return [
    {
      id: "fixCi",
      label: "Fix in a new thread",
      disabled: !context.canFixCi,
      title: context.canFixCi ? undefined : LATER,
    },
    { id: "rerunChecks", label: "Re-run failed checks" },
    { id: "viewLog", label: "View log" },
  ];
}
