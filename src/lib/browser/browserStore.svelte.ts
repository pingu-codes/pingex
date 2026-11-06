// Browser use: what the Browser panel and the approval cards read.
//
// The browser is app-wide (one connection to the user's Chrome), so its
// events are not tagged with a Home. Threads are keyed by the native Codex
// thread id, which is what the backend sees in `item/tool/call`; frontend
// thread ids may be Profile-scoped, so every lookup goes through `nativeId`.

import { SvelteMap } from "svelte/reactivity";
import { type BrowserApproval, type BrowserDecision, type BrowserOverview, events } from "$lib/bindings";
import { commands, identity } from "$lib/services/homeRouting";
import { isTauri } from "$lib/services/tauri";

export function nativeThreadId(threadId: string): string {
  return identity(threadId)?.nativeId ?? threadId;
}

export const browser = $state<{
  overview: BrowserOverview | null;
  approvals: BrowserApproval[];
}>({ overview: null, approvals: [] });

/** Each thread's tabs and grants, refreshed when the backend says so. */
export const threadViews = new SvelteMap<string, NonNullable<BrowserOverview["thread"]>>();

export async function refreshBrowser(threadId?: string | null): Promise<void> {
  const native = threadId ? nativeThreadId(threadId) : null;
  const overview = await commands.browserOverview(native);
  browser.overview = overview;
  browser.approvals = overview.approvals;
  if (native && overview.thread) threadViews.set(native, overview.thread);
}

export function approvalsFor(threadId: string): BrowserApproval[] {
  const native = nativeThreadId(threadId);
  return browser.approvals.filter((approval) => approval.threadId === native);
}

export async function resolveBrowserApproval(id: string, decision: BrowserDecision): Promise<void> {
  browser.approvals = browser.approvals.filter((approval) => approval.id !== id);
  await commands.browserResolveApproval(id, decision);
}

const labels = new Map<string, string>();

/** Title the thread's tab group in Chrome; sent only when the name changes. */
export function labelBrowserThread(threadId: string, name: string): void {
  if (!isTauri()) return;
  const native = nativeThreadId(threadId);
  if (labels.get(native) === name) return;
  labels.set(native, name);
  void commands.browserLabelThread(native, name).catch(() => labels.delete(native));
}

let started = false;

export async function startBrowserListeners(): Promise<void> {
  if (started || !isTauri()) return;
  started = true;
  await events.browserApprovalRequested.listen(({ payload }) => {
    const { approval } = payload;
    if (!browser.approvals.some((existing) => existing.id === approval.id)) {
      browser.approvals = [...browser.approvals, approval];
    }
  });
  await events.browserApprovalResolved.listen(({ payload }) => {
    browser.approvals = browser.approvals.filter((approval) => approval.id !== payload.id);
  });
  await events.browserStatusChanged.listen(({ payload }) => {
    if (browser.overview) browser.overview = { ...browser.overview, status: payload.status };
    else void refreshBrowser();
  });
  await events.browserTabsChanged.listen(({ payload }) => {
    void commands.browserOverview(payload.threadId).then((overview) => {
      browser.overview = overview;
      if (overview.thread) threadViews.set(payload.threadId, overview.thread);
    });
  });
  // Pick up questions asked before this window (re)loaded.
  await refreshBrowser();
}
