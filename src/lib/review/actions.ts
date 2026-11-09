// What each review desk action needs on disk. The table itself is the
// backend's (`src-tauri/src/review/requirements.rs`), exported into the
// bindings as `DESK_ACTION_STEPS`, so the frontend and backend cannot
// disagree. This module adds the words buttons and tooltips use.
//
// Adding an action: add it in Rust, run `bun run typegen`, then give it a
// label here (the type check fails until you do).

import { DESK_ACTION_STEPS } from "$lib/bindings";
import type { CheckoutStep, DeskAction } from "$lib/types";

/** The checkout steps, lowest first. */
export const CHECKOUT_STEPS: readonly CheckoutStep[] = ["noCheckout", "worktree"];

export const STEP_LABELS: Record<CheckoutStep, string> = {
  noCheckout: "No checkout",
  worktree: "Worktree",
};

/** Button text for each action. */
export const ACTION_LABELS: Record<DeskAction, string> = {
  read: "Read",
  comment: "Comment",
  submitReview: "Submit review",
  resolveThread: "Resolve thread",
  viewWholeFile: "View whole file",
  changesSince: "Review changes since",
  merge: "Merge",
  close: "Close pull request",
  retarget: "Retarget",
  markReady: "Mark ready",
  diffReview: "Diff review",
  askAgent: "Ask agent",
  agentReviewWithTests: "Agent review with tests",
  runLocally: "Run locally",
  openInEditor: "Open in editor",
  workOnPr: "Work on this PR",
  fixInThread: "Fix in thread",
  addressReview: "Address review",
  fixCi: "Fix CI in thread",
  push: "Push",
  restack: "Restack",
  resolveConflicts: "Resolve conflicts",
};

/** The lowest step `action` works at. */
export function stepFor(action: DeskAction): CheckoutStep {
  return DESK_ACTION_STEPS[action] ?? "noCheckout";
}

/** The step a PR at `current` must move up to before `action`, or `null`
 *  when it already has enough. Never more than the action needs. */
export function stepUp(current: CheckoutStep, action: DeskAction): CheckoutStep | null {
  const needed = stepFor(action);
  return CHECKOUT_STEPS.indexOf(needed) > CHECKOUT_STEPS.indexOf(current) ? needed : null;
}

/** The tooltip line for a button whose action needs more than the PR has
 *  ("Creates a worktree"), or `null`. */
export function stepHint(current: CheckoutStep, action: DeskAction): string | null {
  const step = stepUp(current, action);
  return step ? `Creates a ${STEP_LABELS[step].toLowerCase()}` : null;
}
