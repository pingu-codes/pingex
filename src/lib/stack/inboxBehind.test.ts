import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import DialogHost from "$lib/components/DialogHost.svelte";
import type { InboxHandlers } from "$lib/inbox/handlers";
import { SCENARIO_DIR, seedInboxScenario } from "$lib/inbox/inboxScenario";
import { inboxes } from "$lib/inbox/inboxStore.svelte";
import PullRequestsView from "$lib/inbox/PullRequestsView.svelte";

const MAIN = SCENARIO_DIR;

beforeEach(() => {
  seedInboxScenario();
  localStorage.clear();
  for (const key of Object.keys(inboxes)) delete inboxes[key];
});

function setup(handlers: Partial<InboxHandlers> = {}) {
  render(DialogHost);
  const all: InboxHandlers = {
    openPr: vi.fn(),
    newThread: vi.fn(),
    startAgentReviews: vi.fn(),
    checkOut: vi.fn(),
    fixCi: vi.fn(),
    openStack: vi.fn(),
    restack: vi.fn(),
    checkOutStack: vi.fn(),
    ...handlers,
  };
  render(PullRequestsView, {
    repos: [{ path: MAIN, name: "codex-custom" }],
    repoPath: MAIN,
    onSelectRepo: vi.fn(),
    handlers: all,
  });
  return { user: userEvent.setup(), handlers: all };
}

describe("my stack's card in the inbox", () => {
  it("says how far behind its base it is, and the badge offers Restack and Compare", async () => {
    const restack = vi.fn();
    const { user } = setup({ restack, openStack: vi.fn() });

    const badge = await screen.findByRole("button", { name: "Behind main by 3" });
    await user.click(badge);
    const menu = await screen.findByRole("menu", { name: "Behind main by 3" });
    expect(within(menu).getByRole("menuitem", { name: "Compare" })).toBeInTheDocument();
    await user.click(within(menu).getByRole("menuitem", { name: "Restack" }));

    await waitFor(() => expect(restack).toHaveBeenCalledWith(MAIN, expect.objectContaining({ name: "driver-trait" })));
  });

  it("a click on the header opens the stack, but its buttons keep their own actions", async () => {
    const openStack = vi.fn();
    const restack = vi.fn();
    const { user } = setup({ restack, openStack });

    await user.click(await screen.findByRole("button", { name: "Restack" }));
    expect(restack).toHaveBeenCalledTimes(1);
    expect(openStack).not.toHaveBeenCalled();

    await user.click(screen.getByRole("button", { name: "Open stack driver-trait" }));
    expect(openStack).toHaveBeenCalledWith(MAIN, expect.objectContaining({ name: "driver-trait" }));
  });

  it("Compare shows what the base gained since the stack forked", async () => {
    const { user } = setup({ restack: vi.fn() });

    await user.click(await screen.findByRole("button", { name: "Behind main by 3" }));
    await user.click(
      within(await screen.findByRole("menu", { name: "Behind main by 3" })).getByRole("menuitem", { name: "Compare" }),
    );

    expect(await screen.findByText("What main gained since driver-trait forked")).toBeInTheDocument();
  });
});

describe("my PRs that are behind their base", () => {
  it("a single PR shows how far behind main it is", async () => {
    setup();
    const row = (await screen.findByText("Behind main by 2")).closest("[role=listitem]");
    expect(row).not.toBeNull();
    expect(within(row as HTMLElement).getByText("#128")).toBeInTheDocument();
  });

  it("a PR that no longer sits on the head of the one below names it, and the card keeps the stack's lead", async () => {
    setup();
    expect(await screen.findByText("Behind #415")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Behind main by 3" })).toBeInTheDocument();
    expect(screen.queryByText("Behind #412")).not.toBeInTheDocument();
  });

  it("a drifted PR shows even when the stack's base has not moved", async () => {
    seedInboxScenario({ behind: 0, drifted: [418] });
    setup();
    expect(await screen.findByText("Behind #415")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^Behind main by/ })).not.toBeInTheDocument();
  });
});
