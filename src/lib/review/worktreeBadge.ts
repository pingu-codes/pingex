// What a PR's worktree badge says: its state and the lines of its tooltip, from
// where the backend found the PR (`PrCheckout`).

import type { PrCheckout } from "$lib/types";
import { basename } from "$lib/utils/path";

/** On the PR's own branch, in a worktree on another one, or not on disk. */
export type BadgeState = "here" | "elsewhere" | "none";

export type WorktreeBadgeFacts = {
  state: BadgeState;
  /** The worktree's folder name, or what stands in for one. */
  name: string;
  path: string | null;
  /** The tooltip's lines under the name and path. */
  lines: string[];
  /** The worktree has something the PR doesn't: worth a mark on the row. */
  holdsWork: boolean;
  /** The whole thing as one sentence, for the accessibility tree. */
  label: string;
};

const NO_WORKTREE = "No local worktree";

function plural(count: number, noun: string): string {
  return `${count} ${noun}${count === 1 ? "" : "s"}`;
}

export function worktreeBadge(checkout: PrCheckout | null, noneHint: string): WorktreeBadgeFacts {
  if (!checkout?.path) {
    return {
      state: "none",
      name: NO_WORKTREE,
      path: null,
      lines: [noneHint],
      holdsWork: false,
      label: `${NO_WORKTREE}. ${noneHint}`,
    };
  }
  const { path, gitBranch, onPrBranch, dirty, ahead, behind, managed, mainCheckout } = checkout;
  const name = mainCheckout ? "Main checkout" : basename(path);
  const lines: string[] = [];
  if (onPrBranch) lines.push(gitBranch ? `On ${gitBranch}` : "Detached HEAD");
  else lines.push(`Now on ${gitBranch ?? "a detached HEAD"}, not this PR's branch`);
  if (dirty) lines.push("Uncommitted changes");
  if (ahead > 0) lines.push(`${plural(ahead, "local commit")} not on the PR`);
  if (behind > 0) lines.push(`Behind the PR by ${plural(behind, "commit")}`);
  if (managed) lines.push("Pingex removes this once the PR closes, if it holds no work");
  return {
    state: onPrBranch ? "here" : "elsewhere",
    name,
    path,
    lines,
    // Uncommitted changes on another branch belong to that branch's PR.
    holdsWork: ahead > 0 || (dirty && onPrBranch),
    label: `Worktree ${name} at ${path}. ${lines.join(". ")}`,
  };
}
