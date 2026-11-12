import { render, screen, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { closeAllDialogs } from "$lib/app/dialogs.svelte";
import DialogHost from "$lib/components/DialogHost.svelte";
import { resetPrLinks } from "$lib/review/prLinks.svelte";
import { checkOut, startPrThread, switchToPr } from "$lib/review/prOnDisk";
import { commands } from "$lib/services/homeRouting";
import { previewCommands } from "$lib/services/preview/commands";
import { desk, seedPreviewDesk } from "$lib/services/preview/desk";
import { deskRepository, inboxPr, prStack, worktreeEntry } from "$lib/services/preview/factories";
import { previewWorktrees } from "$lib/services/preview/fixtures";
import { toaster } from "$lib/toaster";

// Every call here reaches the preview desk's fakes (`services/preview/checkouts.ts`),
// which keep their worktrees in the preview's worktree listing: what the
// sidebar would show.

const HOME = "/home/me/.pingex";
const REPO = "/projects/desk";
const SLUG = "pingex/desk";

const lone = inboxPr({ number: 9, title: "Theirs", author: "jo-r", headRef: "jo-r/theirs" });
const mine = inboxPr({ number: 12, title: "Mine", author: "ciaran", headRef: "focus-rings", headRepo: SLUG });
const stack = prStack(7, [
  inboxPr({ number: 412, title: "Driver", author: "ciaran", headRef: "feat/driver", headRepo: SLUG }),
  inboxPr({ number: 415, title: "Claude", author: "ciaran", headRef: "feat/claude", headRepo: SLUG }),
  inboxPr({ number: 418, title: "Maki", author: "ciaran", headRef: "feat/maki", headRepo: SLUG }),
]);

const listed = (path: string) => previewWorktrees.find((entry) => entry.path === path);

beforeEach(() => {
  seedPreviewDesk({
    home: HOME,
    repositories: [
      deskRepository(SLUG, { repository: { path: REPO, key: REPO, name: "desk" }, prs: [lone, mine], stacks: [stack] }),
    ],
  });
  resetPrLinks();
  closeAllDialogs();
});

afterEach(() => {
  vi.restoreAllMocks();
  for (let index = previewWorktrees.length - 1; index >= 0; index -= 1) {
    if (previewWorktrees[index].path.startsWith("/by-hand")) previewWorktrees.splice(index, 1);
  }
});

describe("checking a PR out", () => {
  it("makes someone else's PR a worktree on pr/<n> that the sidebar lists, and Undo removes it", async () => {
    const undo = vi.spyOn(toaster, "create");

    const checkout = await checkOut(REPO, { pr: 9 });

    const path = `${HOME}/worktrees/desk/pr-9`;
    expect(checkout).toMatchObject({ number: 9, path, gitBranch: "pr/9", adopted: false, stack: null });
    expect(await commands.reviewPrCheckout(REPO, 9)).toMatchObject({ path });
    expect(listed(path)).toMatchObject({ branch: "pr/9", lockReason: "pingex pr worktree", isManagedPrWorktree: true });

    const toast = undo.mock.calls.map(([options]) => options).find((options) => options.action?.label === "Undo");
    expect(toast?.title).toBe("Checked out #9 in pr-9");
    await toast?.action?.onClick?.();
    await vi.waitFor(() => expect(listed(path)).toBeUndefined());
    expect(await commands.reviewPrCheckout(REPO, 9)).toBeNull();
  });

  it("puts my PR on its own git branch, and checking out again makes nothing new", async () => {
    const first = await checkOut(REPO, { pr: 12 });
    const again = await commands.reviewCheckOut(REPO, 12);

    expect(first?.gitBranch).toBe("focus-rings");
    expect(again).toMatchObject({ kind: "checkedOut", created: false, checkout: { path: first?.path } });
  });

  it("checks out the whole stack for any of its PRs, in one worktree on the top PR's branch", async () => {
    const checkout = await checkOut(REPO, { pr: 412 });

    const path = `${HOME}/worktrees/desk/feat-maki`;
    expect(checkout).toMatchObject({ number: 418, path, stack: 7, onPrBranch: true });
    const rows = await commands.reviewPrCheckouts(REPO);
    expect(rows.map((row) => [row.number, row.path, row.onPrBranch])).toEqual([
      [412, path, false],
      [415, path, false],
      [418, path, true],
    ]);
  });

  it("offers a branch checked out by hand for adoption, and Pingex then leaves it be", async () => {
    previewWorktrees.push(worktreeEntry({ path: "/by-hand/focus", branch: "focus-rings" }));
    render(DialogHost);
    const user = userEvent.setup();

    const done = checkOut(REPO, { pr: 12 });
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("/by-hand/focus")).toBeInTheDocument();
    expect(within(dialog).getByText("focus-rings")).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Adopt" }));
    const checkout = await done;

    expect(checkout).toMatchObject({ path: "/by-hand/focus", adopted: true });
    await commands.reviewRemovePrWorktree(REPO, 12);
    expect(listed("/by-hand/focus")).toBeDefined();
  });

  it("checks nothing out when the adoption is declined", async () => {
    previewWorktrees.push(worktreeEntry({ path: "/by-hand/driver", branch: "feat/driver" }));
    render(DialogHost);
    const user = userEvent.setup();

    const done = checkOut(REPO, { stack: 7 });
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));

    expect(await done).toBeNull();
    expect(await commands.reviewPrCheckouts(REPO)).toEqual([]);
  });
});

