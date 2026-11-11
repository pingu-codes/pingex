import { flushSync } from "svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { type NewPendingEntry, PendingReviewStore } from "$lib/review/pendingReview.svelte";
import { PrUnderReview } from "$lib/review/prUnderReview.svelte";
import { prPlace } from "$lib/review/reviewPage";
import { commands } from "$lib/services/homeRouting";
import { invalidate } from "$lib/services/queryCache.svelte";
import type { ChangesSince, PendingReview, PrDetail, PrFile } from "$lib/types";

vi.mock("$lib/services/homeRouting", () => ({
  commands: {
    reviewPrDetail: vi.fn(),
    reviewViewedFiles: vi.fn(),
    reviewSetViewed: vi.fn(),
    reviewPendingReview: vi.fn(),
    reviewSavePendingReview: vi.fn(),
    reviewSubmitPendingReview: vi.fn(),
    reviewDiscardPendingReview: vi.fn(),
    reviewFetchPrs: vi.fn(),
    reviewChangesSince: vi.fn(),
    reviewMarkSeen: vi.fn(),
    reviewFile: vi.fn(),
    reviewReply: vi.fn(),
  },
}));

const REPO = "/repo";
const PR = 7;
const mocked = vi.mocked(commands);

const COMMENT = {
  kind: "comment",
  anchor: { path: "src/a.ts", side: "RIGHT", line: 1, startLine: null, startSide: null },
  body: "Looks off",
} as unknown as NewPendingEntry;

function file(path: string, blobSha = `${path}@1`): PrFile {
  return { path, blobSha, patch: "", hunks: [] } as unknown as PrFile;
}

/** The PR at `headSha`; `changed` files have new content there. */
function detailAt(headSha: string, changed: string[] = []): PrDetail {
  return {
    headSha,
    baseSha: "base",
    summary: { author: "author" },
    commits: [],
    files: ["src/a.ts", "docs/b.md"].map((path) =>
      file(path, changed.includes(path) ? `${path}@${headSha}` : undefined),
    ),
    comments: [],
    reviews: [],
  } as unknown as PrDetail;
}

function sinceAt(headSha: string): ChangesSince {
  return { seenSha: "seen", headSha, fastForward: true, files: [file("src/a.ts")], rangeDiff: null, truncated: false };
}

/** A promise the test settles by hand. */
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (cause: unknown) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}

const order = (mock: { mock: { invocationCallOrder: number[] } }) => mock.mock.invocationCallOrder;
const lastSaved = () => mocked.reviewSavePendingReview.mock.lastCall?.[2];

let stops: (() => void)[] = [];

/** Open the PR as a mounted page would. */
function open(viewer = "me") {
  const pending = new PendingReviewStore(REPO, PR);
  let review!: PrUnderReview;
  stops.push(
    $effect.root(() => {
      review = new PrUnderReview({ repoDir: REPO, number: PR, pending, viewer: () => viewer });
    }),
  );
  flushSync();
  return { review, pending };
}

/** The author pushes: the next read of the PR answers at `headSha`. */
function push(headSha: string, changed: string[] = []) {
  mocked.reviewPrDetail.mockResolvedValue(detailAt(headSha, changed));
  invalidate({ names: ["reviewPrDetail"] });
}

beforeEach(() => {
  for (const mock of Object.values(mocked)) mock.mockReset();
  mocked.reviewPrDetail.mockResolvedValue(detailAt("head1"));
  mocked.reviewViewedFiles.mockResolvedValue([]);
  mocked.reviewSetViewed.mockResolvedValue(undefined as never);
  mocked.reviewPendingReview.mockResolvedValue(null);
  mocked.reviewSavePendingReview.mockResolvedValue(undefined as never);
  mocked.reviewSubmitPendingReview.mockResolvedValue(undefined as never);
  mocked.reviewFetchPrs.mockResolvedValue([]);
  mocked.reviewChangesSince.mockImplementation(async (_repo, _number, _base, head) => sinceAt(head));
  mocked.reviewMarkSeen.mockResolvedValue(undefined as never);
  mocked.reviewFile.mockResolvedValue({ text: "", size: 0, truncated: false });
});

afterEach(() => {
  for (const stop of stops) stop();
  stops = [];
});

