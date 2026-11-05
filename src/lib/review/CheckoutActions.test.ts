import { render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { closeAllDialogs } from "$lib/app/dialogs.svelte";
import DialogHost from "$lib/components/DialogHost.svelte";
import { commands } from "$lib/services/homeRouting";
import { resetPreviewCheckouts } from "$lib/services/preview/checkouts";
import ToastHost from "$lib/ToastHost.svelte";
import { toaster } from "$lib/toaster";
import type { CheckoutTarget } from "$lib/types";
import CheckoutActions from "./CheckoutActions.svelte";

const repoDir = "/Users/ciaran/Projects/codex-custom";
const target: CheckoutTarget = {
  number: 409,
  headRef: "jo-r/usage-meter",
  headSha: "4".repeat(40),
  mine: false,
  stack: null,
};

function mount() {
  const onRunLocally = vi.fn();
  render(DialogHost);
  render(ToastHost);
  render(CheckoutActions, { props: { repoDir, repoName: "codex-custom", target, onRunLocally } });
  return onRunLocally;
}

describe("CheckoutActions", () => {
  beforeEach(() => {
    resetPreviewCheckouts();
    closeAllDialogs();
    toaster.remove();
  });

  it("names the step each button creates on a PR with no checkout", async () => {
    mount();
    expect(await screen.findByRole("button", { name: "Run locally · Creates a review checkout" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Open in editor · Creates a review checkout" })).toBeTruthy();
    expect(screen.getByTestId("checkout-step").textContent).toContain("No checkout");
  });

  it("asks first, creates a review checkout with an Undo toast, and Undo removes it", async () => {
    const onRunLocally = mount();
    await userEvent.click(await screen.findByRole("button", { name: /Run locally/ }));

    expect(await screen.findByText("Create a review checkout of #409?")).toBeTruthy();
    await userEvent.click(screen.getByRole("switch", { name: "Don't ask again for codex-custom" }));
    await userEvent.click(screen.getByRole("button", { name: "Create review checkout" }));

    const path = "/Users/ciaran/.codex/review-checkouts/codex-custom/pr-409";
    await waitFor(() => expect(onRunLocally).toHaveBeenCalledWith(path));
    expect(screen.getByTestId("checkout-step").textContent).toContain("Review checkout");
    expect(await screen.findByText("Installing dependencies in pr-409")).toBeTruthy();
    expect((await commands.reviewRepositorySettings(repoDir)).skipCheckoutConfirm).toBe(true);

    await userEvent.click(await screen.findByRole("button", { name: "Undo" }));
    await waitFor(async () => expect((await commands.reviewCheckoutState(repoDir, target)).step).toBe("noCheckout"));
  });

  it("does nothing when the confirmation is cancelled", async () => {
    const onRunLocally = mount();
    await userEvent.click(await screen.findByRole("button", { name: /Open in editor/ }));
    await userEvent.click(await screen.findByRole("button", { name: "Cancel" }));

    expect(onRunLocally).not.toHaveBeenCalled();
    expect((await commands.reviewCheckoutState(repoDir, target)).step).toBe("noCheckout");
  });
});
