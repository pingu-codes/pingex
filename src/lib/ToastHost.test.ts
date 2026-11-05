import { fireEvent, render, screen } from "@testing-library/svelte";
import { describe, expect, it, vi } from "vitest";
import ToastHost from "./ToastHost.svelte";
import { toastUndo } from "./toaster";

describe("ToastHost", () => {
  it("shows an Undo button that runs the undo", async () => {
    render(ToastHost);
    const undo = vi.fn();
    toastUndo("Created review checkout pr-409", undo, "From jo-r/usage-meter");

    await fireEvent.click(await screen.findByRole("button", { name: "Undo" }));

    expect(undo).toHaveBeenCalledOnce();
  });
});
