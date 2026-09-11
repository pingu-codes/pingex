/** Frontend-only sidebar search preferences, persisted in localStorage. Both
 * message-body scopes are off by default so a plain search stays a cheap
 * scan over titles, previews and project paths. */

const AGENT_KEY = "pingex-search-agent-messages";
const USER_KEY = "pingex-search-user-messages";

function loadFlag(key: string): boolean {
  try {
    return localStorage.getItem(key) === "true";
  } catch {
    return false;
  }
}

function saveFlag(key: string, enabled: boolean): void {
  try {
    localStorage.setItem(key, String(enabled));
  } catch {
    // Best-effort; the choice still applies for the session.
  }
}

export function loadSearchAgentMessages(): boolean {
  return loadFlag(AGENT_KEY);
}

export function loadSearchUserMessages(): boolean {
  return loadFlag(USER_KEY);
}

/** Reactive singleton so every search panel shares the same choice. */
class SearchPrefsStore {
  searchAgentMessages = $state(false);
  searchUserMessages = $state(false);

  constructor() {
    this.searchAgentMessages = loadSearchAgentMessages();
    this.searchUserMessages = loadSearchUserMessages();
  }

  setSearchAgentMessages(enabled: boolean): void {
    this.searchAgentMessages = enabled;
    saveFlag(AGENT_KEY, enabled);
  }

  setSearchUserMessages(enabled: boolean): void {
    this.searchUserMessages = enabled;
    saveFlag(USER_KEY, enabled);
  }
}

export const searchPrefs = new SearchPrefsStore();
