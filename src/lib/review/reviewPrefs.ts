/** Review page preferences local to this app, persisted in localStorage. */

const REVIEW_COLLAPSED_KEY = "pingex-review-panel-collapsed";

/** Whether the Your review panel is folded to its strip. */
export function loadReviewCollapsed(): boolean {
  try {
    return localStorage.getItem(REVIEW_COLLAPSED_KEY) === "true";
  } catch {
    return false;
  }
}

export function saveReviewCollapsed(collapsed: boolean): void {
  try {
    localStorage.setItem(REVIEW_COLLAPSED_KEY, String(collapsed));
  } catch {
    // Best-effort; the choice still applies until the page remounts.
  }
}
