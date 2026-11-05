import { render, screen, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { commands } from "$lib/services/homeRouting";
import { resetPreviewCheckouts, setPreviewPrLink } from "$lib/services/preview/checkouts";
import type { PrLink } from "$lib/types";
import ThreadBranchChip from "./ThreadBranchChip.svelte";

vi.mock("$lib/services/homeRouting", async (importOriginal) => {
  const actual = await importOriginal<typeof import("$lib/services/homeRouting")>();
  return {
    ...actual,
    commands: {
      ...actual.commands,
      copyText: vi.fn(() => Promise.resolve(null)),
      reviewBranchProgress: vi.fn(() =>
        Promise.resolve({ startSha: "3c1e9a2".padEnd(40, "0"), tipSha: "9".repeat(40), ahead: 4, behind: 0 }),
      ),
    },
  };
});

const link: PrLink = {
  threadId: "t1",
  repoKey: "/Users/ciaran/Projects/codex-custom",
  prNumber: 415,
  mine: true,
  step: "noCheckout",
  checkoutPath: null,
  gitBranch: "feat/claude-driver",
  startSha: "3c1e9a2".padEnd(40, "0"),
  createdAt: 1,
  closedAt: null,
};

describe("ThreadBranchChip", () => {
  beforeEach(() => {
    resetPreviewCheckouts();
    setPreviewPrLink(link);
  });

  it("shows the start commit and how far the git branch moved, with its actions", async () => {
    render(ThreadBranchChip, { link });
    const chip = screen.getByTestId("thread-branch-chip");
    expect(chip.textContent).toContain("feat/claude-driver");
    expect(chip.textContent).toContain("@ 3c1e9a2");
    expect(await within(chip).findByText("+4")).toBeTruthy();

    await userEvent.click(screen.getByRole("button", { name: /Git branch menu|feat\/claude-driver/ }));
    const menu = await screen.findByRole("menu", { name: /Git branch/ });
    expect(within(menu).getByText("Thread started at 3c1e9a2 · the branch is now 4 commits ahead")).toBeTruthy();
    expect(within(menu).getByRole("menuitem", { name: "Check out start commit" }).getAttribute("aria-disabled")).toBe(
      "true",
    );

    await userEvent.click(within(menu).getByRole("menuitem", { name: "Copy commit" }));
    expect(commands.copyText).toHaveBeenCalledWith(link.startSha);
  });
});
