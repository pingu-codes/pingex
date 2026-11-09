// A PR as a `CheckoutTarget`, with its place in a stack as the inbox knows it.
// A stack's PRs share a worktree and stay on real git branches, so anything
// that puts a PR on disk needs to say which stack it is in.

import { inboxes } from "$lib/inbox/inboxStore.svelte";
import { checkoutTarget, type TargetPr, type TargetStack } from "$lib/review/checkoutTarget";
import { stackName } from "$lib/stack/stack";
import type { CheckoutTarget, StackListing } from "$lib/types";

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
