// Pure helpers for the Stack page: the stack as the backend's stack
// operations take it, reordering, the git branch menu (Context menus board,
// "Branch in the stack tree"), and the merge follow-up.
//
// Orders are bottom first, like GitHub's stack listing; the tree draws them
// top first.

import type { MenuEntry } from "$lib/components/actionMenu";
import type { Inbox, InboxPr, PrBaseInput, PrStack, StackInput, StackListing, WorktreeEntry } from "$lib/types";

/** What a git branch can't be named: blank, with a space, or like an option. */
export function validBranchName(name: string): boolean {
  return name.length > 0 && !/\s/.test(name) && !name.startsWith("-");
}

/** A stack's name: its bottom PR's git branch without an `owner/` prefix. */
export function stackName(stack: PrStack): string {
  const bottom = stack.prs[0]?.headRef ?? "";
  return bottom.split("/").pop() || `Stack ${stack.number}`;
}

/** The stack as the stack operations take it: GitHub's listing joined with
 *  each PR's inbox facts. */
export function stackInputFor(stack: PrStack, inbox: Inbox): StackInput {
  const byNumber = new Map(inbox.prs.map((pr) => [pr.number, pr]));
  const open = stack.prs.filter((entry) => entry.state === "open" && !entry.merged);
  const bottom = open.map((entry) => byNumber.get(entry.number)).find((pr) => pr?.baseRef === stack.baseRef);
  return {
    number: stack.number,
    name: stackName(stack),
    baseRef: stack.baseRef,
    baseSha: bottom?.baseSha ?? "",
    prs: stack.prs.map((entry) => ({
      number: entry.number,
      title: byNumber.get(entry.number)?.title ?? "",
      headRef: entry.headRef,
      headSha: byNumber.get(entry.number)?.headSha ?? entry.headSha,
      open: entry.state === "open",
      merged: entry.merged,
    })),
  };
}

/** The open, unmerged PR numbers, bottom first. */
export function openOrder(input: StackInput): number[] {
  return input.prs.filter((pr) => pr.open && !pr.merged).map((pr) => pr.number);
}

/** `order` with `number` one place up (toward the top) or down; `null` when
 *  it cannot move that way. */
export function moved(order: number[], number: number, direction: "up" | "down"): number[] | null {
  const index = order.indexOf(number);
  const target = direction === "up" ? index + 1 : index - 1;
  if (index < 0 || target < 0 || target >= order.length) return null;
  const next = [...order];
  [next[index], next[target]] = [next[target], next[index]];
  return next;
}

/** `order` after dropping `dragged` onto `onto` (it then builds on `onto`;
 *  `null` puts it on the base). `null` when nothing would change. */
export function dropped(order: number[], dragged: number, onto: number | null): number[] | null {
  if (dragged === onto || !order.includes(dragged)) return null;
  const rest = order.filter((number) => number !== dragged);
  const at = onto === null ? 0 : rest.indexOf(onto) + 1;
  if (onto !== null && at === 0) return null;
  const next = [...rest.slice(0, at), dragged, ...rest.slice(at)];
  return next.every((number, index) => number === order[index]) ? null : next;
}

/** The git branch menu for open PR `number` (Context menus board). */
export function branchMenu(
  input: StackInput,
  number: number,
  context: { worktreeName: string | null; canRestack: boolean },
): MenuEntry[] {
  const order = openOrder(input);
  const index = order.indexOf(number);
  const parent = index > 0 ? order[index - 1] : null;
  return [
    {
      id: "checkout",
      label: context.worktreeName ? `Check out in ${context.worktreeName}` : "Check out in a worktree",
      shortcut: "C",
    },
    { id: "restackFrom", label: "Restack from here", disabled: !context.canRestack },
    { id: "push", label: "Push this branch", shortcut: "P", disabled: !context.canRestack },
    { kind: "separator" },
    { id: "moveUp", label: "Move up", shortcut: "Alt+Up", disabled: index === order.length - 1 },
    { id: "moveDown", label: "Move down", shortcut: "Alt+Down", disabled: index === 0 },
    { id: "insertBelow", label: "Insert branch below" },
    { id: "split", label: "Split commits into new PR…" },
    {
      id: "fold",
      label: parent === null ? "Fold into parent" : `Fold into #${parent}`,
      disabled: parent === null,
      title: parent === null ? "The bottom PR has no parent PR" : undefined,
    },
    { id: "rename", label: "Rename branch…", shortcut: "F2" },
    { kind: "separator" },
    { id: "remove", label: "Remove from stack", danger: true },
  ];
}

