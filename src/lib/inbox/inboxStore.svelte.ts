// Every repository's last synced inbox, shared by the Pull requests view and
// the sidebar's "N need you" badges, and the sync cadence that keeps them
// fresh: on window focus, on the Sync button, after Pingex's own changes,
// and on a slow timer while the window stays focused (`syncDelay`). Nothing
// here polls git.

import { syncDelay } from "$lib/inbox/inbox";
import { commands } from "$lib/services/homeRouting";
import type { Inbox, StackListing } from "$lib/types";
import { errorMessage } from "$lib/utils/errors";

export type RepoInbox = {
  inbox: Inbox | null;
  stacks: StackListing | null;
  error: string | null;
  syncing: boolean;
};

export const inboxes = $state<Record<string, RepoInbox>>({});

const NO_STACKS: StackListing = { supported: false, stacks: [] };

function entry(repoDir: string): RepoInbox {
  inboxes[repoDir] ??= { inbox: null, stacks: null, error: null, syncing: false };
  return inboxes[repoDir];
}

/** Sync one repository's inbox and its stacks. */
export async function syncInbox(repoDir: string): Promise<void> {
  const state = entry(repoDir);
  if (state.syncing) return;
  state.syncing = true;
  try {
    const [inbox, stacks] = await Promise.all([
      commands.reviewInbox(repoDir),
      commands.reviewStacks(repoDir).catch(() => NO_STACKS),
    ]);
    state.inbox = inbox;
    state.stacks = stacks;
    state.error = null;
  } catch (cause) {
    state.error = errorMessage(cause);
  } finally {
    state.syncing = false;
  }
}

/** The "N need you" count per repository folder, from the last sync. */
export function needsYouByRepo(): Record<string, number> {
  return Object.fromEntries(
    Object.entries(inboxes).flatMap(([repoDir, state]) => (state.inbox ? [[repoDir, state.inbox.counts.needsMe]] : [])),
  );
}

/** Mark the activity on `numbers` seen, at once in the view and then in the
 *  database. */
export async function markActivitySeen(repoDir: string, numbers: number[]): Promise<void> {
  if (numbers.length === 0) return;
  const inbox = inboxes[repoDir]?.inbox;
  if (inbox) {
    inbox.feed = inbox.feed.filter((entry) => !numbers.includes(entry.number));
    for (const triage of inbox.triage) if (numbers.includes(triage.number)) triage.newActivity = 0;
  }
  await commands.reviewMarkActivitySeen(repoDir, numbers);
}

export async function mutePr(repoDir: string, number: number, headSha: string): Promise<void> {
  await commands.reviewMutePr(repoDir, number, headSha);
  await syncInbox(repoDir);
}

/**
 * Keep `repoDirs()` synced while the app runs: now, whenever the window
 * gains focus, and on the timer while it stays focused. A repository whose
 * last sync failed (no GitHub remote, signed out) waits for the next focus.
 * Returns the stop function.
 */
export function startInboxSync(repoDirs: () => string[]): () => void {
  let timer: ReturnType<typeof setTimeout> | null = null;
  const lowestRemaining = () => {
    const left = Object.values(inboxes).flatMap((state) =>
      state.inbox?.rateLimit ? [state.inbox.rateLimit.remaining] : [],
    );
    return left.length > 0 ? Math.min(...left) : null;
  };
  const schedule = () => {
    if (timer) clearTimeout(timer);
    timer = null;
    if (!document.hasFocus()) return;
    timer = setTimeout(() => void round(false), syncDelay(lowestRemaining()));
  };
  const round = async (all: boolean) => {
    const dirs = repoDirs().filter((dir) => all || !inboxes[dir]?.error);
    await Promise.all(dirs.map(syncInbox));
    schedule();
  };
  const onFocus = () => void round(true);
  const onBlur = () => {
    if (timer) clearTimeout(timer);
    timer = null;
  };
  window.addEventListener("focus", onFocus);
  window.addEventListener("blur", onBlur);
  void round(true);
  return () => {
    onBlur();
    window.removeEventListener("focus", onFocus);
    window.removeEventListener("blur", onBlur);
  };
}
