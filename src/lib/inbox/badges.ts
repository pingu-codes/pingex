import type { BadgeTone } from "$lib/inbox/inbox";

/** The Skeleton preset behind each badge tone. */
export const TONE_CLASS: Record<BadgeTone, string> = {
  success: "preset-tonal-success",
  warning: "preset-tonal-warning",
  error: "preset-tonal-error",
  primary: "preset-tonal-primary",
  surface: "preset-tonal",
};

/** The state ring before a row: solid by review state, dashed for a draft. */
export const RING_CLASS: Record<BadgeTone, string> = {
  success: "border-success-500",
  warning: "border-warning-500",
  error: "border-error-500",
  primary: "border-primary-500",
  surface: "border-surface-400-600",
};
