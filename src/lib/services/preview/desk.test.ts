import { describe, expect, it } from "vitest";
import { commands } from "$lib/services/homeRouting";
import { DESK_REPOS } from "$lib/services/preview/demoDesk";
import { resetPreviewDesk, seedPreviewDesk } from "$lib/services/preview/desk";
import { deskRepository, inboxPr, prDetail, reviewThread } from "$lib/services/preview/factories";
import type { PendingReview } from "$lib/types";

const MAIN = "/Users/ciaran/Projects/codex-custom";

// The demo the browser preview opens on and the Playwright suite drives.
describe("the preview desk's demo", () => {
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

  it("has agent reviews queued, running and finished with findings", async () => {
    const reviews = await commands.reviewAgentReviews(MAIN);
    expect(reviews.map((review) => review.status).sort()).toEqual(["done", "queued", "running"]);
    const findings = await commands.reviewFindings(MAIN, 409);
    expect(findings.length).toBeGreaterThan(0);
    expect(findings.some((finding) => finding.state === "merged")).toBe(true);
  });

  describe("activity", () => {
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

// The desk a test seeds: exactly what it states, and the fakes keep the
// detail, the inbox row and the Pending review consistent.
describe("a seeded preview desk", () => {
  const DIR = "/repos/app";
  const pr = inboxPr({ number: 7, title: "Fix the loader", author: "mira-k", unresolvedThreads: 1 });
  const seed = () =>
    seedPreviewDesk({
      repositories: [
        deskRepository("me/app", {
          repository: { path: DIR },
          prs: [pr],
          details: [prDetail(pr, { comments: reviewThread("THREAD_4", "src/a.ts", 3, [["tomasz", "Why?"]]) })],
        }),
      ],
    });

  it("holds only the seed, until the reset brings the demo back", async () => {
    seed();
    expect((await commands.reviewInbox(DIR)).prs.map((entry) => entry.number)).toEqual([7]);
    expect(await commands.reviewAgentReviews(DIR)).toEqual([]);
    resetPreviewDesk();
    expect((await commands.reviewInbox(MAIN)).repo).toBe("ciaran/codex-custom");
  });

  it("puts a reply in its thread, and resolving flips the whole thread", async () => {
    seed();
    await commands.reviewReply(DIR, 7, 40, "Because the cache is cold.");
    const thread = (await commands.reviewPrDetail(DIR, 7)).comments.filter((entry) => entry.threadId === "THREAD_4");
    expect(thread.map((entry) => [entry.author, entry.body])).toEqual([
      ["tomasz", "Why?"],
      ["ciaran", "Because the cache is cold."],
    ]);

    await commands.reviewResolveThread(DIR, "THREAD_4");
    const resolved = (await commands.reviewPrDetail(DIR, 7)).comments.filter((entry) => entry.threadId === "THREAD_4");
    expect(resolved.every((entry) => entry.isResolved)).toBe(true);
    expect((await commands.reviewInbox(DIR)).prs[0].unresolvedThreads).toBe(0);
  });

  it("submits the Pending review onto the PR and forgets it", async () => {
    seed();
    const pending: PendingReview = {
      headSha: pr.headSha,
      summary: "Two things",
      verdict: "requestChanges",
      entries: [
        { kind: "comment", id: "c1", anchor: { path: "src/a.ts", line: 5, side: "RIGHT" }, body: "Split this" },
        { kind: "reply", id: "r1", threadId: "THREAD_4", commentId: 40, path: "src/a.ts", line: 3, body: "Agreed" },
      ],
    };
    await commands.reviewSavePendingReview(DIR, 7, pending);

    await commands.reviewSubmitPendingReview(DIR, 7);

    expect(await commands.reviewPendingReview(DIR, 7)).toBeNull();
    const detail = await commands.reviewPrDetail(DIR, 7);
    const review = detail.reviews.at(-1);
    expect(review).toMatchObject({ author: "ciaran", state: "CHANGES_REQUESTED", body: "Two things" });
    const posted = detail.comments.filter((entry) => entry.reviewId === review?.id);
    expect(posted.map((entry) => [entry.path, entry.line, entry.body])).toEqual([
      ["src/a.ts", 5, "Split this"],
      ["src/a.ts", 3, "Agreed"],
    ]);
    expect(posted[1].threadId).toBe("THREAD_4");
    expect((await commands.reviewInbox(DIR)).triage[0].myReview).toBe("CHANGES_REQUESTED");
    await expect(commands.reviewSubmitPendingReview(DIR, 7)).rejects.toThrow(/no pending review/);
  });

  it("posts a review submitted at once, with its inline comments as new threads", async () => {
    seed();
    await commands.reviewSubmit(DIR, 7, "APPROVE", "Ship it", pr.headSha, [
      { path: "src/a.ts", line: 9, body: "Nit: a name" },
    ]);
    const detail = await commands.reviewPrDetail(DIR, 7);
    expect(detail.reviews.at(-1)).toMatchObject({ state: "APPROVED", body: "Ship it" });
    const comment = detail.comments.at(-1);
    expect(comment).toMatchObject({ path: "src/a.ts", line: 9, body: "Nit: a name", isResolved: false });
    expect(comment?.threadId).not.toBe("THREAD_4");
    expect((await commands.reviewInbox(DIR)).prs[0].unresolvedThreads).toBe(2);
  });
});
