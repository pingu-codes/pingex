import { flushSync } from "svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { emitCommandSettled } from "$lib/services/commandEvents";
import { commands } from "$lib/services/homeRouting";
import {
  fetchQuery,
  invalidate,
  prefetch,
  type QueryHandle,
  query,
  resetQueryCache,
} from "$lib/services/queryCache.svelte";

vi.mock("$lib/services/homeRouting", () => ({
  commands: {
    gitStatus: vi.fn(),
    reviewPrDetail: vi.fn(),
    reviewDiff: vi.fn(),
    reviewProviderStatus: vi.fn(),
  },
}));

const gitStatus = vi.mocked(commands.gitStatus);
const prDetail = vi.mocked(commands.reviewPrDetail);
const reviewDiff = vi.mocked(commands.reviewDiff);
const providerStatus = vi.mocked(commands.reviewProviderStatus);

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

/** Let settled promises run their callbacks. */
const settle = () => vi.advanceTimersByTimeAsync(0);

/** Mount `query` the way a component would, until `stop()`. */
function mounted<T>(make: () => QueryHandle<T>) {
  let handle!: QueryHandle<T>;
  const stop = $effect.root(() => {
    handle = make();
  });
  flushSync();
  return { handle, stop };
}

const settled = (name: string, names: string[], args: unknown[]) => emitCommandSettled({ name, names, args });

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["Date", "setTimeout", "clearTimeout"] });
  resetQueryCache();
  gitStatus.mockReset();
  prDetail.mockReset();
  reviewDiff.mockReset();
  providerStatus.mockReset();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("fetchQuery", () => {
  it("serves a fresh answer without asking again", async () => {
    gitStatus.mockResolvedValue("clean" as never);
    expect(await fetchQuery("gitStatus", "/repo")).toBe("clean");
    expect(await fetchQuery("gitStatus", "/repo")).toBe("clean");
    expect(gitStatus).toHaveBeenCalledTimes(1);
  });

  it("joins a read already in flight", async () => {
    const read = deferred<string>();
    gitStatus.mockReturnValue(read.promise as never);
    const first = fetchQuery("gitStatus", "/repo");
    const second = fetchQuery("gitStatus", "/repo");
    read.resolve("clean");
    expect(await Promise.all([first, second])).toEqual(["clean", "clean"]);
    expect(gitStatus).toHaveBeenCalledTimes(1);
  });

  it("serves an aged answer at once and refreshes it behind", async () => {
    prDetail.mockResolvedValueOnce("v1" as never).mockResolvedValueOnce("v2" as never);
    expect(await fetchQuery("reviewPrDetail", "/repo", 1)).toBe("v1");
    await vi.advanceTimersByTimeAsync(31_000);
    expect(await fetchQuery("reviewPrDetail", "/repo", 1)).toBe("v1");
    await settle();
    expect(prDetail).toHaveBeenCalledTimes(2);
    expect(await fetchQuery("reviewPrDetail", "/repo", 1)).toBe("v2");
  });

  it("waits for a new answer once the old one expires", async () => {
    prDetail.mockResolvedValueOnce("v1" as never).mockResolvedValueOnce("v2" as never);
    await fetchQuery("reviewPrDetail", "/repo", 1);
    await vi.advanceTimersByTimeAsync(11 * 60_000);
    expect(await fetchQuery("reviewPrDetail", "/repo", 1)).toBe("v2");
  });

  it("waits for a new answer after an invalidation", async () => {
    gitStatus.mockResolvedValueOnce("before" as never).mockResolvedValueOnce("after" as never);
    await fetchQuery("gitStatus", "/repo");
    invalidate({ kinds: ["local"] });
    expect(await fetchQuery("gitStatus", "/repo")).toBe("after");
  });

  it("rejects when the first read fails, and tries again next time", async () => {
    gitStatus.mockRejectedValueOnce(new Error("not a repository")).mockResolvedValueOnce("clean" as never);
    await expect(fetchQuery("gitStatus", "/repo")).rejects.toThrow("not a repository");
    expect(await fetchQuery("gitStatus", "/repo")).toBe("clean");
  });

  it("keys on the arguments, ignoring a trailing null", async () => {
    gitStatus.mockImplementation(async (dir) => `status of ${dir}` as never);
    expect(await fetchQuery("gitStatus", "/a")).toBe("status of /a");
    expect(await fetchQuery("gitStatus", "/b")).toBe("status of /b");
    expect(await fetchQuery("gitStatus", "/a", null)).toBe("status of /a");
    expect(gitStatus).toHaveBeenCalledTimes(2);
  });
});

