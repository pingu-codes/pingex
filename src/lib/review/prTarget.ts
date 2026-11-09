// A PR as a `CheckoutTarget`, with its place in a stack as the inbox knows it.
// A stack's PRs share a worktree and stay on real git branches, so anything
// that puts a PR on disk needs to say which stack it is in.

import { inboxes } from "$lib/inbox/inboxStore.svelte";
import { checkoutTarget, type TargetPr, type TargetStack } from "$lib/review/checkoutTarget";
import { stackName } from "$lib/stack/stack";
import type { CheckoutTarget, PrDetail, StackListing } from "$lib/types";

/** The open stack PR `number` is an open member of, in `listing`. */
export function stackOfPr(listing: StackListing | null | undefined, number: number): TargetStack | null {
  if (!listing?.supported) return null;
  const stack = listing.stacks.find(
    (candidate) =>
      candidate.open &&
      candidate.prs.some((entry) => entry.number === number && entry.state === "open" && !entry.merged),
  );
  if (!stack) return null;
  return {
    name: stackName(stack),
    branches: stack.prs.filter((entry) => entry.state === "open" && !entry.merged).map((entry) => entry.headRef),
  };
}

export function prTarget(repoDir: string, pr: TargetPr, mine: boolean): CheckoutTarget {
  return checkoutTarget(pr, mine, stackOfPr(inboxes[repoDir]?.stacks, pr.number));
}

/** An inbox row's target. `viewer` defaults to the account the last sync read. */
export function inboxPrTarget(
  repoDir: string,
  pr: TargetPr & { author: string },
  viewer: string | null | undefined = inboxes[repoDir]?.inbox?.viewer,
): CheckoutTarget {
  return prTarget(repoDir, pr, !!viewer && pr.author === viewer);
}

/** The target for a PR the review page has open. */
export function detailTarget(repoDir: string, detail: PrDetail, viewer?: string | null): CheckoutTarget {
  return inboxPrTarget(repoDir, { ...detail.summary, headSha: detail.headSha }, viewer ?? undefined);
}
