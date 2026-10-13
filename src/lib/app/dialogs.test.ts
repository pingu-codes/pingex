import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { closeAllDialogs, dialogStack, openDialog } from "$lib/app/dialogs.svelte";
import ConfirmDialog from "$lib/components/ConfirmDialog.svelte";
import DialogHost from "$lib/components/DialogHost.svelte";

const confirm = (title: string) => ({ title, body: "This cannot be undone.", confirmLabel: "Delete" });

describe("openDialog", () => {
  beforeEach(() => {
    closeAllDialogs();
  });

  it("resolves with the dialog's result", async () => {
    render(DialogHost);
    const result = openDialog(ConfirmDialog, confirm("Refactor the parser"));

    await userEvent.click(await screen.findByRole("button", { name: "Delete" }));
    expect(await result).toBe(true);
    expect(dialogStack).toHaveLength(0);
  });

  it("resolves null when the dialog is dismissed", async () => {
    render(DialogHost);
    const result = openDialog(ConfirmDialog, confirm("Refactor the parser"));

    await userEvent.click(await screen.findByRole("button", { name: "Cancel" }));
    expect(await result).toBeNull();
  });

  it("stacks dialogs and resolves each independently", async () => {
    const first = openDialog(ConfirmDialog, confirm("First"));
    const second = openDialog(ConfirmDialog, confirm("Second"));
    expect(dialogStack).toHaveLength(2);

    closeAllDialogs();
    expect(await first).toBeNull();
    expect(await second).toBeNull();
    expect(dialogStack).toHaveLength(0);
  });
});
