import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import DialogHost from "$lib/components/DialogHost.svelte";
import { SCENARIO_DIR, seedInboxScenario } from "$lib/inbox/inboxScenario";
import { inboxes } from "$lib/inbox/inboxStore.svelte";
import { resetPrLinks } from "$lib/review/prLinks.svelte";
import { previewCommands } from "$lib/services/preview/commands";
import { resetFollowUps } from "$lib/stack/followUps.svelte";
import StackPage from "$lib/stack/StackPage.svelte";

// What runs when, and what each question asks, is `stackUnderWork.svelte.test.ts`;
// these check what the page draws and how its controls and dialogs reach the stack.
const MAIN = SCENARIO_DIR;
const spy = <K extends keyof typeof previewCommands>(name: K) => vi.spyOn(previewCommands, name);

function setup(props: { initialPr?: number } = {}) {
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
  seedInboxScenario();
  resetFollowUps();
  resetPrLinks();
  localStorage.clear();
  for (const key of Object.keys(inboxes)) delete inboxes[key];
});

afterEach(() => vi.restoreAllMocks());

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
    const detail = spy("reviewPrDetail");
    const user = setup();
    await loaded();
    const readsOf = (number: number) => detail.mock.calls.filter(([, read]) => read === number).length;
    await waitFor(() => expect(readsOf(418)).toBe(1));

    await user.click(node(418));
    await screen.findByRole("heading", { name: "maki over ACP" });
    expect(readsOf(418)).toBe(1);
  });

  it("asks before pushing someone else's stack, also from its worktree", async () => {
    const push = spy("reviewPushStack");
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
    expect(push).not.toHaveBeenCalled();
  });

  it("moves a git branch with ⌥↑ after confirming, and the tree shows the new order", async () => {
    const reorder = spy("reviewReorderStack");
    const user = setup();
    await loaded();

    node(415).focus();
    await user.keyboard("{Alt>}{ArrowUp}{/Alt}");
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("#415 feat/claude-driver onto feat/maki-acp")).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Reorder" }));

    await waitFor(() => expect(nodeOrder()).toEqual([415, 418, 412]));
    expect(reorder).toHaveBeenCalledWith(MAIN, expect.objectContaining({ number: 7 }), [412, 418, 415]);
  });

  it("moves nothing while dragging, and reorders on the drop", async () => {
    const reorder = spy("reviewReorderStack");
    const user = setup();
    await loaded();

    await fireEvent.dragStart(node(418));
    await fireEvent.dragOver(node(412));
    expect(screen.getByText("Drop to rebase onto #412 · retargets PR")).toBeInTheDocument();
    expect(nodeOrder()).toEqual([418, 415, 412]);
    expect(reorder).not.toHaveBeenCalled();

    await fireEvent.drop(node(412));
    await user.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Reorder" }));

    await waitFor(() => expect(nodeOrder()).toEqual([415, 418, 412]));
    expect(reorder).toHaveBeenCalledWith(MAIN, expect.anything(), [412, 418, 415]);
  });

  it("after a PR merges, offers to restack the rest and delete its git branch, until Later", async () => {
    seedInboxScenario({ merged: [412] });
    const user = setup();
    await waitFor(() => expect(screen.getByText("#412 merged into main")).toBeInTheDocument());
    expect(screen.getByText("Restack #415 and #418 onto main and delete feat/driver-trait?")).toBeInTheDocument();
    expect(nodeOrder()).toEqual([418, 415]);

    await user.click(screen.getByRole("button", { name: "Later" }));
    expect(screen.queryByText("#412 merged into main")).toBeNull();
  });
});

describe("opening on a PR", () => {
  it("opens on the PR it was asked for, with the stack beside it", async () => {
    setup({ initialPr: 412 });
    await loaded();

    expect(node(412)).toHaveAttribute("aria-pressed", "true");
    expect(node(415)).toHaveAttribute("aria-pressed", "false");
  });

  it("comes back to the PR shown last when not asked for one", async () => {
    const user = setup();
    await loaded();
    await user.click(node(418));

    cleanup();
    setup();
    await loaded();

    expect(node(418)).toHaveAttribute("aria-pressed", "true");
  });
});

describe("collapsing the stack", () => {
  it("folds the tree to a strip of its PRs, and back", async () => {
    const user = setup();
    await loaded();

    await user.click(screen.getByRole("button", { name: "Collapse stack" }));
    expect(screen.queryByRole("region", { name: "Stack tree" })).toBeNull();
    const strip = screen.getByRole("navigation", { name: "Stack branches" });
    expect(
      within(strip)
        .getAllByRole("button", { name: /^#\d+$/ })
        .map((button) => button.textContent?.trim()),
    ).toEqual(["#418", "#415", "#412"]);

    await user.click(within(strip).getByRole("button", { name: "#412" }));
    expect(within(strip).getByRole("button", { name: "#412" })).toHaveAttribute("aria-current", "true");
    await screen.findByRole("region", { name: "Pull request 412" });

    await user.click(screen.getByRole("button", { name: "Expand stack" }));
    expect(screen.queryByRole("navigation", { name: "Stack branches" })).toBeNull();
    expect(node(412)).toHaveAttribute("aria-pressed", "true");
  });

  it("stays folded on the next page that shows a stack", async () => {
    const user = setup();
    await loaded();
    await user.click(screen.getByRole("button", { name: "Collapse stack" }));

    cleanup();
    setup({ initialPr: 412 });

    expect(await screen.findByRole("navigation", { name: "Stack branches" })).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Stack tree" })).toBeNull();
  });

  it("unfolds for a restack started from the header, whose progress is in the tree", async () => {
    const user = setup();
    await loaded();
    await user.click(screen.getByRole("button", { name: "Collapse stack" }));

    await user.click(screen.getByRole("button", { name: /Restack onto/ }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("Restack onto main")).toBeInTheDocument();
    expect(within(dialog).getByText(/rewrites pushed commits on #412, #415, #418/)).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Restack" }));

    const progress = await screen.findByLabelText("Restack progress");
    expect(screen.getByRole("region", { name: "Stack tree" })).toBeInTheDocument();
    expect(within(progress).getByText(/Conflict in driver\.rs/)).toBeInTheDocument();
    expect(within(progress).getByText("Waiting")).toBeInTheDocument();
    expect(within(progress).getByRole("button", { name: "Resolve in a thread" })).toBeInTheDocument();
  });
});

describe("a PR that no longer sits on the head of the one below", () => {
  it("says so on its git branch, and the restack preview loads though the base has not moved", async () => {
    seedInboxScenario({ behind: 0, drifted: [418] });
    setup();

    expect(await screen.findByText("The stack is on its tip")).toBeInTheDocument();
    await waitFor(() => expect(within(node(418)).getByText("Behind #415")).toBeInTheDocument());
    expect(within(node(415)).queryByText(/^Behind /)).not.toBeInTheDocument();
    expect(await screen.findByText(/Restack rebases all 3 git branches/)).toBeInTheDocument();
  });
});
