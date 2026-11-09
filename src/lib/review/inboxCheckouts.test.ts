import { render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { closeAllDialogs } from "$lib/app/dialogs.svelte";
import { view } from "$lib/app/navigation.svelte";
import DialogHost from "$lib/components/DialogHost.svelte";
import { resetPrLinks } from "$lib/review/prLinks.svelte";
import { commands } from "$lib/services/homeRouting";
import { resetPreviewCheckouts } from "$lib/services/preview/checkouts";
import { inboxPr } from "$lib/services/preview/factories";
import { checkoutHandlers } from "./inboxCheckouts";

vi.mock("$lib/services/homeRouting", async (importOriginal) => {
  const actual = await importOriginal<typeof import("$lib/services/homeRouting")>();
  return {
    ...actual,
    commands: {
      ...actual.commands,
      startThread: vi.fn(actual.commands.startThread),
      startTurn: vi.fn(actual.commands.startTurn),
      reviewCheckLogs: vi.fn(() =>
        Promise.resolve([{ name: "test:changed", log: "expected 2, got 3", truncated: false }]),
      ),
    },
  };
});

const repoDir = "/Users/ciaran/Projects/codex-custom";
const pr = inboxPr({ number: 418, title: "maki ACP driver", headRef: "feat/maki-acp" });
const handlers = checkoutHandlers(() => "codex-custom");

describe("inbox checkout handlers", () => {
  beforeEach(() => {
    resetPreviewCheckouts();
    resetPrLinks();
    closeAllDialogs();
    vi.mocked(commands.startThread).mockClear();
  });

  it("Fix CI creates a worktree after asking, then starts a thread there seeded with the failing log", async () => {
    render(DialogHost);
    const done = handlers.fixCi(repoDir, pr);
    await userEvent.click(await screen.findByRole("button", { name: "Create worktree" }));
    await done;

    const path = "/Users/ciaran/.codex/pr-worktrees/codex-custom/pr-418";
    await waitFor(() => expect(commands.startThread).toHaveBeenCalledWith(path, null, null));
    const input = vi.mocked(commands.startTurn).mock.calls.at(-1)?.[1] as { text: string }[];
    expect(input[0].text).toContain("expected 2, got 3");
    expect((await commands.reviewPrLinks()).map((link) => link.prNumber)).toEqual([418]);
  });

  it("Work on this PR makes a worktree, then opens a draft there", async () => {
    render(DialogHost);
    const done = handlers.newThread(repoDir, pr);
    await userEvent.click(await screen.findByRole("button", { name: "Create worktree" }));
    await done;

    expect(view.draftCwd).toBe("/Users/ciaran/.codex/pr-worktrees/codex-custom/pr-418");
  });

  it("Create worktrees skips a PR that already has one", async () => {
    await handlers.createWorktrees(repoDir, [pr]);
    const first = await commands.reviewCheckoutState(repoDir, { ...pr, mine: false, stack: null });
    await handlers.createWorktrees(repoDir, [pr]);
    expect(first.step).toBe("worktree");
  });
});
