import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { InboxHandlers } from "$lib/inbox/handlers";
import { inboxes } from "$lib/inbox/inboxStore.svelte";
import PullRequestsView from "$lib/inbox/PullRequestsView.svelte";
import { commands } from "$lib/services/homeRouting";
import { resetDeskActivity } from "$lib/services/preview/desk";

vi.mock("$lib/services/homeRouting", async (importOriginal) => {
  const actual = await importOriginal<typeof import("$lib/services/homeRouting")>();
  return {
    ...actual,
    commands: {
      ...actual.commands,
      reviewInbox: vi.fn(actual.commands.reviewInbox),
      reviewMarkActivitySeen: vi.fn(actual.commands.reviewMarkActivitySeen),
    },
  };
});

const MAIN = "/Users/ciaran/Projects/codex-custom";
const REPOS = [
  { path: MAIN, name: "codex-custom" },
  { path: "/Users/ciaran/Projects/arctic-explorer", name: "arctic-explorer" },
];

function setup(handlers: Partial<InboxHandlers> = {}) {
  const all: InboxHandlers = { openPr: vi.fn(), newThread: vi.fn(), ...handlers };
  const onSelectRepo = vi.fn();
  render(PullRequestsView, { repos: REPOS, repoPath: MAIN, onSelectRepo, handlers: all });
  return { user: userEvent.setup(), handlers: all, onSelectRepo };
}

const row = (number: number) => screen.getByRole("listitem", { name: new RegExp(`^#${number} `) });
const queryRow = (number: number) => screen.queryByRole("listitem", { name: new RegExp(`^#${number} `) });
const filter = (name: RegExp) => screen.getByRole("radio", { name });

beforeEach(() => {
  resetDeskActivity();
  localStorage.clear();
  for (const key of Object.keys(inboxes)) delete inboxes[key];
  vi.clearAllMocks();
});

async function loaded() {
  await waitFor(() => expect(row(412)).toBeInTheDocument());
}

