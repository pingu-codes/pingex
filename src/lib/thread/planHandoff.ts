/**
 * "Clear context & implement": the plan is carried into a brand-new thread, so
 * the implementation turn starts with the plan as its only context instead of
 * the whole planning conversation. The seed message therefore has to restate
 * the plan in full — nothing else survives the switch.
 */

/** The message that opens the fresh implementation thread. */
export function freshPlanPrompt(plan: string): string {
  return [
    "Implement the plan below.",
    "",
    "It was agreed in an earlier session that this thread cannot see, so the plan is your only context — re-read whatever files you need before changing anything.",
    "",
    plan.trim(),
  ].join("\n");
}
