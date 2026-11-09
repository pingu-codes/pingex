import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { closeAllDialogs } from "$lib/app/dialogs.svelte";
import { view } from "$lib/app/navigation.svelte";
import DialogHost from "$lib/components/DialogHost.svelte";
import { linkExpectedThread, prLinks, resetPrLinks } from "$lib/review/prLinks.svelte";
import { commands } from "$lib/services/homeRouting";
import { resetPreviewCheckouts } from "$lib/services/preview/checkouts";
import { repositoryCommands } from "$lib/services/preview/repositories";
import type { CheckoutTarget, RepositorySettings } from "$lib/types";
import { createPrWorktree, stepUpFor, workOnPr } from "./checkoutSteps";

vi.mock("$lib/services/homeRouting", async (importOriginal) => {
  const actual = await importOriginal<typeof import("$lib/services/homeRouting")>();
  return {
    ...actual,
    commands: { ...actual.commands, reviewRunCheckoutSetup: vi.fn(actual.commands.reviewRunCheckoutSetup) },
  };
});

const repoDir = "/Users/ciaran/Projects/codex-custom";
const theirs: CheckoutTarget = {
  number: 418,
  headRef: "jo-r/maki-acp",
  headSha: "a".repeat(40),
  mine: false,
  stack: null,
  headRepo: "ciaran/codex-custom",
};

describe("stepping up for an action", () => {
  beforeEach(() => {
    resetPreviewCheckouts();
    resetPrLinks();
    closeAllDialogs();
  });

  it("asks to push to their branch even when a worktree has to be made first", async () => {
    render(DialogHost);
    const done = stepUpFor({ repoDir, repoName: "codex-custom", target: theirs, action: "push" });

    await userEvent.click(await screen.findByRole("button", { name: "Create worktree" }));
    await userEvent.click(await screen.findByRole("button", { name: "Push to their branch" }));

    expect((await done)?.step).toBe("worktree");
  });

  it("Work on this PR opens a draft in the worktree whose thread files under the PR", async () => {
    render(DialogHost);
    const done = workOnPr({ repoDir, repoName: "codex-custom", target: theirs, action: "workOnPr" });

    await userEvent.click(await screen.findByRole("button", { name: "Create worktree" }));
    const checkout = await done;

    expect(checkout?.gitBranch).toBe(`pr/${theirs.number}`);
    expect(checkout?.managed).toBe(true);
    expect(view.draftCwd).toBe(checkout?.path);

    await linkExpectedThread("t1", checkout?.path ?? "");
    expect(prLinks.byThread.t1).toMatchObject({ prNumber: theirs.number, gitBranch: `pr/${theirs.number}` });
    // Only the first thread: the next one started there is the user's own call.
    await linkExpectedThread("t2", checkout?.path ?? "");
    expect(prLinks.byThread.t2).toBeUndefined();
  });

  it("puts my own PR on its real branch", async () => {
    const checkout = await createPrWorktree(repoDir, { ...theirs, mine: true, headRepo: "ciaran/codex-custom" });
    expect(checkout.gitBranch).toBe(theirs.headRef);
    expect(checkout.managed).toBe(false);
  });

  it("waits for setup before anything starts in a new worktree", async () => {
    const settings = repositoryCommands.reviewRepositorySettings?.(repoDir) as RepositorySettings;
    repositoryCommands.reviewSaveRepositorySettings?.(repoDir, { ...settings, setupCommand: "bun install" });
    let finishSetup = () => {};
    vi.mocked(commands.reviewRunCheckoutSetup).mockImplementationOnce(
      () => new Promise<void>((resolve) => (finishSetup = resolve)) as never,
    );

    let created = false;
    const done = createPrWorktree(repoDir, theirs).then(() => (created = true));
    await vi.waitFor(() => expect(commands.reviewRunCheckoutSetup).toHaveBeenCalledWith(repoDir, theirs.number));
    expect(created).toBe(false);

    finishSetup();
    await done;
    expect(created).toBe(true);
    repositoryCommands.reviewSaveRepositorySettings?.(repoDir, { ...settings, setupCommand: null });
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
