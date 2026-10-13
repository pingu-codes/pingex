/**
 * How a subagent or process status reads at a glance. Codex's collab agents,
 * the app's own agent runs and running commands each spell their states
 * differently; they all land in one of four buckets.
 */
export type StatusBucket = "Active" | "Failed" | "Stopped" | "Finished";

const ACTIVE = new Set(["active", "running", "pendingInit", "inProgress"]);
const FAILED = new Set(["errored", "failed", "systemError", "notFound"]);
const STOPPED = new Set(["killed", "orphaned", "interrupted"]);

/** The bucket for `status`; anything unrecognised reads as finished. */
export function statusBucket(status: string): StatusBucket {
  if (ACTIVE.has(status)) return "Active";
  if (FAILED.has(status)) return "Failed";
  if (STOPPED.has(status)) return "Stopped";
  return "Finished";
}

/** Text colour for `status`: green while working, red on failure, muted once over. */
export function statusTone(status: string): string {
  const bucket = statusBucket(status);
  return bucket === "Active" ? "text-success-500" : bucket === "Failed" ? "text-error-500" : "text-surface-500";
}
