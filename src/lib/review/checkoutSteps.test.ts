import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { closeAllDialogs } from "$lib/app/dialogs.svelte";
import DialogHost from "$lib/components/DialogHost.svelte";
import { commands } from "$lib/services/homeRouting";
import { resetPreviewCheckouts } from "$lib/services/preview/checkouts";
import type { CheckoutTarget } from "$lib/types";
import { stepUpFor } from "./checkoutSteps";

const repoDir = "/Users/ciaran/Projects/codex-custom";
const theirs: CheckoutTarget = {
  number: 418,
  headRef: "jo-r/maki-acp",
  headSha: "a".repeat(40),
  mine: false,
  stack: null,
};

describe("stepping up for an action", () => {
  beforeEach(() => {
    resetPreviewCheckouts();
    closeAllDialogs();
  });

  it("asks to push to their branch even when a worktree has to be made first", async () => {
    render(DialogHost);
    const done = stepUpFor({ repoDir, repoName: "codex-custom", target: theirs, action: "push" });

    await userEvent.click(await screen.findByRole("button", { name: "Create worktree" }));
    await userEvent.click(await screen.findByRole("button", { name: "Push to their branch" }));

    expect((await done)?.step).toBe("worktree");
  });

  it("makes no worktree when the push to their branch is cancelled", async () => {
    render(DialogHost);
    const done = stepUpFor({ repoDir, repoName: "codex-custom", target: theirs, action: "push" });

    await userEvent.click(await screen.findByRole("button", { name: "Create worktree" }));
    await screen.findByRole("button", { name: "Push to their branch" });
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));

    expect(await done).toBeNull();
    expect((await commands.reviewCheckoutState(repoDir, theirs)).step).toBe("noCheckout");
  });
});
