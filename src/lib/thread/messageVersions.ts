/**
 * Message versions. Editing a user message forks the thread before that
 * message and sends the new text on the fork, so each version lives in its
 * own thread. A `ThreadBranch` row records each fork; this module turns those
 * rows into "‹ 2 / 3 ›" for a bubble and the thread to open for each arrow.
 *
 * Turn ids survive a fork verbatim, so a version group is keyed by the turn
 * id of the original message (`groupTurnId`) and each later version by the
 * fork's own edit turn. A bubble finds its place in a group by its own turn
 * id, wherever in the family it is shown.
 */
import type { ThreadBranch } from "$lib/types";

export interface MessageVersion {
  /** Thread holding this version. */
  threadId: string;
  /** The version's turn; unknown until the fork's first turn has an id. */
  turnId: string | null;
}

export interface MessageVersions {
  /** Zero-based position of the version on show. */
  index: number;
  count: number;
  /** Thread to open for each arrow. */
  prevThreadId: string | null;
  nextThreadId: string | null;
}

/** Every version group, oldest version first. */
export function versionGroups(branches: ThreadBranch[]): Map<string, MessageVersion[]> {
  const groups = new Map<string, MessageVersion[]>();
  for (const branch of sorted(branches)) {
    let group = groups.get(branch.groupTurnId);
    if (!group) {
      group = [{ threadId: branch.parentThreadId, turnId: branch.groupTurnId }];
      groups.set(branch.groupTurnId, group);
    }
    group.push({ threadId: branch.threadId, turnId: branch.editTurnId });
  }
  return groups;
}

/**
 * The versions of the message whose turn is `turnId`, or null when it has
 * only ever had one. `currentThreadId` breaks the tie for a fork whose edit
 * turn is not known yet: the version is then the one in the thread on show.
 */
export function versionsForTurn(
  turnId: string,
  branches: ThreadBranch[],
  currentThreadId: string | null,
): MessageVersions | null {
  for (const group of versionGroups(branches).values()) {
    let index = group.findIndex((version) => version.turnId === turnId);
    if (index === -1 && currentThreadId) {
      index = group.findIndex((version) => version.turnId === null && version.threadId === currentThreadId);
    }
    if (index === -1) continue;
    return {
      index,
      count: group.length,
      prevThreadId: index > 0 ? group[index - 1].threadId : null,
      nextThreadId: index < group.length - 1 ? group[index + 1].threadId : null,
    };
  }
  return null;
}

/** Whether `turnId` is the pending edit turn of the branch `threadId`. */
export function isPendingEditTurn(threadId: string | null, turnIndex: number, branches: ThreadBranch[]): boolean {
  const branch = branches.find((candidate) => candidate.threadId === threadId);
  return !!branch && branch.editTurnId === null && branch.inheritedTurns === turnIndex;
}

/** The group a new edit of `turnId` joins: the original's when `turnId` is
 *  itself an edit, otherwise `turnId` starts one. */
export function groupForTurn(turnId: string, branches: ThreadBranch[]): string {
  return branches.find((branch) => branch.editTurnId === turnId)?.groupTurnId ?? turnId;
}

/** The thread at the top of `threadId`'s family — the one in the sidebar. */
export function rootThreadId(threadId: string | null, branches: ThreadBranch[]): string | null {
  let current = threadId;
  const seen = new Set<string>();
  while (current && !seen.has(current)) {
    seen.add(current);
    const branch = branches.find((candidate) => candidate.threadId === current);
    if (!branch) break;
    current = branch.parentThreadId;
  }
  return current;
}

/**
 * The most recently active thread in the subtree under `threadId`, itself
 * included; ties go to the thread nearest the top. Branch rows carry their
 * own activity; `activityOf` supplies it for the root, which has no row.
 */
export function newestLeaf(
  threadId: string,
  branches: ThreadBranch[],
  activityOf: (threadId: string) => number | null | undefined = () => null,
): string {
  let best = threadId;
  let bestAt = Number.NEGATIVE_INFINITY;
  const frontier = [threadId];
  const seen = new Set<string>();
  while (frontier.length) {
    const current = frontier.shift()!;
    if (seen.has(current)) continue;
    seen.add(current);
    const branch = branches.find((candidate) => candidate.threadId === current);
    const at = branch?.updatedAt ?? activityOf(current) ?? Number.NEGATIVE_INFINITY;
    if (at > bestAt) {
      best = current;
      bestAt = at;
    }
    for (const child of sorted(branches)) {
      if (child.parentThreadId === current) frontier.push(child.threadId);
    }
  }
  return best;
}

/** Whether `threadId` is a version branch rather than a sidebar thread. */
export function isBranch(threadId: string | null, branches: ThreadBranch[]): boolean {
  return branches.some((branch) => branch.threadId === threadId);
}

function sorted(branches: ThreadBranch[]): ThreadBranch[] {
  return [...branches].sort((a, b) => a.createdAt - b.createdAt || a.threadId.localeCompare(b.threadId));
}