describe("query", () => {
  it("shows the cached answer on the first render of a remount", async () => {
    prDetail.mockResolvedValue("detail" as never);
    const first = mounted(() => query("reviewPrDetail", () => ["/repo", 7]));
    expect(first.handle.loading).toBe(true);
    await settle();
    expect(first.handle.value).toBe("detail");
    first.stop();

    const second = mounted(() => query("reviewPrDetail", () => ["/repo", 7]));
    expect(second.handle.value).toBe("detail");
    expect(second.handle.loading).toBe(false);
    second.stop();
    expect(prDetail).toHaveBeenCalledTimes(1);
  });

  it("re-reads what it shows when invalidated, keeping the old value meanwhile", async () => {
    const next = deferred<string>();
    gitStatus.mockResolvedValueOnce("before" as never).mockReturnValueOnce(next.promise as never);
    const { handle, stop } = mounted(() => query("gitStatus", () => ["/repo"]));
    await settle();
    invalidate({ kinds: ["local"] });
    expect(handle.value).toBe("before");
    expect(handle.refreshing).toBe(true);
    expect(handle.loading).toBe(false);
    next.resolve("after");
    await settle();
    expect(handle.value).toBe("after");
    stop();
  });

  it("leaves an unobserved entry to re-read when next asked for", async () => {
    gitStatus.mockResolvedValue("clean" as never);
    const { stop } = mounted(() => query("gitStatus", () => ["/repo"]));
    await settle();
    stop();
    invalidate({ kinds: ["local"] });
    await settle();
    expect(gitStatus).toHaveBeenCalledTimes(1);
  });

  it("reads again after a mutation that landed mid-read", async () => {
    const first = deferred<string>();
    gitStatus.mockReturnValueOnce(first.promise as never).mockResolvedValueOnce("after commit" as never);
    const { handle, stop } = mounted(() => query("gitStatus", () => ["/repo"]));
    invalidate({ kinds: ["local"] });
    first.resolve("before commit");
    await settle();
    expect(handle.value).toBe("after commit");
    expect(gitStatus).toHaveBeenCalledTimes(2);
    stop();
  });

  it("keeps the value when a refresh fails", async () => {
    gitStatus.mockResolvedValueOnce("clean" as never).mockRejectedValueOnce(new Error("offline"));
    const { handle, stop } = mounted(() => query("gitStatus", () => ["/repo"]));
    await settle();
    await handle.reload();
    expect(handle.value).toBe("clean");
    expect(handle.error).toBe("offline");
    stop();
  });

  it("follows reactive arguments and shows nothing for null", async () => {
    gitStatus.mockImplementation(async (dir) => `status of ${dir}` as never);
    let dir = $state<string | null>("/a");
    const { handle, stop } = mounted(() => query("gitStatus", () => (dir ? [dir] : null)));
    await settle();
    expect(handle.value).toBe("status of /a");
    dir = "/b";
    flushSync();
    await settle();
    expect(handle.value).toBe("status of /b");
    dir = null;
    flushSync();
    expect(handle.value).toBeUndefined();
    expect(handle.loading).toBe(false);
    stop();
  });
});

