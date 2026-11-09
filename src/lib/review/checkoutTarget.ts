// The one place a PR becomes a `CheckoutTarget`, so every caller sends the head
// repository the backend needs to choose a branch and a push target.

import type { CheckoutTarget } from "$lib/types";

export interface TargetPr {
  number: number;
  headRef: string;
  headSha: string;
  headRepo?: string | null;
}

export function checkoutTarget(pr: TargetPr, mine: boolean, stack: string | null = null): CheckoutTarget {
  return {
    number: pr.number,
    headRef: pr.headRef,
    headSha: pr.headSha,
    mine,
    stack,
    headRepo: pr.headRepo ?? null,
  };
}
