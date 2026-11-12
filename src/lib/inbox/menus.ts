// The inbox's menus: a PR row's (Context menus board, "Pull request row")
// and the CI status badge's. Item ids are what `PullRequestsView` runs. What
// works in the PR's worktree is disabled until the PR is checked out.

import type { MenuEntry } from "$lib/components/actionMenu";
import { CHECK_OUT_FIRST } from "$lib/review/prOnDisk";
import type { InboxPr, PrCheckout } from "$lib/types";
import { basename } from "$lib/utils/path";

/** A PR row's menu. `checkout` is where the PR is checked out, `null` when it
 *  isn't, and `undefined` until read; what needs the worktree waits for it. */
export function prMenu(pr: InboxPr, context: { checkout: PrCheckout | null | undefined }): MenuEntry[] {
  const { checkout } = context;
  const needsCheckout = checkout ? undefined : CHECK_OUT_FIRST;
  return [
    { id: "open", label: "Open", shortcut: "Enter" },
    checkout
      ? { id: "checkOut", label: "Checked out", hint: basename(checkout.path), disabled: true }
      : { id: "checkOut", label: `Check out #${pr.number}`, disabled: checkout === undefined },
    { id: "newThread", label: "Work on this PR", shortcut: "T", disabled: !checkout, title: needsCheckout },
    { id: "agentReview", label: "Start agent review", shortcut: "R", disabled: !checkout, title: needsCheckout },
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

export function ciMenu(checkedOut: boolean): MenuEntry[] {
  return [
    {
      id: "fixCi",
      label: "Fix in a new thread",
      disabled: !checkedOut,
      title: checkedOut ? undefined : CHECK_OUT_FIRST,
    },
    { id: "rerunChecks", label: "Re-run failed checks" },
    { id: "viewLog", label: "View log" },
  ];
}
