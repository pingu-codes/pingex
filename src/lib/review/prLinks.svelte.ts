// The PR link on every thread in this Home, by thread id: which PR a thread
// is about and what was checked out when it started. The sidebar files
// threads from it (`reviewFiling.ts`); the thread header's git-branch chip
// reads the start from it.

import { commands } from "$lib/services/homeRouting";
import type { CheckoutTarget, PrLink } from "$lib/types";

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

/** Drafts opened in a PR's worktree, by that directory: the PR their thread
 *  is filed under once it starts. */
const pendingLinks = new Map<string, { repoDir: string; target: CheckoutTarget }>();

/** The next thread started in `cwd` is about `target`. */
export function expectPrThread(cwd: string, repoDir: string, target: CheckoutTarget): void {
  pendingLinks.set(cwd, { repoDir, target });
}

/** File a thread that just started in `cwd`, if a PR expected it. A link that
 *  can't be written leaves the thread running, unfiled. */
export async function linkExpectedThread(threadId: string, cwd: string): Promise<void> {
  const pending = pendingLinks.get(cwd);
  if (!pending) return;
  pendingLinks.delete(cwd);
  try {
    rememberPrLink(await commands.reviewLinkThread(pending.repoDir, threadId, pending.target));
  } catch {
    // The thread itself is fine; only its place in the sidebar is lost.
  }
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
  pendingLinks.clear();
  loading = null;
}
