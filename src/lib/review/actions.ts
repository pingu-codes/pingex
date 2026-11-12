// The words buttons and tooltips use for review desk actions that need no
// worktree, and the hint on those that do (`CHECK_OUT_FIRST` in `prOnDisk.ts`).

/** Button text for the actions the inbox row offers by name. */
export const ACTION_LABELS = {
  merge: "Merge",
  fixCi: "Fix CI in thread",
  changesSince: "Review changes since",
  workOnPr: "Work on this PR",
  checkOut: "Check out",
} as const;
