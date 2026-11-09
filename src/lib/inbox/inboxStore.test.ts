import { beforeEach, describe, expect, it, vi } from "vitest";
import { inboxes, onInboxSynced, resyncInbox, syncInbox } from "$lib/inbox/inboxStore.svelte";
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
