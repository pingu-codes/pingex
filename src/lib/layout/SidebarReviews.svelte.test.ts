import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { menuAction } from "$lib/app/actions.svelte";
import { closeAllDialogs } from "$lib/app/dialogs.svelte";
import DialogHost from "$lib/components/DialogHost.svelte";
import Sidebar from "$lib/layout/Sidebar.svelte";
import { prLinks, resetPrLinks } from "$lib/review/prLinks.svelte";
import { resetPreviewCheckouts, setPreviewPrLink } from "$lib/services/preview/checkouts";
import { threadSummary } from "$lib/services/preview/factories";
import type { PrLink, Project } from "$lib/types";

vi.mock("$lib/services/homeRouting", async (importOriginal) => {
  const actual = await importOriginal<typeof import("$lib/services/homeRouting")>();
  return {
    ...actual,
    commands: {
      ...actual.commands,
      setProjectExpanded: vi.fn(() => Promise.resolve(null)),
      gitStatus: vi.fn(() => Promise.reject(new Error("not in tests"))),
    },
  };
});

const repo = "/Users/ciaran/Projects/codex-custom";

function link(threadId: string, overrides: Partial<PrLink> = {}): PrLink {
  return {
    threadId,
    repoKey: repo,
    prNumber: 126,
    mine: false,
    step: "noCheckout",
    checkoutPath: null,
    gitBranch: "octo/worktrees",
    startSha: "6".repeat(40),
    createdAt: 1,
    closedAt: null,
    ...overrides,
  };
}

function setup() {
  const project: Project = {
    path: repo,
    name: "codex-custom",
    kind: "folder",
    workspaceId: null,
    archived: false,
    instructions: "",
    sources: [],
    pinned: false,
    expanded: true,
    threads: [
      threadSummary({ id: "diff", title: "Diff review of #126", cwd: repo }),
      threadSummary({ id: "plain", title: "Plain thread", cwd: repo }),
      threadSummary({ id: "mine", title: "Address review on #128", cwd: repo }),
    ],
  };
  render(DialogHost);
  render(Sidebar, {
    projects: [project],
    account: null,
    selectedThread: null,
    loading: false,
    onAddProject: vi.fn(),
    onSelectThread: vi.fn(),
    onNewThread: vi.fn(),
    onOpenSettings: vi.fn(),
    onMenuAction: menuAction,
  });
}

describe("Sidebar Reviews group", () => {
  beforeEach(() => {
    resetPreviewCheckouts();
    resetPrLinks();
    closeAllDialogs();
    setPreviewPrLink(link("diff"));
    setPreviewPrLink(link("mine", { prNumber: 128, mine: true, step: "worktree", checkoutPath: "/wt/review" }));
  });

  it("files threads on other people's PRs under Reviews and keeps my worktree threads in place with a PR chip", async () => {
    setup();
    const reviews = await screen.findByTestId("reviews-group");

    expect(within(reviews).getByText("Diff review of #126")).toBeTruthy();
    expect(within(reviews).queryByText("Plain thread")).toBeNull();
    expect(within(reviews).queryByText("Address review on #128")).toBeNull();
    expect(screen.getAllByText("Diff review of #126")).toHaveLength(1);
    const mine = screen.getByText("Address review on #128").closest("button") as HTMLElement;
    expect(within(mine).getByTestId("pr-chip").textContent).toContain("#128");
  });

  it("moves a thread to another PR from its menu, keeping where it started", async () => {
    setup();
    const reviews = await screen.findByTestId("reviews-group");
    await userEvent.click(within(reviews).getByRole("button", { name: "Thread menu" }));
    await userEvent.click(await screen.findByRole("menuitem", { name: /Move to PR/ }));
    await userEvent.click(await screen.findByRole("button", { name: /#128/ }));

    await waitFor(() => expect(prLinks.byThread.diff?.prNumber).toBe(128));
    expect(prLinks.byThread.diff).toMatchObject({ mine: true, step: "noCheckout", startSha: "6".repeat(40) });
    // Now my PR without a worktree: still under Reviews, with the new number.
    await waitFor(() =>
      expect(within(screen.getByTestId("reviews-group")).getByTestId("pr-chip").textContent).toContain("#128"),
    );
  });
});
