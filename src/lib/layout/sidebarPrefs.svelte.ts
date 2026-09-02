/** Frontend-only sidebar preferences, persisted in localStorage. These are
 * local runtime settings with no Codex equivalent, so they never touch
 * config.toml. */

const STORAGE_KEY = "pingex-hide-old-threads";

/** Threads not touched within this window count as "old". */
export const THREAD_AGE_LIMIT_SECONDS = 86400;

/** True when a thread's `updatedAt` (Unix seconds) is more than a day old.
 * Matches `relativeTime`: anything labelled "Today" is kept. */
export function isStale(updatedAt: number, now = Date.now() / 1000): boolean {
  return now - updatedAt > THREAD_AGE_LIMIT_SECONDS;
}

export function loadHideOldThreads(): boolean {
  try {
    const value = localStorage.getItem(STORAGE_KEY);
    return value === null ? true : value === "true";
  } catch {
    return true;
  }
}

function saveHideOldThreads(enabled: boolean): void {
  try {
    localStorage.setItem(STORAGE_KEY, String(enabled));
  } catch {
    // Best-effort; the choice still applies for the session.
  }
}

/** Reactive singleton so the sidebar and settings view stay in sync. */
class SidebarPrefsStore {
  hideOldThreads = $state(true);

  constructor() {
    this.hideOldThreads = loadHideOldThreads();
  }

  setHideOldThreads(enabled: boolean): void {
    this.hideOldThreads = enabled;
    saveHideOldThreads(enabled);
  }
}

export const sidebarPrefs = new SidebarPrefsStore();
