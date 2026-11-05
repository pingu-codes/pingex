// The agent reviews the inbox shows as queued or running. Agent reviews get
// their backend record in a later slice; until then the browser preview
// reads its fakes and the app lists none.

import { isTauri } from "$lib/services/tauri";
import type { CheckoutStep, HarnessKind } from "$lib/types";

export type AgentReviewRow = {
  id: string;
  number: number;
  harness: HarnessKind;
  step: CheckoutStep;
  status: "queued" | "running";
  /** Unix milliseconds; `null` while queued. */
  startedAt: number | null;
};

/** Agent reviews in flight for repository `slug` (`owner/name`). */
export async function loadAgentReviews(slug: string): Promise<AgentReviewRow[]> {
  if (isTauri()) return [];
  const { previewAgentReviews } = await import("$lib/services/preview/desk");
  return previewAgentReviews.filter((review) => review.repo === slug).map(({ repo: _repo, ...row }) => row);
}
