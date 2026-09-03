import { render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { WorktreeEntry } from "$lib/types";
import AddWorktreeDialog from "$lib/worktrees/AddWorktreeDialog.svelte";

function entry(overrides: Partial<WorktreeEntry> = {}): WorktreeEntry {
  return {
    path: "/repo/wt-feature",
    head: "abcdef1",
    branch: "feature",
    detached: false,
    bare: false,
    locked: false,
    lockReason: null,
    prunable: false,
    prunableReason: null,
    isMain: false,
    isCodexManaged: false,
    missingDir: false,
    branchCheckedOutElsewhere: false,
    upstream: null,
    ahead: 0,
    behind: 0,
    status: { staged: 0, unstaged: 0, untracked: 0, conflicted: 0 },
    state: null,
    ...overrides,
  };
}

vi.mock("$lib/services/api", () => ({
  isTauri: () => false,
  gitWorktrees: vi.fn(async () => [entry({ path: "/repo", branch: "main", isMain: true }), entry()]),
}));

describe("AddWorktreeDialog", () => {
  it("offers only linked worktrees and adopts the chosen one", async () => {
    const submit = vi.fn(async () => {});
    const close = vi.fn();
    render(AddWorktreeDialog, { repoDir: "/repo", projects: [], submit, close });

    const option = await screen.findByRole("option", { name: /wt-feature/ });
    expect(screen.queryByRole("option", { name: /main/ })).toBeNull();

    const add = screen.getByRole("button", { name: "Add worktree" });
    expect(add).toBeDisabled();
    await userEvent.click(option);
    expect(add).toBeEnabled();
    await userEvent.click(add);

    expect(submit).toHaveBeenCalledWith("/repo/wt-feature");
    await waitFor(() => expect(close).toHaveBeenCalledWith(true));
  });

  it("keeps the dialog open and shows the error when adopting fails", async () => {
    const submit = vi.fn(async () => {
      throw new Error("/repo is the main working tree, not a linked worktree");
    });
    const close = vi.fn();
    render(AddWorktreeDialog, { repoDir: "/repo", projects: [], submit, close });

    await userEvent.click(await screen.findByRole("option", { name: /wt-feature/ }));
    await userEvent.click(screen.getByRole("button", { name: "Add worktree" }));

    expect(await screen.findByText(/main working tree/)).toBeTruthy();
    expect(close).not.toHaveBeenCalled();
  });
});
