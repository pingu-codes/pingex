import type { ThreadItem } from "$lib/types";

const PROPOSED_PLAN = /<proposed_plan>\s*([\s\S]*?)\s*<\/proposed_plan>/;

/**
 * The plan an item carries, if any. Codex only turns a `<proposed_plan>` block
 * into a `plan` item when it believes the turn ran in plan mode; a plan the
 * model wrote while Codex thought it was in default mode (e.g. after a mode
 * mismatch on resume) arrives as a plain agent message, so that is read too.
 */
export function planText(item: ThreadItem): string | null {
  if (item.type === "plan") return item.text || null;
  if (item.type === "agentMessage" && !item.streaming && item.text) {
    return PROPOSED_PLAN.exec(item.text)?.[1] || null;
  }
  return null;
}
