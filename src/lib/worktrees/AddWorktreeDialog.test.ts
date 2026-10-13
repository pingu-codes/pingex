import { render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { worktreeEntry } from "$lib/services/preview/factories";
import AddWorktreeDialog from "$lib/worktrees/AddWorktreeDialog.svelte";

vi.mock("$lib/services/homeRouting", () => ({
  commands: {
    gitWorktrees: vi.fn(async () => [worktreeEntry({ path: "/repo", branch: "main", isMain: true }), worktreeEntry()]),
  },
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
