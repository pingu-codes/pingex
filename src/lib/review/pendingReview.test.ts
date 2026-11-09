import { beforeEach, describe, expect, it, vi } from "vitest";
import { type NewPendingEntry, PendingReviewStore } from "$lib/review/pendingReview.svelte";
import { commands } from "$lib/services/homeRouting";
import type { PendingReview } from "$lib/types";

vi.mock("$lib/services/homeRouting", () => ({
  commands: {
    reviewPendingReview: vi.fn(),
    reviewSavePendingReview: vi.fn(),
    reviewSubmitPendingReview: vi.fn(),
    reviewDiscardPendingReview: vi.fn(),
  },
}));

const COMMENT = {
  kind: "comment",
  anchor: { path: "a.ts", side: "RIGHT", line: 1, startLine: null, startSide: null },
  body: "Looks off",
} as unknown as NewPendingEntry;

const stored = (overrides: Partial<PendingReview> = {}): PendingReview => ({
  headSha: "old",
  entries: [],
  summary: "",
  verdict: "comment",
  updatedAt: 0,
  ...overrides,
});

const lastSaved = () => vi.mocked(commands.reviewSavePendingReview).mock.lastCall?.[2];

async function loaded(head = "a") {
  const pending = new PendingReviewStore("/repo", 1);
  await pending.load(head);
  return pending;
}

beforeEach(() => {
  vi.mocked(commands.reviewPendingReview).mockReset().mockResolvedValue(null);
  vi.mocked(commands.reviewSavePendingReview)
    .mockReset()
    .mockResolvedValue(undefined as never);
  vi.mocked(commands.reviewSubmitPendingReview)
    .mockReset()
    .mockResolvedValue(undefined as never);
  vi.mocked(commands.reviewDiscardPendingReview)
    .mockReset()
    .mockResolvedValue(undefined as never);
});

describe("the head a review posts against", () => {
  it("follows the PR while the review holds nothing", async () => {
    const pending = await loaded("cached");
    pending.syncHead("fresh");
    pending.add(COMMENT);
    await pending.save();
    expect(lastSaved()?.headSha).toBe("fresh");
  });

  it("stays on the commit the reviewer read once there is something in it", async () => {
    const pending = await loaded("read");
    pending.add(COMMENT);
    pending.syncHead("newer");
    expect(pending.headSha).toBe("read");
  });

  it("starts the next review on the latest head after a submit", async () => {
    const pending = await loaded("read");
    pending.add(COMMENT);
    pending.syncHead("newer");
    await pending.submit();
    expect(pending.headSha).toBe("newer");
  });

  it("starts the next review on the latest head after a discard", async () => {
    const pending = await loaded("read");
    pending.add(COMMENT);
    pending.syncHead("newer");
    await pending.discard();
    expect(pending.headSha).toBe("newer");
  });

  it("keeps a restored review on the head it was written at", async () => {
    vi.mocked(commands.reviewPendingReview).mockResolvedValue(stored({ summary: "Nearly" }));
    const pending = await loaded("new");
    pending.syncHead("newer");
    expect(pending.headSha).toBe("old");
    expect(pending.writtenAt).toBe("old");
  });
});

describe("when the stored review can't be read", () => {
  it("reports it, and doesn't overwrite the stored review", async () => {
    vi.mocked(commands.reviewPendingReview).mockRejectedValue("database is locked");
    const pending = await loaded();
    expect(pending.loadError).toBe("database is locked");

    pending.add(COMMENT);
    await pending.save();
    expect(commands.reviewSavePendingReview).not.toHaveBeenCalled();
  });

  it("refuses to submit", async () => {
    vi.mocked(commands.reviewPendingReview).mockRejectedValue("database is locked");
    const pending = await loaded();
    pending.add(COMMENT);
    await expect(pending.submit()).rejects.toThrow();
    expect(commands.reviewSubmitPendingReview).not.toHaveBeenCalled();
  });

  it("on retry, keeps both what was stored and what was written since, and saves them", async () => {
    vi.mocked(commands.reviewPendingReview).mockRejectedValueOnce("database is locked");
    const pending = await loaded();
    pending.add(COMMENT);

    const earlier = { ...COMMENT, id: "earlier" } as PendingReview["entries"][number];
    vi.mocked(commands.reviewPendingReview).mockResolvedValue(stored({ headSha: "a", entries: [earlier] }));
    await pending.retryLoad();

    expect(pending.loadError).toBeNull();
    expect(pending.entries.map((entry) => entry.id)[0]).toBe("earlier");
    expect(pending.entries).toHaveLength(2);
    await vi.waitFor(() => expect(lastSaved()?.entries).toHaveLength(2));
  });
});

describe("when a save fails", () => {
  it("reports it without an unhandled rejection, and clears it on the next good save", async () => {
    const pending = await loaded();
    vi.mocked(commands.reviewSavePendingReview).mockRejectedValueOnce("disk full");
    pending.add(COMMENT);
    await vi.waitFor(() => expect(pending.saveError).toBe("disk full"));

    pending.setVerdict("approve");
    await vi.waitFor(() => expect(pending.saveError).toBeNull());
  });

  it("still rejects to a caller that waits on the save", async () => {
    const pending = await loaded();
    pending.add(COMMENT);
    await pending.save();
    vi.mocked(commands.reviewSavePendingReview).mockRejectedValueOnce("disk full");
    await expect(pending.submit()).rejects.toBe("disk full");
    expect(pending.entries).toHaveLength(1);
  });
});