describe("switching a stack's worktree", () => {
  it("puts the worktree on the PR's branch once nothing runs there", async () => {
    await checkOut(REPO, { stack: 7 });

    const switched = await switchToPr(REPO, 412);

    expect(switched).toMatchObject({ number: 412, gitBranch: "feat/driver", onPrBranch: true });
    expect((await commands.reviewPrCheckout(REPO, 418))?.onPrBranch).toBe(false);
  });

  it("is refused on a worktree with changes, and on a PR that isn't checked out", async () => {
    await checkOut(REPO, { stack: 7 });
    for (const row of desk().checkouts.values()) row.dirty = true;
    const failure = vi.spyOn(toaster, "error");

    expect(await switchToPr(REPO, 412)).toBeNull();
    expect(await switchToPr(REPO, 9)).toBeNull();

    expect(failure.mock.calls.map(([options]) => options.description)).toEqual([
      expect.stringMatching(/uncommitted changes/),
      "Check out #9 first",
    ]);
  });
});

describe("starting a PR thread", () => {
  it("needs the PR checked out", async () => {
    await expect(startPrThread(REPO, 9, { prompt: [{ type: "text", text: "Fix it" }] })).rejects.toThrow(
      "Check out #9 first",
    );
  });

  it("runs in the worktree, writable, with its context, filed under the PR before its turn", async () => {
    const turn = vi.spyOn(previewCommands, "startTurn").mockImplementation(() => ({
      id: "turn-1",
      status: "inProgress",
      items: [],
    }));
    const started = vi.spyOn(previewCommands, "startThread");
    const linked = vi.spyOn(previewCommands, "reviewLinkThread");
    const path = (await checkOut(REPO, { pr: 9 }))?.path;

    const { thread, link } = await startPrThread(REPO, 9, { prompt: [{ type: "text", text: "Fix it" }] });

    expect(started).toHaveBeenCalledWith(path, null, null);
    expect(link).toMatchObject({ prNumber: 9, checkoutPath: path });
    expect(linked.mock.invocationCallOrder[0]).toBeLessThan(turn.mock.invocationCallOrder[0]);
    const [id, input, options] = turn.mock.calls[0];
    expect(id).toBe(thread.id);
    expect(input).toEqual([
      { type: "text", text: "Fix it" },
      expect.objectContaining({ text: expect.stringContaining("worktree for pull request #9") }),
    ]);
    expect(options).toMatchObject({ sandboxMode: "workspace-write" });
  });

  it("switches a stack's worktree to the PR first", async () => {
    vi.spyOn(previewCommands, "startTurn").mockImplementation(() => ({
      id: "turn-1",
      status: "inProgress",
      items: [],
    }));
    await checkOut(REPO, { stack: 7 });

    const { link } = await startPrThread(REPO, 415, { prompt: [{ type: "text", text: "Tidy the driver" }] });

    expect(link.gitBranch).toBe("feat/claude");
    expect((await commands.reviewPrCheckout(REPO, 415))?.onPrBranch).toBe(true);
  });
});
