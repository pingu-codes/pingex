import { describe, expect, it } from "vitest";
import { threadSummary } from "$lib/services/preview/factories";
import type { PrLink, Project } from "$lib/types";
import { fileReviewThreads } from "./reviewFiling";

const repo = (path: string, threads = [] as Project["threads"]): Project =>
  ({ path, name: path.split("/").pop(), kind: "folder", threads }) as unknown as Project;

const link = (threadId: string, overrides: Partial<PrLink> = {}): PrLink => ({
  threadId,
  repoKey: "/code/pingex",
  prNumber: 409,
  mine: false,
  step: "noCheckout",
  checkoutPath: null,
  gitBranch: "jo-r/usage-meter",
  startSha: "3c1e9a2000000000000000000000000000000000",
  createdAt: 1,
  closedAt: null,
  ...overrides,
});

describe("fileReviewThreads", () => {
  const diff = threadSummary({ id: "diff", title: "Diff review", updatedAt: 30 });
  const plain = threadSummary({ id: "plain", title: "Unrelated", updatedAt: 20 });
  const tests = threadSummary({ id: "tests", title: "Stack review with tests", updatedAt: 40 });
  const mine = threadSummary({ id: "mine", title: "Address review on #415", updatedAt: 10 });

  it("files threads on other people's PRs and my PRs without a worktree under Reviews", () => {
    const filing = fileReviewThreads([repo("/code/pingex", [diff, plain, mine])], [tests], {
      diff: link("diff"),
      tests: link("tests", { prNumber: 403, step: "worktree" }),
      mine: link("mine", { prNumber: 415, mine: true, step: "worktree" }),
    });

    expect(filing.reviews["/code/pingex"].map((row) => row.thread.id)).toEqual(["tests", "diff"]);
    expect([...filing.taken].sort()).toEqual(["diff", "tests"]);
  });

  it("leaves a thread to its usual place once its PR has closed", () => {
    const filing = fileReviewThreads([repo("/code/pingex", [diff])], [], { diff: link("diff", { closedAt: 99 }) });
    expect(filing.reviews).toEqual({});
    expect(filing.taken.size).toBe(0);
  });

  it("keeps someone else's PR thread under Reviews even in a worktree", () => {
    const filing = fileReviewThreads([repo("/code/pingex", [diff])], [], { diff: link("diff", { step: "worktree" }) });
    expect(filing.reviews["/code/pingex"]).toHaveLength(1);
  });

  it("files nothing for a repository the sidebar does not list", () => {
    const filing = fileReviewThreads([repo("/code/other", [diff])], [], { diff: link("diff") });
    expect(filing.reviews).toEqual({});
    expect(filing.taken.size).toBe(0);
  });
});
