import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { worktreeEntry } from "$lib/services/preview/factories";
import RemoveWorktreeDialog from "$lib/worktrees/RemoveWorktreeDialog.svelte";

describe("RemoveWorktreeDialog", () => {
  it("removes a clean worktree without typed confirmation", async () => {
    const close = vi.fn();
    render(RemoveWorktreeDialog, { entry: worktreeEntry(), threadCount: 1, close });
    const remove = screen.getByRole("button", { name: "Remove" });
    expect(remove).toBeEnabled();
    await userEvent.click(remove);
    expect(close).toHaveBeenCalledWith({ force: false });
  });

  it("gates force-remove of a dirty worktree behind the typed folder name", async () => {
    const close = vi.fn();
    const dirty = worktreeEntry({ status: { staged: 0, unstaged: 2, untracked: 0, conflicted: 0 } });
    render(RemoveWorktreeDialog, { entry: dirty, threadCount: 0, close });

    const force = screen.getByRole("button", { name: "Force remove" });
    expect(force).toBeDisabled();

    const field = screen.getByLabelText("Type the worktree folder name to confirm");
    await userEvent.type(field, "wrong-name");
    expect(force).toBeDisabled();

    await userEvent.clear(field);
    await userEvent.type(field, "wt-feature");
    expect(force).toBeEnabled();

    await userEvent.click(force);
    expect(close).toHaveBeenCalledWith({ force: true });
  });
});
