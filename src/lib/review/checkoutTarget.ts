// The one place a PR becomes a `CheckoutTarget`, so every caller sends the head
// repository the backend needs to choose a branch and a push target.

import type { CheckoutTarget } from "$lib/types";

export interface TargetPr {
  number: number;
  headRef: string;
  headSha: string;
  headRepo?: string | null;
}

/** A PR's stack: its name and the git branches of its open PRs, bottom first. */
export interface TargetStack {
  name: string;
  branches: string[];
}

export function checkoutTarget(pr: TargetPr, mine: boolean, stack: TargetStack | null = null): CheckoutTarget {
  return {
    number: pr.number,
    headRef: pr.headRef,
    headSha: pr.headSha,
    mine,
    stack: stack?.name ?? null,
    headRepo: pr.headRepo ?? null,
    stackBranches: stack?.branches ?? [],
  };
}
