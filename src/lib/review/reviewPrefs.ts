/** Review page preferences local to this app, persisted in localStorage. */

const REVIEW_COLLAPSED_KEY = "pingex-review-panel-collapsed";
const STACK_TREE_COLLAPSED_KEY = "pingex-stack-tree-collapsed";

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

/** Whether the Stack page's tree is folded to its strip. */
export function loadStackTreeCollapsed(): boolean {
  try {
    return localStorage.getItem(STACK_TREE_COLLAPSED_KEY) === "true";
  } catch {
    return false;
  }
}

export function saveStackTreeCollapsed(collapsed: boolean): void {
  try {
    localStorage.setItem(STACK_TREE_COLLAPSED_KEY, String(collapsed));
  } catch {
    // Best-effort; the choice still applies until the page remounts.
  }
}
