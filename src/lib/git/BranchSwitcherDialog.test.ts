import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import BranchSwitcherDialog from "./BranchSwitcherDialog.svelte";

const checkout = vi.fn();
const create = vi.fn();

vi.mock("$lib/services/api", () => ({
  isTauri: () => false,
  gitBranches: vi.fn(async () => [
    { name: "main", isRemote: false, isCurrent: true },
    { name: "feature", isRemote: false, isCurrent: false },
    { name: "origin/main", isRemote: true, isCurrent: false },
    { name: "origin/remote-only", isRemote: true, isCurrent: false },
  ]),
  gitCheckoutBranch: (...args: unknown[]) => checkout(...args),
  gitCreateBranch: (...args: unknown[]) => create(...args),
}));

beforeEach(() => {
  checkout.mockReset();
  create.mockReset();
});

describe("BranchSwitcherDialog", () => {
  it("switches to a local branch and closes with its name", async () => {
    checkout.mockResolvedValue(undefined);
    const close = vi.fn();
    render(BranchSwitcherDialog, { dir: "/repo", currentBranch: "main", close });

    await userEvent.click(await screen.findByRole("option", { name: /feature/ }));
    expect(checkout).toHaveBeenCalledWith("/repo", "feature", false);
    expect(close).toHaveBeenCalledWith("feature");
    // A remote-only branch is offered; one already local is not duplicated.
    expect(screen.getByRole("option", { name: /origin\/remote-only/ })).toBeTruthy();
    expect(screen.queryByRole("option", { name: /origin\/main/ })).toBeNull();
  });

  it("offers to switch anyway after a dirty-tree refusal", async () => {
    checkout
      .mockRejectedValueOnce(new Error("dirtyTree: This checkout has uncommitted changes."))
      .mockResolvedValueOnce(undefined);
    const close = vi.fn();
    render(BranchSwitcherDialog, { dir: "/repo", currentBranch: "main", close });

    await userEvent.click(await screen.findByRole("option", { name: /feature/ }));
    expect(await screen.findByText(/uncommitted changes/)).toBeTruthy();
    expect(close).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: "Switch anyway" }));
    expect(checkout).toHaveBeenLastCalledWith("/repo", "feature", true);
    expect(close).toHaveBeenCalledWith("feature");
  });

  it("creates a branch from a base and checks it out", async () => {
    create.mockResolvedValue(undefined);
    const close = vi.fn();
    render(BranchSwitcherDialog, { dir: "/repo", currentBranch: "main", close });

    await userEvent.click(screen.getByRole("tab", { name: "Create" }));
    await userEvent.type(screen.getByRole("textbox", { name: "New branch name" }), "feat/new");
    await userEvent.type(screen.getByRole("combobox", { name: "Base revision" }), "main");
    await userEvent.click(screen.getByRole("button", { name: "Create branch" }));
    expect(create).toHaveBeenCalledWith("/repo", "feat/new", "main", true);
    expect(close).toHaveBeenCalledWith("feat/new");
  });
});