describe("invalidation by mutations", () => {
  it("a git mutation marks local reads, not GitHub ones", async () => {
    gitStatus.mockResolvedValue("clean" as never);
    prDetail.mockResolvedValue("detail" as never);
    const status = mounted(() => query("gitStatus", () => ["/repo"]));
    const detail = mounted(() => query("reviewPrDetail", () => ["/repo", 1]));
    await settle();
    settled("gitCommit", ["dir", "message", "window"], ["/repo", "msg"]);
    await settle();
    expect(gitStatus).toHaveBeenCalledTimes(2);
    expect(prDetail).toHaveBeenCalledTimes(1);
    status.stop();
    detail.stop();
  });

  it("a GitHub write marks that repository's reads only", async () => {
    prDetail.mockResolvedValue("detail" as never);
    const here = mounted(() => query("reviewPrDetail", () => ["/repo", 1]));
    const elsewhere = mounted(() => query("reviewPrDetail", () => ["/other", 1]));
    await settle();
    settled("reviewReply", ["repoDir", "number", "commentId", "body", "window"], ["/repo", 1, 5, "ok"]);
    await settle();
    expect(prDetail.mock.calls.map(([repo]) => repo)).toEqual(["/repo", "/other", "/repo"]);
    here.stop();
    elsewhere.stop();
  });

  it("a local-only review write and a read change nothing", async () => {
    prDetail.mockResolvedValue("detail" as never);
    const { stop } = mounted(() => query("reviewPrDetail", () => ["/repo", 1]));
    await settle();
    settled("reviewSetViewed", ["repoDir", "number", "files", "viewed", "window"], ["/repo", 1, [], true]);
    settled("reviewInbox", ["repoDir", "window"], ["/repo"]);
    settled("reviewPrDetail", ["repoDir", "number", "window"], ["/repo", 1]);
    await settle();
    expect(prDetail).toHaveBeenCalledTimes(1);
    stop();
  });

  it("an account change marks that repository's GitHub reads and provider status, not local ones", async () => {
    gitStatus.mockResolvedValue("clean" as never);
    prDetail.mockResolvedValue("detail" as never);
    providerStatus.mockResolvedValue("signed in" as never);
    const status = mounted(() => query("gitStatus", () => ["/repo"]));
    const detail = mounted(() => query("reviewPrDetail", () => ["/repo", 1]));
    const elsewhere = mounted(() => query("reviewPrDetail", () => ["/other", 1]));
    const provider = mounted(() => query("reviewProviderStatus", () => ["/repo"]));
    await settle();
    settled("reviewSetRepositoryAccount", ["repoDir", "login", "window"], ["/repo", "me"]);
    await settle();
    expect(gitStatus).toHaveBeenCalledTimes(1);
    expect(prDetail.mock.calls.map(([repo]) => repo)).toEqual(["/repo", "/other", "/repo"]);
    expect(providerStatus).toHaveBeenCalledTimes(2);
    settled("reviewSwitchGithubAccount", ["login", "window"], ["me"]);
    await settle();
    expect(prDetail.mock.calls.map(([repo]) => repo)).toEqual(["/repo", "/other", "/repo", "/repo", "/other"]);
    for (const view of [status, detail, elsewhere, provider]) view.stop();
  });

  it("a write that reports through named reads marks only those", async () => {
    gitStatus.mockResolvedValue("clean" as never);
    providerStatus.mockResolvedValue("linked" as never);
    const status = mounted(() => query("gitStatus", () => ["/repo"]));
    const here = mounted(() => query("reviewProviderStatus", () => ["/repo"]));
    const elsewhere = mounted(() => query("reviewProviderStatus", () => ["/other"]));
    await settle();
    settled("reviewLinkClone", ["repoDir", "slug", "window"], ["/repo", "o/n"]);
    await settle();
    expect(gitStatus).toHaveBeenCalledTimes(1);
    expect(providerStatus.mock.calls.map(([repo]) => repo)).toEqual(["/repo", "/other", "/repo"]);
    for (const view of [status, here, elsewhere]) view.stop();
  });

  it("an uncached read marks nothing", async () => {
    gitStatus.mockResolvedValue("clean" as never);
    const { stop } = mounted(() => query("gitStatus", () => ["/repo"]));
    await settle();
    settled("reviewWorktreeContext", ["dir", "window"], ["/repo"]);
    settled("gitWorktreeHandoffPreflight", ["worktreePath", "targetDir", "window"], ["/repo", "/main"]);
    await settle();
    expect(gitStatus).toHaveBeenCalledTimes(1);
    stop();
  });

  it("SHA-keyed answers survive mutations and expire with time", async () => {
    reviewDiff.mockResolvedValue("patch" as never);
    await fetchQuery("reviewDiff", "/repo", "a", "b");
    settled("reviewReply", ["repoDir", "number", "commentId", "body", "window"], ["/repo", 1, 5, "ok"]);
    settled("gitCommit", ["dir", "message", "window"], ["/repo", "msg"]);
    await fetchQuery("reviewDiff", "/repo", "a", "b");
    expect(reviewDiff).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(31 * 60_000);
    await fetchQuery("reviewDiff", "/repo", "a", "b");
    expect(reviewDiff).toHaveBeenCalledTimes(2);
  });
});

describe("prefetch", () => {
  it("warms the entry a later mount shows at once", async () => {
    prDetail.mockResolvedValue("next PR" as never);
    prefetch("reviewPrDetail", "/repo", 2);
    await settle();
    const { handle, stop } = mounted(() => query("reviewPrDetail", () => ["/repo", 2]));
    expect(handle.value).toBe("next PR");
    stop();
  });

  it("swallows a failure", async () => {
    prDetail.mockRejectedValue(new Error("rate limited"));
    prefetch("reviewPrDetail", "/repo", 2);
    await settle();
    expect(prDetail).toHaveBeenCalledTimes(1);
  });
});
