// The agent reviews the inbox shows: the Agent reviews panel (queued and
// running) and each row's latest run ("Agent review ready · N findings").
// The records come from the desk through the queue
// (`$lib/review/agentReviews.svelte`).

import { agentReviews, refreshAgentReviews } from "$lib/review/agentReviews.svelte";
import type { AgentReview, HarnessKind } from "$lib/types";

export type AgentReviewRow = {
  id: string;
  number: number;
  harness: HarnessKind;
  status: "queued" | "running" | "ready" | "failed";
  /** Unix milliseconds; `null` while queued. */
  startedAt: number | null;
  /** Findings a finished run reported. */
  findings: number;
};

function row(review: AgentReview): AgentReviewRow {
  return {
    id: review.id,
    number: review.number,
    harness: review.request.harness,
    status: review.status === "done" ? "ready" : review.status,
    startedAt: review.startedAt,
    findings: review.findings,
  };
}

/** Each PR's latest run (`reviews` newest first), one row per PR. */
export function latestPerPr(reviews: AgentReview[]): AgentReviewRow[] {
  const seen = new Set<number>();
  return reviews
    .filter((review) => {
      if (seen.has(review.number)) return false;
      seen.add(review.number);
      return true;
    })
    .map(row);
}

/** The runs in flight, oldest first: what the panel lists. */
export function inFlight(reviews: AgentReview[]): AgentReviewRow[] {
  return reviews
    .filter((review) => review.status === "queued" || review.status === "running")
    .reverse()
    .map(row);
}

/** Re-read the repository's agent reviews. */
export async function loadAgentReviews(repoDir: string): Promise<AgentReview[]> {
  return refreshAgentReviews(repoDir).catch(() => []);
}

/** The repository's agent reviews as last read, newest first. */
export function repoAgentReviews(repoDir: string): AgentReview[] {
  return agentReviews.byRepo[repoDir] ?? [];
}
