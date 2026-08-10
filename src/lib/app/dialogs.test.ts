import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { closeAllDialogs, dialogStack, openDialog, submitState } from "$lib/app/dialogs.svelte";
import DialogHost from "$lib/components/DialogHost.svelte";
import DeleteThreadDialog from "$lib/layout/DeleteThreadDialog.svelte";

describe("openDialog", () => {
  beforeEach(() => {
    closeAllDialogs();
  });

  it("resolves with the dialog's result", async () => {
    render(DialogHost);
    const result = openDialog(DeleteThreadDialog, { title: "Refactor the parser" });

    await userEvent.click(await screen.findByRole("button", { name: "Delete" }));
    expect(await result).toBe(true);
    expect(dialogStack).toHaveLength(0);
  });

  it("resolves null when the dialog is dismissed", async () => {
    render(DialogHost);
    const result = openDialog(DeleteThreadDialog, { title: "Refactor the parser" });

    await userEvent.click(await screen.findByRole("button", { name: "Cancel" }));
    expect(await result).toBeNull();
  });

  it("stacks dialogs and resolves each independently", async () => {
    const first = openDialog(DeleteThreadDialog, { title: "First" });
    const second = openDialog(DeleteThreadDialog, { title: "Second" });
    expect(dialogStack).toHaveLength(2);

    closeAllDialogs();
    expect(await first).toBeNull();
    expect(await second).toBeNull();
    expect(dialogStack).toHaveLength(0);
  });
});

describe("submitState", () => {
  it("reports failure inline instead of throwing", async () => {
    const action = submitState();

    expect(await action.run(async () => {})).toBe(true);
    expect(action.error).toBeNull();

    expect(
      await action.run(async () => {
        throw new Error("worktree already exists");
      }),
    ).toBe(false);
    expect(action.error).toBe("worktree already exists");
    expect(action.busy).toBe(false);
  });
});