describe("following the PR's head", () => {
  it("a new head while the first is loading: one read of the stored review, marks only for the newest head", async () => {
    const stored = deferred<PendingReview | null>();
    const firstMarks = deferred<string[]>();
    mocked.reviewPendingReview.mockReturnValue(stored.promise);
    mocked.reviewViewedFiles.mockReturnValueOnce(firstMarks.promise).mockResolvedValueOnce(["docs/b.md"]);
    const { review, pending } = open();
    await vi.waitFor(() => expect(mocked.reviewViewedFiles).toHaveBeenCalledOnce());

    push("head2");
    await vi.waitFor(() => expect(mocked.reviewViewedFiles).toHaveBeenCalledTimes(2));
    firstMarks.resolve(["src/a.ts", "docs/b.md"]);
    stored.resolve({ headSha: "", entries: [COMMENT as never], summary: "", verdict: "comment", updatedAt: 0 });
    await vi.waitFor(() => expect(review.ready).toBe(true));

    expect(mocked.reviewPendingReview).toHaveBeenCalledOnce();
    expect(pending.entries).toHaveLength(1);
    expect([...review.files.viewed]).toEqual(["docs/b.md"]);
  });

  it("re-reads the viewed marks on a new head: a changed file is unread and open, an unchanged one stays", async () => {
    mocked.reviewViewedFiles.mockResolvedValue(["src/a.ts", "docs/b.md"]);
    const { review } = open();
    await vi.waitFor(() => expect(review.files.viewed.size).toBe(2));
    expect(review.files.collapsed.has("src/a.ts")).toBe(true);

    mocked.reviewViewedFiles.mockResolvedValue(["docs/b.md"]);
    push("head2", ["src/a.ts"]);

    await vi.waitFor(() => expect([...review.files.viewed]).toEqual(["docs/b.md"]));
    expect(mocked.reviewViewedFiles).toHaveBeenLastCalledWith(REPO, PR, [
      { path: "src/a.ts", blobSha: "src/a.ts@head2" },
      { path: "docs/b.md", blobSha: "docs/b.md@1" },
    ]);
    expect(review.files.collapsed.has("src/a.ts")).toBe(false);
    expect(review.files.collapsed.has("docs/b.md")).toBe(true);
  });

  it("posts a first comment against the head the PR is at now, not the one it opened at", async () => {
    const { review, pending } = open();
    await vi.waitFor(() => expect(review.ready).toBe(true));
    push("head2");
    await vi.waitFor(() => expect(review.detail?.headSha).toBe("head2"));

    pending.add(COMMENT);
    expect(await review.submit()).toBe(true);
    expect(lastSaved()?.headSha).toBe("head2");
    expect(review.stale).toBe(false);
  });

  it("keeps a review with content on the head it was written at, and says it is stale", async () => {
    const { review, pending } = open();
    await vi.waitFor(() => expect(review.ready).toBe(true));
    pending.add(COMMENT);
    push("head2");
    await vi.waitFor(() => expect(review.detail?.headSha).toBe("head2"));

    expect(pending.headSha).toBe("head1");
    expect(review.stale).toBe(true);
  });
});

describe("changes since the Seen head", () => {
  it("fetches the PR's refs before reading them", async () => {
    const { review } = open();
    await vi.waitFor(() => expect(review.detail).not.toBeNull());
    await review.showChanges("since");

    expect(review.changes).toBe("since");
    expect(review.shownFiles.map((shown) => shown.path)).toEqual(["src/a.ts"]);
    expect(order(mocked.reviewFetchPrs)[0]).toBeLessThan(order(mocked.reviewChangesSince)[0]);
  });

  it("opens on them when the inbox asks, and remembers the choice", async () => {
    prPlace(REPO, PR).changes = "since";
    const { review } = open();
    expect(review.opensOnSince).toBe(true);
    await vi.waitFor(() => expect(review.changes).toBe("since"));

    await review.showChanges("all");
    expect(prPlace(REPO, PR).changes).toBe("all");
  });

  it("follows a new head, and drops an answer for the head the PR has left", async () => {
    const stale = deferred<ChangesSince>();
    mocked.reviewChangesSince.mockReturnValueOnce(stale.promise);
    prPlace(REPO, PR).changes = "since";
    const { review } = open();
    await vi.waitFor(() => expect(mocked.reviewChangesSince).toHaveBeenCalledOnce());

    push("head2");
    await vi.waitFor(() => expect(review.detail?.headSha).toBe("head2"));
    stale.resolve(sinceAt("head1"));
    await vi.waitFor(() => expect(mocked.reviewChangesSince).toHaveBeenCalledTimes(2));
    await vi.waitFor(() => expect(review.since?.headSha).toBe("head2"));
    expect(mocked.reviewChangesSince).toHaveBeenLastCalledWith(REPO, PR, "base", "head2");
  });
});

