import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { inboxes, onInboxSynced, resyncInbox, syncInbox } from "$lib/inbox/inboxStore.svelte";
import { emitCommandSettled } from "$lib/services/commandEvents";
import { commands } from "$lib/services/homeRouting";
import type { Inbox } from "$lib/types";

vi.mock("$lib/services/homeRouting", async (importOriginal) => {
  const actual = await importOriginal<typeof import("$lib/services/homeRouting")>();
  return {
    ...actual,
    commands: { ...actual.commands, reviewInbox: vi.fn(actual.commands.reviewInbox) },
  };
});

const MAIN = "/Users/ciaran/Projects/codex-custom";

beforeEach(() => {
  for (const key of Object.keys(inboxes)) delete inboxes[key];
  vi.mocked(commands.reviewInbox).mockReset();
});

describe("syncing after the account changes", () => {
  const inbox = (viewer: string) =>
    ({ repo: "o/n", viewer, prs: [], people: [], feed: [], complete: false }) as unknown as Inbox;

  it("runs again when asked during a sync, so the old account's answer can't stick", async () => {
    let finish: (inbox: Inbox) => void = () => {};
    vi.mocked(commands.reviewInbox)
      .mockImplementationOnce(() => new Promise((resolve) => (finish = resolve)))
      .mockResolvedValue(inbox("new-account"));

    void syncInbox(MAIN);
    const resynced = resyncInbox(MAIN);
    finish(inbox("old-account"));
    await resynced;

    expect(inboxes[MAIN]?.inbox?.viewer).toBe("new-account");
    expect(commands.reviewInbox).toHaveBeenCalledTimes(2);
  });

  it("lets a second caller wait for the sync already running", async () => {
    let finish: (inbox: Inbox) => void = () => {};
    vi.mocked(commands.reviewInbox).mockImplementationOnce(() => new Promise((resolve) => (finish = resolve)));

    void syncInbox(MAIN);
    let joined = false;
    const second = syncInbox(MAIN).then(() => (joined = true));
    await Promise.resolve();
    expect(joined).toBe(false);

    finish(inbox("me"));
    await second;
    expect(inboxes[MAIN]?.inbox?.viewer).toBe("me");
    expect(commands.reviewInbox).toHaveBeenCalledTimes(1);
  });

  it("tells listeners about every sync that read an inbox", async () => {
    const heard = vi.fn();
    onInboxSynced(heard);
    vi.mocked(commands.reviewInbox).mockResolvedValue(inbox("me"));
    await syncInbox(MAIN);
    expect(heard).toHaveBeenCalledWith(MAIN, expect.objectContaining({ viewer: "me" }));
  });

  it("syncs once when nothing is running", async () => {
    vi.mocked(commands.reviewInbox).mockResolvedValue(inbox("me"));
    await resyncInbox(MAIN);
    expect(inboxes[MAIN]?.inbox?.viewer).toBe("me");
    expect(commands.reviewInbox).toHaveBeenCalledTimes(1);
  });
});

describe("syncing after Pingex's own writes", () => {
  const OTHER = "/Users/ciaran/Projects/other";
  const inbox = { repo: "o/n", viewer: "me", prs: [], people: [], feed: [], complete: false } as unknown as Inbox;
  const settled = (name: string, names: string[], args: unknown[]) => emitCommandSettled({ name, names, args });
  const synced = () => vi.mocked(commands.reviewInbox).mock.calls.map(([repoDir]) => repoDir);

  beforeEach(async () => {
    vi.mocked(commands.reviewInbox).mockResolvedValue(inbox);
    await syncInbox(MAIN);
    await syncInbox(OTHER);
    vi.mocked(commands.reviewInbox).mockClear();
    vi.useFakeTimers({ toFake: ["Date", "setTimeout", "clearTimeout"] });
    vi.advanceTimersByTime(1000);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("resyncs a repository shortly after a write the inbox shows, once for a burst", async () => {
    settled("reviewMerge", ["repoDir", "number", "request", "window"], [MAIN, 1, {}]);
    settled("reviewReply", ["repoDir", "number", "commentId", "body", "window"], [MAIN, 1, 5, "ok"]);
    await vi.advanceTimersByTimeAsync(0);
    expect(synced()).toEqual([]);
    await vi.advanceTimersByTimeAsync(1000);
    expect(synced()).toEqual([MAIN]);
  });

  it("leaves the inbox alone after a write it does not show", async () => {
    settled("reviewEditComment", ["repoDir", "comment", "body", "window"], [MAIN, 1, "ok"]);
    settled("reviewSetViewed", ["repoDir", "number", "files", "viewed", "window"], [MAIN, 1, [], true]);
    settled("gitCommit", ["dir", "message", "window"], [MAIN, "msg"]);
    await vi.advanceTimersByTimeAsync(1000);
    expect(synced()).toEqual([]);
  });

  it("resyncs at once after an account change: the repository's own, or every one", async () => {
    settled("reviewSetRepositoryAccount", ["repoDir", "login", "window"], [OTHER, "me"]);
    await vi.advanceTimersByTimeAsync(0);
    expect(synced()).toEqual([OTHER]);
    settled("reviewSwitchGithubAccount", ["login", "window"], ["me"]);
    await vi.advanceTimersByTimeAsync(0);
    expect(synced().sort()).toEqual([OTHER, MAIN, OTHER].sort());
  });
});
