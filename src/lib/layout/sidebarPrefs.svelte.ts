/** Frontend-only sidebar preferences, persisted in localStorage. These are
 * local runtime settings with no Codex equivalent, so they never touch
 * config.toml. */

const STORAGE_KEY = "pingex-hide-old-threads";
const NEST_WORKTREES_KEY = "pingex-nest-worktrees";

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

export function loadNestWorktrees(): boolean {
  try {
    return localStorage.getItem(NEST_WORKTREES_KEY) === "true";
  } catch {
    return false;
  }
}

function saveNestWorktrees(enabled: boolean): void {
  try {
    localStorage.setItem(NEST_WORKTREES_KEY, String(enabled));
  } catch {
    // Best-effort; the choice still applies for the session.
  }
}

/** Reactive singleton so the sidebar and settings view stay in sync. */
class SidebarPrefsStore {
  hideOldThreads = $state(true);
  /** Show every worktree as a child of its repository's row. */
  nestWorktrees = $state(false);

  constructor() {
    this.hideOldThreads = loadHideOldThreads();
    this.nestWorktrees = loadNestWorktrees();
  }

  setNestWorktrees(enabled: boolean): void {
    this.nestWorktrees = enabled;
    saveNestWorktrees(enabled);
  }

  setHideOldThreads(enabled: boolean): void {
    this.hideOldThreads = enabled;
    saveHideOldThreads(enabled);
  }
}

export const sidebarPrefs = new SidebarPrefsStore();
