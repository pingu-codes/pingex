import { fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import DialogHost from "$lib/components/DialogHost.svelte";
import { inboxes } from "$lib/inbox/inboxStore.svelte";
import { resetPrLinks } from "$lib/review/prLinks.svelte";
import { commands } from "$lib/services/homeRouting";
import { deskLiveRepo, resetDeskActivity } from "$lib/services/preview/desk";
import { resetPreviewStacks } from "$lib/services/preview/stack";
import { resetFollowUps } from "$lib/stack/followUps.svelte";
import StackPage from "$lib/stack/StackPage.svelte";

vi.mock("$lib/services/homeRouting", async (importOriginal) => {
  const actual = await importOriginal<typeof import("$lib/services/homeRouting")>();
  return {
    ...actual,
    commands: {
      ...actual.commands,
      reviewRestack: vi.fn(actual.commands.reviewRestack),
      reviewAbortRestack: vi.fn(actual.commands.reviewAbortRestack),
      reviewReorderStack: vi.fn(actual.commands.reviewReorderStack),
      reviewPushStack: vi.fn(actual.commands.reviewPushStack),
    },
  };
});

const MAIN = "/Users/ciaran/Projects/codex-custom";

function setup(props: { onReviewStack?: (repoDir: string, prs: { number: number }[]) => void } = {}) {
  render(DialogHost);
  render(StackPage, {
    repoDir: MAIN,
    repoName: "codex-custom",
    stackNumber: 7,
    onBack: vi.fn(),
    onAskCodex: vi.fn(),
    ...props,
  });
  return userEvent.setup();
}

const tree = () => screen.getByRole("list", { name: "Git branches, top first" });
const node = (number: number) => within(tree()).getByRole("button", { name: new RegExp(`^#${number} `) });
const nodeOrder = () =>
  within(tree())
    .getAllByRole("button", { name: /^#\d+ / })
    .map((element) => Number(element.getAttribute("aria-label")?.match(/^#(\d+)/)?.[1]));

async function loaded() {
  await waitFor(() => expect(screen.getByText(/3 commits ahead of stack base/)).toBeInTheDocument());
}

beforeEach(() => {
  resetPreviewStacks();
  resetFollowUps();
  resetDeskActivity();
  resetPrLinks();
  localStorage.clear();
  for (const key of Object.keys(inboxes)) delete inboxes[key];
  vi.clearAllMocks();
});

describe("Stack page", () => {
  it("draws the git branch tree from the top PR down to the base, with the worktree", async () => {
    setup();
    await loaded();

    expect(nodeOrder()).toEqual([418, 415, 412]);
    expect(within(node(418)).getByText("Draft")).toBeInTheDocument();
    expect(within(node(415)).getByText("Changes requested")).toBeInTheDocument();
    expect(within(node(415)).getByText("7 commits")).toBeInTheDocument();
    expect(within(node(415)).getByText("Checked out")).toBeInTheDocument();
    expect(within(node(412)).getByText("Approved")).toBeInTheDocument();
    expect(screen.getByText("refactor: move to bun")).toBeInTheDocument();
    expect(await screen.findByText(/One conflict expected in/)).toBeInTheDocument();
    const worktree = screen.getByLabelText("Worktree harness-stack");
    expect(within(worktree).getByText("Clean")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "driver-trait" })).toBeInTheDocument();
    expect(screen.getByText(/3 PRs · 14 commits/)).toBeInTheDocument();
  });

  it("previews the restack, stops at the conflict, and Abort puts everything back", async () => {
    const user = setup();
    await loaded();

    await user.click(screen.getByRole("button", { name: /Restack onto main/ }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("Restack onto main")).toBeInTheDocument();
    expect(within(dialog).getByText(/Conflict in driver\.rs/)).toBeInTheDocument();
    expect(within(dialog).getByText(/rewrites pushed commits on #412, #415, #418/)).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Restack" }));

    const progress = await screen.findByLabelText("Restack progress");
    expect(within(progress).getByText(/3 commits replayed/)).toBeInTheDocument();
    expect(within(progress).getByText(/Conflict in driver\.rs/)).toBeInTheDocument();
    expect(within(progress).getByText("Waiting")).toBeInTheDocument();
    expect(within(progress).getByRole("button", { name: "Resolve in a thread" })).toBeInTheDocument();
    expect(within(progress).getByRole("button", { name: "Open in editor" })).toBeInTheDocument();

    await user.click(within(progress).getByRole("button", { name: "Abort restack" }));
    await waitFor(() => expect(screen.queryByLabelText("Restack progress")).toBeNull());
    expect(commands.reviewAbortRestack).toHaveBeenCalledWith(MAIN, 7);
  });

  it("continues a resolved restack to the top", async () => {
    const user = setup();
    await loaded();
    await user.click(screen.getByRole("button", { name: /Restack onto main/ }));
    await user.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Restack" }));
    const progress = await screen.findByLabelText("Restack progress");

    await user.click(within(progress).getByRole("button", { name: "Continue" }));

    await waitFor(() => expect(within(progress).getByText("Restacked onto main")).toBeInTheDocument());
    expect(within(progress).queryByText("Waiting")).toBeNull();
  });

  it("moves a git branch with ⌥↑ after confirming, and the tree shows the new order", async () => {
    const user = setup();
    await loaded();

    node(415).focus();
    await user.keyboard("{Alt>}{ArrowUp}{/Alt}");
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("#415 feat/claude-driver onto feat/maki-acp")).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Reorder" }));

    await waitFor(() => expect(nodeOrder()).toEqual([415, 418, 412]));
    expect(commands.reviewReorderStack).toHaveBeenCalledWith(
      MAIN,
      expect.objectContaining({ number: 7 }),
      [412, 418, 415],
    );
  });

  it("moves nothing while dragging, and reorders on the drop", async () => {
    const user = setup();
    await loaded();

    await fireEvent.dragStart(node(418));
    await fireEvent.dragOver(node(412));
    expect(screen.getByText("Drop to rebase onto #412 · retargets PR")).toBeInTheDocument();
    expect(nodeOrder()).toEqual([418, 415, 412]);
    expect(commands.reviewReorderStack).not.toHaveBeenCalled();

    await fireEvent.drop(node(412));
    await user.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Reorder" }));

    await waitFor(() => expect(nodeOrder()).toEqual([415, 418, 412]));
    expect(commands.reviewReorderStack).toHaveBeenCalledWith(MAIN, expect.anything(), [412, 418, 415]);
  });

  it("leaves Review whole stack off until it is wired, then hands it the open PRs", async () => {
    setup();
    await loaded();
    expect(screen.getByRole("button", { name: /Review whole stack/ })).toBeDisabled();
  });

  it("hands the open PRs to Review whole stack", async () => {
    const onReviewStack = vi.fn();
    const user = setup({ onReviewStack });
    await loaded();

    await user.click(screen.getByRole("button", { name: /Review whole stack/ }));

    expect(onReviewStack).toHaveBeenCalledWith(MAIN, [
      expect.objectContaining({ number: 412 }),
      expect.objectContaining({ number: 415 }),
      expect.objectContaining({ number: 418 }),
    ]);
  });

  it("after a PR merges, offers to restack the rest and delete its git branch, until Later", async () => {
    const stack = deskLiveRepo(MAIN).stacks.stacks.find((candidate) => candidate.number === 7);
    if (!stack) throw new Error("fixture stack missing");
    const saved = structuredClone(stack.prs);
    stack.prs[0] = { ...stack.prs[0], merged: true, state: "closed" };
    try {
      const user = setup();
      await waitFor(() => expect(screen.getByText("#412 merged into main")).toBeInTheDocument());
      expect(screen.getByText("Restack #415 and #418 onto main and delete feat/driver-trait?")).toBeInTheDocument();
      expect(nodeOrder()).toEqual([418, 415]);

      await user.click(screen.getByRole("button", { name: "Later" }));
      expect(screen.queryByText("#412 merged into main")).toBeNull();
    } finally {
      stack.prs = saved;
    }
  });
});
