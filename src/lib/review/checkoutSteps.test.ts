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
import { previewStackWorktree, resetPreviewStacks, setPreviewStackWorktree } from "$lib/services/preview/stack";
import type { CheckoutTarget, RepositorySettings, StackInput } from "$lib/types";
import { checkOutStack, createPrWorktree, stepUpFor, workOnPr } from "./checkoutSteps";

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

const stackInput: StackInput = {
  number: 8,
  name: "focus-rings",
  baseRef: "main",
  baseSha: "b".repeat(40),
  prs: [
    { number: 401, title: "Rings", headRef: "jo-r/focus-rings", headSha: "1".repeat(40), open: true, merged: false },
    { number: 403, title: "Trap", headRef: "jo-r/focus-trap", headSha: "2".repeat(40), open: true, merged: false },
  ],
};
const inStack = (number: number, headRef: string): CheckoutTarget => ({
  number,
  headRef,
  headSha: "1".repeat(40),
  mine: false,
  stack: "focus-rings",
  headRepo: "ciaran/codex-custom",
  stackBranches: ["jo-r/focus-rings", "jo-r/focus-trap"],
});
const top = inStack(403, "jo-r/focus-trap");
const lower = inStack(401, "jo-r/focus-rings");

describe("a stack's worktree", () => {
  beforeEach(() => {
    resetPreviewCheckouts();
    resetPreviewStacks();
    resetPrLinks();
    closeAllDialogs();
  });

  it("checking out a stack asks, then makes one worktree for all of it", async () => {
    render(DialogHost);
    const done = checkOutStack({ repoDir, repoName: "codex-custom", input: stackInput, target: top });

    await userEvent.click(await screen.findByRole("button", { name: "Check out stack" }));
    const result = await done;

    expect(result?.created).toBe(true);
    expect(result?.checkout.gitBranch).toBe("jo-r/focus-trap");
    expect(previewStackWorktree(repoDir, 401)?.path).toBe(result?.checkout.path);
  });

  it("cancelling makes nothing", async () => {
    render(DialogHost);
    const done = checkOutStack({ repoDir, repoName: "codex-custom", input: stackInput, target: top });

    await userEvent.click(await screen.findByRole("button", { name: "Cancel" }));

    expect(await done).toBeNull();
    expect(previewStackWorktree(repoDir, 401)).toBeNull();
  });

  it("a stack that already has a worktree isn't asked about or moved", async () => {
    const path = "/wt/stack";
    setPreviewStackWorktree(repoDir, 401, { path, branch: "jo-r/focus-rings", clean: true });
    render(DialogHost);

    const result = await checkOutStack({ repoDir, repoName: "codex-custom", input: stackInput, target: top });

    expect(result?.created).toBe(false);
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(previewStackWorktree(repoDir, 401)?.branch).toBe("jo-r/focus-rings");
  });

  it("working on a PR whose branch isn't checked out asks, then switches the worktree", async () => {
    const path = "/wt/stack";
    setPreviewStackWorktree(repoDir, 401, { path, branch: "jo-r/focus-trap", clean: true });
    render(DialogHost);
    const done = stepUpFor({ repoDir, repoName: "codex-custom", target: lower, action: "workOnPr" });

    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent("jo-r/focus-trap");
    expect(dialog).toHaveTextContent("Threads running in this worktree will see the change");
    await userEvent.click(await screen.findByRole("button", { name: "Switch worktree" }));
    const checkout = await done;

    expect(checkout).toMatchObject({ path, gitBranch: "jo-r/focus-rings", onPrBranch: true });
    expect(previewStackWorktree(repoDir, 401)?.branch).toBe("jo-r/focus-rings");
  });

  it("cancelling the switch leaves the worktree where it is", async () => {
    setPreviewStackWorktree(repoDir, 401, { path: "/wt/stack", branch: "jo-r/focus-trap", clean: true });
    render(DialogHost);
    const done = stepUpFor({ repoDir, repoName: "codex-custom", target: lower, action: "workOnPr" });

    await userEvent.click(await screen.findByRole("button", { name: "Cancel" }));

    expect(await done).toBeNull();
    expect(previewStackWorktree(repoDir, 401)?.branch).toBe("jo-r/focus-trap");
  });

  it("a read needs no switch", async () => {
    setPreviewStackWorktree(repoDir, 401, { path: "/wt/stack", branch: "jo-r/focus-trap", clean: true });
    render(DialogHost);

    const checkout = await stepUpFor({ repoDir, repoName: "codex-custom", target: lower, action: "read" });

    expect(checkout?.onPrBranch).toBe(false);
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});

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
