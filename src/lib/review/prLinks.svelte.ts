// The PR link on every thread in this Home, by thread id: which PR a thread
// is about and the worktree it started in. Links are made as a thread starts
// (`prThreadLink.ts`). The sidebar files threads from it (`reviewFiling.ts`);
// the thread header's git-branch chip reads the start from it.

import { commands } from "$lib/services/homeRouting";
import type { PrLink } from "$lib/types";

export const prLinks = $state<{ byThread: Record<string, PrLink> }>({ byThread: {} });

let loading: Promise<void> | null = null;

/** Read every link once; later calls wait on the same read. */
export function ensurePrLinks(): Promise<void> {
  loading ??= reloadPrLinks();
  return loading;
}

export async function reloadPrLinks(): Promise<void> {
  try {
    const links = (await commands.reviewPrLinks()) ?? [];
    prLinks.byThread = Object.fromEntries(links.map((link) => [link.threadId, link]));
  } catch {
    // The sidebar files by cwd alone until links can be read.
  }
}

/** Keep a link the backend just recorded. */
export function rememberPrLink(link: PrLink): void {
  prLinks.byThread[link.threadId] = link;
}

/** File a thread under open PR `number` instead, or under none. Only the PR
 *  changes; the recorded start stays. */
export async function moveThreadToPr(threadId: string, number: number | null): Promise<void> {
  await commands.reviewMoveThreadToPr(threadId, number);
  await reloadPrLinks();
}

/** Forget what was read, for tests and Home switches. */
export function resetPrLinks(): void {
  prLinks.byThread = {};
  loading = null;
}
