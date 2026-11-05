// Merge follow-ups shared by the Stack page and the sidebar: which ones the
// viewer put off with "Later" (kept per viewer in localStorage), and which
// worktrees hold a stack whose follow-up is still due (the sidebar's dot).

import { inboxes } from "$lib/inbox/inboxStore.svelte";
import { followUpWorktrees } from "$lib/stack/stack";
import { worktreeMap } from "$lib/worktrees/worktreeMap.svelte";

const LATER_KEY = "pingex.stack.followUpLater";

function load(): string[] {
  try {
    return JSON.parse(localStorage.getItem(LATER_KEY) ?? "[]") as string[];
  } catch {
    return [];
  }
}

export const followUps = $state<{ later: string[] }>({ later: load() });

/** Put the follow-up `key` (`followUpKey`) off until another PR merges. */
export function putOffFollowUp(key: string): void {
  if (followUps.later.includes(key)) return;
  followUps.later = [...followUps.later, key];
  try {
    localStorage.setItem(LATER_KEY, JSON.stringify(followUps.later));
  } catch {
    // A per-viewer convenience: fine to lose.
  }
}

/** Worktree paths whose stack has a merged PR to follow up. */
export function followUpPaths(): Set<string> {
  const later = new Set(followUps.later);
  const paths = new Set<string>();
  for (const [repo, state] of Object.entries(inboxes)) {
    if (!state.stacks) continue;
    for (const path of followUpWorktrees(state.stacks, worktreeMap.byRepo[repo] ?? [], later)) paths.add(path);
  }
  return paths;
}

/** Forget what was put off, for tests. */
export function resetFollowUps(): void {
  followUps.later = [];
}
