import { beforeEach, describe, expect, it } from "vitest";
import { commands } from "$lib/services/homeRouting";
import { DESK_REPOS, previewAgentReviews, resetDeskActivity } from "$lib/services/preview/desk";

const MAIN = "/Users/ciaran/Projects/codex-custom";

// The review desk's base data, served by the preview command fakes that
// every desk UI test and the browser preview render over.
describe("review desk preview data", () => {
  it("has a cloned, a PRs-only and a no-GitHub-remote repository", () => {
    expect(DESK_REPOS.map((repo) => repo.state).sort()).toEqual(["cloned", "cloned", "noRemote", "prsOnly"]);
    const prsOnly = DESK_REPOS.find((repo) => repo.state === "prsOnly");
    expect(prsOnly?.path).toBeNull();
  });

  it("serves each repository's inbox by its folder, or by its slug when it has no clone", async () => {
    const main = await commands.reviewInbox("/Users/ciaran/Projects/codex-custom");
    expect(main.repo).toBe("ciaran/codex-custom");
    const other = await commands.reviewInbox("/Users/ciaran/Projects/arctic-explorer");
    expect(other.repo).toBe("ciaran/arctic-explorer");
    const prsOnly = await commands.reviewInbox("openai/codex");
    expect(prsOnly.prs.some((pr) => pr.reviewRequests.includes(prsOnly.viewer))).toBe(true);
    await expect(commands.reviewInbox("/Users/ciaran/Projects/notes")).rejects.toThrow(/no GitHub repository/);
  });

  it("has a three-PR stack of mine and a stack another author asks me to review", async () => {
    const inbox = await commands.reviewInbox("/Users/ciaran/Projects/codex-custom");
    const listing = await commands.reviewStacks("/Users/ciaran/Projects/codex-custom");
    expect(listing.supported).toBe(true);
    const pr = (number: number) => inbox.prs.find((candidate) => candidate.number === number);

    const authors = listing.stacks.map((stack) => new Set(stack.prs.map((entry) => pr(entry.number)?.author)));
    const mine = listing.stacks[authors.findIndex((set) => set.size === 1 && set.has(inbox.viewer))];
    expect(mine.prs).toHaveLength(3);
    const theirs = listing.stacks.find((stack) => stack.prs.every((entry) => pr(entry.number)?.author === "jo-r"));
    expect(theirs?.prs.every((entry) => pr(entry.number)?.reviewRequests.includes(inbox.viewer))).toBe(true);

    // Bottom first: each PR is based on the head git branch of the one below.
    for (const stack of [mine, theirs!]) {
      expect(pr(stack.prs[0].number)?.baseRef).toBe(stack.baseRef);
      for (let index = 1; index < stack.prs.length; index++) {
        expect(pr(stack.prs[index].number)?.baseRef).toBe(stack.prs[index - 1].headRef);
      }
    }
  });

  it("has a PR with unresolved human review threads", async () => {
    const detail = await commands.reviewPrDetail("/Users/ciaran/Projects/codex-custom", 409);
    const threads = new Set(
      detail.comments.filter((comment) => comment.threadId && !comment.isResolved).map((c) => c.threadId),
    );
    expect(threads.size).toBeGreaterThanOrEqual(2);
    expect(detail.comments.some((comment) => comment.isResolved)).toBe(true);
    expect(detail.summary.number).toBe(409);
  });

  it("has agent reviews queued and running", () => {
    expect(previewAgentReviews.map((review) => review.status).sort()).toEqual(["queued", "running"]);
  });

  describe("activity", () => {
    beforeEach(resetDeskActivity);

    it("classifies the inbox the way the desk service does", async () => {
      const inbox = await commands.reviewInbox(MAIN);
      expect(inbox.triage.map((triage) => triage.number)).toEqual(inbox.prs.map((pr) => pr.number));
      expect(inbox.counts).toEqual({ needsMe: 8, mine: 4, reviewRequested: 4, everyone: 9 });
      const triage = (number: number) => inbox.triage.find((entry) => entry.number === number);
      expect(triage(412)?.needsMe).toBe("readyToMerge");
      expect(triage(401)?.commitsSinceMyReview).toBe(2);
    });

    it("keeps the feed until activity is marked seen, and a muted PR leaves Needs me", async () => {
      const before = await commands.reviewInbox(MAIN);
      expect(before.feed.length).toBeGreaterThan(0);
      const pr415 = before.triage.find((triage) => triage.number === 415);
      expect(pr415?.newActivity).toBeGreaterThan(0);

      await commands.reviewMarkActivitySeen(MAIN, [415]);
      await commands.reviewMutePr(MAIN, 412, before.prs.find((pr) => pr.number === 412)?.headSha ?? "");
      const after = await commands.reviewInbox(MAIN);
      expect(after.feed.some((entry) => entry.number === 415)).toBe(false);
      expect(after.triage.find((triage) => triage.number === 415)?.newActivity).toBe(0);
      expect(after.triage.find((triage) => triage.number === 412)?.needsMe).toBeNull();
      expect(after.counts.needsMe).toBe(before.counts.needsMe - 1);
    });
  });
});