describe("Pull requests inbox", () => {
  it("shows each filter's count and puts what needs me first", async () => {
    const { user } = setup();
    await loaded();

    expect(screen.getByRole("heading", { name: "codex-custom" })).toBeInTheDocument();
    expect(screen.getByText(/ciaran\/codex-custom · synced/)).toBeInTheDocument();
    expect(filter(/Needs me 8/)).toBeChecked();
    expect(filter(/Mine 4/)).toBeInTheDocument();
    expect(filter(/Review requested 4/)).toBeInTheDocument();
    expect(filter(/Everyone 9/)).toBeInTheDocument();
    expect(queryRow(420)).toBeNull();

    await user.click(filter(/Everyone 9/));
    expect(row(420)).toBeInTheDocument();
    await user.click(filter(/Mine 4/));
    expect(queryRow(409)).toBeNull();
    expect(row(128)).toBeInTheDocument();
  });

  it("switches repository from the heading combobox by typing", async () => {
    const { user, onSelectRepo } = setup();
    await loaded();

    const picker = screen.getByRole("combobox", { name: "Repository" });
    expect(picker).toHaveValue("codex-custom");
    await user.click(picker);
    expect(screen.getAllByRole("option")).toHaveLength(2);
    await user.keyboard("{Escape}");
    expect(picker).toHaveValue("codex-custom");

    await user.click(picker);
    await user.keyboard("arctic");
    expect(screen.getAllByRole("option")).toHaveLength(1);
    await user.keyboard("{Enter}");
    expect(onSelectRepo).toHaveBeenCalledWith("/Users/ciaran/Projects/arctic-explorer");
  });

  it("narrows with the chips and the text filter", async () => {
    const { user } = setup();
    await loaded();

    await user.click(screen.getByRole("button", { name: "CI failing", pressed: false }));
    expect(row(418)).toBeInTheDocument();
    expect(queryRow(412)).toBeNull();
    await user.click(screen.getByRole("button", { name: "CI failing", pressed: true }));

    await user.type(screen.getByRole("textbox", { name: "Filter pull requests" }), "handshake");
    expect(row(409)).toBeInTheDocument();
    expect(queryRow(412)).toBeNull();
  });

  it("shows my stack as one card and groups a review request by its author's stack", async () => {
    const { user } = setup();
    await loaded();

    const card = screen.getByRole("region", { name: "Stack driver-trait" });
    expect(within(card).getByText("3 PRs onto")).toBeInTheDocument();
    expect(
      within(card)
        .getAllByRole("listitem")
        .map((item) => item.getAttribute("aria-label")?.split(" ")[0]),
    ).toEqual(["#412", "#415", "#418"]);
    expect(within(row(412)).getByTitle("1 of 3 in the stack")).toHaveTextContent("1");
    expect(within(row(418)).getByTitle("3 of 3 in the stack")).toHaveTextContent("3");
    const theirs = screen.getByRole("region", { name: "Stack focus-rings" });
    expect(within(theirs).getByText("jo-r · 2 PRs")).toBeInTheDocument();
    expect(within(row(403)).getByRole("button", { name: "Review stack" })).toBeInTheDocument();
    expect(within(row(401)).getByText("2 commits since your approval")).toBeInTheDocument();
    expect(within(row(403)).getByTitle("2 of 2 in the stack")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Group by stack", pressed: true }));
    expect(screen.queryByRole("region", { name: /^Stack / })).toBeNull();
    expect(row(415)).toBeInTheDocument();
    expect(within(row(415)).queryByTitle(/in the stack$/)).toBeNull();
  });

  it("gives each row the primary action its state calls for", async () => {
    setup();
    await loaded();

    expect(within(row(412)).getByRole("button", { name: "Merge" })).toBeInTheDocument();
    expect(within(row(415)).getByRole("button", { name: "Respond" })).toBeInTheDocument();
    expect(within(row(418)).getByRole("button", { name: "Fix CI in thread" })).toBeDisabled();
    expect(await within(row(409)).findByRole("button", { name: "Review" })).toBeInTheDocument();
    expect(await within(row(409)).findByText(/Agent review ready · 3\s+findings/)).toBeInTheDocument();
    expect(within(row(401)).getByText("Agent review running")).toBeInTheDocument();
  });

  it("opens a PR from a plain click on its row", async () => {
    const { user, handlers } = setup();
    await loaded();

    await user.click(within(row(128)).getByText("Add pull-request review view"));
    expect(handlers.openPr).toHaveBeenCalledWith(MAIN, expect.objectContaining({ number: 128 }), {});
    expect(screen.queryByText(/selected$/)).toBeNull();
  });

  it("selects several rows with ⌘-click, ⇧-click and X, then marks them seen at once", async () => {
    const { user } = setup();
    await loaded();

    await user.keyboard("{Meta>}");
    await user.click(within(row(412)).getByText("Harness driver trait"));
    await user.keyboard("{/Meta}");
    await user.keyboard("{Shift>}");
    await user.click(within(row(418)).getByText("maki over ACP"));
    await user.keyboard("{/Shift}");
    expect(screen.getByText("3 selected")).toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: "Select #415" })).toBeChecked();

    row(128).focus();
    await user.keyboard("x");
    expect(screen.getByText("4 selected")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Mark seen" }));
    expect(commands.reviewMarkActivitySeen).toHaveBeenCalledWith(MAIN, [412, 415, 418, 128]);
    expect(screen.queryByText(/selected$/)).toBeNull();
  });

  it("shows unseen activity until marked seen with E, and Mark all seen clears the feed", async () => {
    const { user } = setup();
    await loaded();

    expect(row(415)).toHaveAttribute("data-unseen", "true");
    expect(within(row(415)).getByText("1 new")).toBeInTheDocument();
    const feed = screen.getByRole("complementary", { name: "Since you last looked" });
    expect(within(feed).getByText(/requested changes on #415/)).toBeInTheDocument();

    row(415).focus();
    await user.keyboard("e");
    expect(commands.reviewMarkActivitySeen).toHaveBeenCalledWith(MAIN, [415]);
    expect(row(415)).not.toHaveAttribute("data-unseen", "true");
    expect(within(feed).queryByText(/requested changes on #415/)).toBeNull();

    await user.click(within(feed).getByRole("button", { name: "Mark all seen" }));
    expect(within(feed).getByText("Nothing new since you last looked.")).toBeInTheDocument();
    // Seen state is the database's: a fresh sync keeps it.
    for (const key of Object.keys(inboxes)) delete inboxes[key];
    await user.click(screen.getByRole("button", { name: "Sync" }));
    await waitFor(() => expect(row(401)).not.toHaveAttribute("data-unseen", "true"));
  });

  it("collapses the Since you last looked feed to a count", async () => {
    const { user } = setup();
    await loaded();
    const feed = screen.getByRole("complementary", { name: "Since you last looked" });
    const toggle = within(feed).getByRole("button", { name: /Since you last looked/ });
    expect(toggle).toHaveAttribute("aria-expanded", "true");

    await user.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(within(feed).queryByText(/requested changes on #415/)).toBeNull();
    expect(within(toggle).getByText(String(inboxes[MAIN]?.inbox?.feed.length))).toBeInTheDocument();

    await user.click(toggle);
    expect(within(feed).getByText(/requested changes on #415/)).toBeInTheDocument();
  });

  it("opens a row with Enter and starts a thread with T", async () => {
    const { user, handlers } = setup();
    await loaded();

    row(412).focus();
    await user.keyboard("{Enter}");
    expect(handlers.openPr).toHaveBeenCalledWith(MAIN, expect.objectContaining({ number: 412 }), {});
    await user.keyboard("t");
    expect(handlers.newThread).toHaveBeenCalledWith(MAIN, expect.objectContaining({ number: 412 }));
  });

  it("lists agent reviews in flight", async () => {
    setup();
    await loaded();
    const panel = await screen.findByRole("region", { name: "Agent reviews" });
    await waitFor(() => expect(within(panel).getByText("Running")).toBeInTheDocument());
    expect(within(panel).getByText("Queued")).toBeInTheDocument();
  });
});