export type MergeFollowUp =
  /** Some PRs merged and others are open: restack those and delete the
   *  merged git branches. */
  | { kind: "restack"; merged: number[]; rest: number[]; branches: string[] }
  /** Every PR merged: offer to remove the worktree and archive its threads. */
  | { kind: "cleanup" };

/** What to offer after PRs in the stack merged, given which merged git
 *  branches are still here. `dismissed` is the key `followUpKey` makes. */
export function mergeFollowUp(
  input: StackInput,
  mergedHere: number[],
  hasWorktree: boolean,
  dismissed: Set<string>,
): MergeFollowUp | null {
  if (dismissed.has(followUpKey(input))) return null;
  const rest = openOrder(input);
  const merged = input.prs.filter((pr) => pr.merged).map((pr) => pr.number);
  if (merged.length === 0) return null;
  if (rest.length === 0) return hasWorktree ? { kind: "cleanup" } : null;
  if (mergedHere.length === 0) return null;
  const branches = input.prs.filter((pr) => mergedHere.includes(pr.number)).map((pr) => pr.headRef);
  return { kind: "restack", merged: mergedHere, rest, branches };
}

/** The key a "Later" on the follow-up is remembered by: it comes back when
 *  another PR merges. */
export function followUpKey(stack: { number: number; prs: { number: number; merged: boolean }[] }): string {
  const merged = stack.prs.filter((pr) => pr.merged).map((pr) => pr.number);
  return `${stack.number}:${merged.join(",")}`;
}

/** Worktrees that hold a stack with a merged PR still to follow up: the
 *  sidebar's dot. */
export function followUpWorktrees(stacks: StackListing, worktrees: WorktreeEntry[], dismissed: Set<string>): string[] {
  if (!stacks.supported) return [];
  const paths: string[] = [];
  for (const stack of stacks.stacks) {
    if (!stack.prs.some((pr) => pr.merged) || dismissed.has(followUpKey(stack))) continue;
    const branches = new Set(stack.prs.map((pr) => pr.headRef));
    for (const worktree of worktrees) {
      if (worktree.branch && branches.has(worktree.branch) && !paths.includes(worktree.path)) paths.push(worktree.path);
    }
  }
  return paths;
}

/** "Behind main by 3", or `null` when not behind (or not known). */
export function behindLabel(baseRef: string, behind: number | null | undefined): string | null {
  if (!behind || behind <= 0) return null;
  return `Behind ${baseRef} by ${behind}`;
}

/** A PR as `reviewPrsBehind` counts it: GitHub's head against GitHub's base
 *  tip from the last sync. */
export function baseInput(pr: Pick<InboxPr, "number" | "headSha" | "baseRef" | "baseSha">): PrBaseInput {
  return { number: pr.number, headSha: pr.headSha, baseRef: pr.baseRef, baseSha: pr.baseSha };
}

/** The viewer's open PRs, the ones the inbox checks against their base. */
export function myBaseInputs(inbox: Inbox): PrBaseInput[] {
  return inbox.prs.filter((pr) => pr.author === inbox.viewer).map(baseInput);
}

/** The open PR whose git branch `pr` is based on, if it is one in `inbox`. */
export function prBelow(pr: Pick<InboxPr, "number" | "baseRef">, inbox: Inbox | null | undefined): InboxPr | null {
  if (!inbox) return null;
  return (
    inbox.prs.find(
      (other) =>
        other.number !== pr.number &&
        other.headRef === pr.baseRef &&
        (other.headRepo === null || other.headRepo === inbox.repo),
    ) ?? null
  );
}

/** "Behind main by 3", or "Behind #12" when the base is open PR #12's git
 *  branch: there a count misleads, since a rewritten #12 inflates it. */
export function driftLabel(
  pr: Pick<InboxPr, "number" | "baseRef">,
  behind: number | null | undefined,
  inbox: Inbox | null | undefined,
): string | null {
  if (!behind || behind <= 0) return null;
  const below = prBelow(pr, inbox);
  return below ? `Behind #${below.number}` : behindLabel(pr.baseRef, behind);
}

/** The first message of a "Resolve in a thread" thread. */
export function conflictPrompt(branch: string, ontoRef: string, files: string[]): string {
  return [
    `A restack of the stack onto ${ontoRef} stopped at a conflict while replaying ${branch} in this worktree.`,
    `Conflicted files:\n${files.map((file) => `- ${file}`).join("\n")}`,
    "Resolve the conflicts in these files, keeping both sides' intent, and remove every conflict marker. Don't run `git rebase --continue` or commit: Pingex continues the restack once the markers are gone.",
  ].join("\n\n");
}
