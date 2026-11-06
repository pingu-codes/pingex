import { fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import DialogHost from "$lib/components/DialogHost.svelte";
import { inboxes } from "$lib/inbox/inboxStore.svelte";
import { startStackReview } from "$lib/review/agentReviews.svelte";
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
      reviewPrDetail: vi.fn(actual.commands.reviewPrDetail),
    },
  };
});

vi.mock("$lib/review/agentReviews.svelte", async (importOriginal) => {
  const actual = await importOriginal<typeof import("$lib/review/agentReviews.svelte")>();
  return { ...actual, startStackReview: vi.fn(async () => ({})) };
});

const MAIN = "/Users/ciaran/Projects/codex-custom";

function setup() {
  render(DialogHost);
  render(StackPage, {
    repoDir: MAIN,
    repoName: "codex-custom",
    stackNumber: 7,
    onBack: vi.fn(),
    onAskCodex: vi.fn(),
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

  it("warms the PRs either side of the selected one, so a click shows its review from the cache", async () => {
    const user = setup();
    await loaded();
    const readsOf = (number: number) =>
      vi.mocked(commands.reviewPrDetail).mock.calls.filter(([, read]) => read === number).length;
    await waitFor(() => expect(readsOf(418)).toBe(1));

    await user.click(node(418));
    await screen.findByRole("heading", { name: "maki over ACP" });
    expect(readsOf(418)).toBe(1);
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

  it("after a restack finishes, pushes the stack from its worktree without asking for another", async () => {
    const user = setup();
    await loaded();
    await user.click(screen.getByRole("button", { name: /Restack onto main/ }));
    await user.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Restack" }));
    const progress = await screen.findByLabelText("Restack progress");
    await user.click(within(progress).getByRole("button", { name: "Continue" }));
    await waitFor(() => expect(within(progress).getByText("Restacked onto main")).toBeInTheDocument());

    const header = screen.getByRole("heading", { name: "driver-trait" }).parentElement as HTMLElement;
    const push = within(header).getByRole("button", { name: "Push stack" });
    expect(push).toBeEnabled();
    expect(within(header).getByRole("button", { name: /Restack onto main/ })).toBeEnabled();
    await user.click(push);

    await waitFor(() => expect(commands.reviewPushStack).toHaveBeenCalledOnce());
    expect(screen.queryByRole("dialog")).toBeNull();
    await waitFor(() => expect(screen.queryByLabelText("Restack progress")).toBeNull());
  });

  it("asks before pushing someone else's stack, also from its worktree", async () => {
    const user = setup();
    await loaded();
    const repo = inboxes[MAIN];
    if (repo?.inbox) repo.inbox.viewer = "jo-r";

    const header = screen.getByRole("heading", { name: "driver-trait" }).parentElement as HTMLElement;
    await user.click(within(header).getByRole("button", { name: "Push stack" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("Push to feat/driver-trait?")).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));

    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(commands.reviewPushStack).not.toHaveBeenCalled();
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

  it("queues one agent review over the stack's open PRs", async () => {
    const user = setup();
    await loaded();

    await user.click(screen.getByRole("button", { name: /Review whole stack/ }));

    await waitFor(() =>
      expect(startStackReview).toHaveBeenCalledWith(
        MAIN,
        [
          expect.objectContaining({ number: 412 }),
          expect.objectContaining({ number: 415 }),
          expect.objectContaining({ number: 418 }),
        ],
        { viewer: "ciaran" },
      ),
    );
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