describe("submitting", () => {
  it("posts, then fetches the refs and marks the head seen, then re-reads the PR", async () => {
    const { review, pending } = open();
    await vi.waitFor(() => expect(review.ready).toBe(true));
    pending.add(COMMENT);
    const reads = mocked.reviewPrDetail.mock.calls.length;

    expect(await review.submit()).toBe(true);

    expect(mocked.reviewMarkSeen).toHaveBeenCalledWith(REPO, PR, "head1");
    const submitted = order(mocked.reviewSubmitPendingReview)[0];
    const fetched = order(mocked.reviewFetchPrs).at(-1) as number;
    const seen = order(mocked.reviewMarkSeen)[0];
    const reread = order(mocked.reviewPrDetail).at(-1) as number;
    expect(mocked.reviewPrDetail.mock.calls.length).toBe(reads + 1);
    expect(submitted).toBeLessThan(fetched);
    expect(fetched).toBeLessThan(seen);
    expect(seen).toBeLessThan(reread);
    expect(pending.entries).toEqual([]);
  });

  it("posts a bare Approve", async () => {
    const { review, pending } = open();
    await vi.waitFor(() => expect(review.ready).toBe(true));
    pending.setVerdict("approve");

    expect(await review.submit()).toBe(true);
    expect(mocked.reviewSubmitPendingReview).toHaveBeenCalledOnce();
    expect(lastSaved()?.verdict).toBe("approve");
    expect(order(mocked.reviewSavePendingReview).at(-1)).toBeLessThan(order(mocked.reviewSubmitPendingReview)[0]);
  });

  it("posts the author's own review as a comment, which is all GitHub takes from them", async () => {
    const { review, pending } = open("author");
    await vi.waitFor(() => expect(review.ready).toBe(true));
    expect(review.ownPr).toBe(true);
    pending.add(COMMENT);
    pending.setVerdict("approve");

    await review.submit();
    expect(lastSaved()?.verdict).toBe("comment");
  });

  it("keeps the review and marks nothing seen when GitHub refuses it", async () => {
    mocked.reviewSubmitPendingReview.mockRejectedValue(new Error("Validation failed"));
    const { review, pending } = open();
    await vi.waitFor(() => expect(review.ready).toBe(true));
    pending.add(COMMENT);

    expect(await review.submit()).toBe(false);
    expect(review.error).toBe("Validation failed");
    expect(pending.entries).toHaveLength(1);
    expect(mocked.reviewMarkSeen).not.toHaveBeenCalled();
  });
});

describe("writes and reads by SHA", () => {
  it("re-reads the PR after a write", async () => {
    const { review } = open();
    await vi.waitFor(() => expect(review.detail).not.toBeNull());
    mocked.reviewPrDetail.mockClear();

    expect(await review.write(() => commands.reviewReply(REPO, PR, 10, "Thanks"))).toBe(true);
    expect(mocked.reviewPrDetail).toHaveBeenCalledOnce();
    expect(order(mocked.reviewReply)[0]).toBeLessThan(order(mocked.reviewPrDetail)[0]);
  });

  it("saves a viewed mark, and says when that fails", async () => {
    mocked.reviewSetViewed.mockRejectedValue(new Error("database is locked"));
    const { review } = open();
    await vi.waitFor(() => expect(review.detail).not.toBeNull());
    const target = review.shownFiles[0];

    review.setViewed([target], true);
    expect(review.files.viewed.has(target.path)).toBe(true);
    expect(mocked.reviewSetViewed).toHaveBeenCalledWith(REPO, PR, [{ path: "src/a.ts", blobSha: "src/a.ts@1" }], true);
    await vi.waitFor(() => expect(review.error).toBe("database is locked"));
  });

  it("reads a whole file only once the PR's refs are fetched", async () => {
    const fetch = deferred<never>();
    mocked.reviewFetchPrs.mockReturnValue(fetch.promise);
    const { review } = open();
    let handle!: ReturnType<PrUnderReview["fileAt"]>;
    stops.push(
      $effect.root(() => {
        handle = review.fileAt(() => ["abc", "src/a.ts"]);
      }),
    );
    flushSync();
    await Promise.resolve();
    expect(mocked.reviewFile).not.toHaveBeenCalled();

    fetch.resolve(undefined as never);
    await vi.waitFor(() => expect(handle.value).toBeDefined());
    expect(mocked.reviewFile).toHaveBeenCalledWith(REPO, "abc", "src/a.ts");
  });
});
