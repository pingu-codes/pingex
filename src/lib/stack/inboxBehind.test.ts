import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import DialogHost from "$lib/components/DialogHost.svelte";
import type { InboxHandlers } from "$lib/inbox/handlers";
import { inboxes } from "$lib/inbox/inboxStore.svelte";
import PullRequestsView from "$lib/inbox/PullRequestsView.svelte";
import { resetDeskActivity } from "$lib/services/preview/desk";
import { resetPreviewStacks } from "$lib/services/preview/stack";

const MAIN = "/Users/ciaran/Projects/codex-custom";

beforeEach(() => {
  resetPreviewStacks();
  resetDeskActivity();
  localStorage.clear();
  for (const key of Object.keys(inboxes)) delete inboxes[key];
});

function setup(handlers: Partial<InboxHandlers> = {}) {
  render(DialogHost);
  const all: InboxHandlers = { openPr: vi.fn(), newThread: vi.fn(), ...handlers };
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

  it("Compare shows what the base gained since the stack forked", async () => {
    const { user } = setup({ restack: vi.fn() });

    await user.click(await screen.findByRole("button", { name: "Behind main by 3" }));
    await user.click(
      within(await screen.findByRole("menu", { name: "Behind main by 3" })).getByRole("menuitem", { name: "Compare" }),
    );

    expect(await screen.findByText("What main gained since driver-trait forked")).toBeInTheDocument();
  });
});
