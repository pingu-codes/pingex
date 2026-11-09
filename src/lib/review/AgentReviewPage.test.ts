import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import DialogHost from "$lib/components/DialogHost.svelte";
import AgentReviewPage from "$lib/review/AgentReviewPage.svelte";
import { commands } from "$lib/services/homeRouting";
import { resetPreviewAgentReviews } from "$lib/services/preview/agentReviews";
import { DESK_REPOS, resetDeskReviewState, THREADED_PR } from "$lib/services/preview/desk";

vi.mock("$lib/services/homeRouting", async (importOriginal) => {
  const actual = await importOriginal<typeof import("$lib/services/homeRouting")>();
  return {
    ...actual,
    commands: {
      ...actual.commands,
      reviewSetFindingState: vi.fn(actual.commands.reviewSetFindingState),
    },
  };
});

const repoDir = DESK_REPOS[0].path as string;

function renderPage() {
  render(DialogHost);
  render(AgentReviewPage, { repoDir, number: THREADED_PR, onBack: vi.fn(), viewer: "ciaran" });
}

const card = (title: string) => screen.findByRole("article", { name: `Agent finding: ${title}` });
const yourReview = () => screen.getByRole("complementary", { name: "Your review" });

beforeEach(() => {
  resetDeskReviewState();
  resetPreviewAgentReviews();
  vi.mocked(commands.reviewSetFindingState).mockClear();
});

describe("agent findings on the review page", () => {
  it("shows a finding inline as a draft only I can see, and counts it in the file tree", async () => {
    renderPage();
    const finding = await card("Read errors lose the path");
    expect(within(finding).getByText(/draft, only you can see this/)).toBeInTheDocument();
    expect(within(finding).getByText("Medium")).toBeInTheDocument();
    expect(within(finding).getByText(/no try\/catch/)).toBeInTheDocument();
    // The repeat of tomasz's comment is merged, not a second card.
    expect(screen.getByText(/Same as tomasz's loader.ts:6 comment/)).toBeInTheDocument();
    // Agent findings filter, and the file tree's count of open findings.
    expect(screen.getByRole("button", { name: "Agent findings" })).toBeInTheDocument();
    const tree = screen.getByRole("navigation", { name: "Changed files" });
    expect(within(tree).getByTitle("Agent findings")).toHaveTextContent("2");
    // The summary in Your review.
    const summary = within(yourReview()).getByRole("region", { name: "Agent review" });
    expect(within(summary).getByText(/3 findings · 0 added to your review · 0 dismissed/)).toBeInTheDocument();
  });

  it("⌘F finds a finding's text and paints it, on a card and on a merged line", async () => {
    const user = userEvent.setup();
    renderPage();
    const finding = await card("Read errors lose the path");
    await user.keyboard("{Meta>}f{/Meta}");
    const input = screen.getByRole("textbox", { name: "Find on this tab" });

    // Marked only if the element's text equals the unit's, which offsets rely on.
    await user.type(input, "no try/catch");
    await screen.findByText("1 of 1");
    await waitFor(() => expect(finding.querySelector("[data-find-current]")).toHaveTextContent(/no try\/catch/));

    await user.clear(input);
    await user.type(input, "Same as tomasz's loader.ts:6 comment");
    await screen.findByText("1 of 1");
    await waitFor(() => expect(document.querySelector("[data-find-current]")).toHaveTextContent(/Same as tomasz/));
  });

  it("adds a finding to my review, and lets me edit what it posts", async () => {
    const user = userEvent.setup();
    renderPage();
    const finding = await card("Read errors lose the path");
    await user.click(within(finding).getByRole("button", { name: "Add to my review" }));

    await waitFor(() => expect(within(yourReview()).getByText("loader.ts:7")).toBeInTheDocument());
    expect(within(finding).getByText(/added to your review/)).toBeInTheDocument();
    expect(commands.reviewSetFindingState).toHaveBeenCalledWith(
      repoDir,
      expect.stringMatching(/-1$/),
      "added",
      null,
      null,
    );

    await user.click(within(finding).getByRole("button", { name: "Edit" }));
    const text = within(finding).getByRole("textbox", { name: "Comment text" });
    await user.clear(text);
    await user.type(text, "Say which file failed to load.");
    await user.click(within(finding).getByRole("button", { name: "Save" }));
    expect(within(finding).getByText("Say which file failed to load.")).toBeInTheDocument();
    const stored = await commands.reviewPendingReview(repoDir, THREADED_PR);
    expect(stored?.entries).toEqual([
      expect.objectContaining({
        kind: "acceptedFinding",
        body: "Say which file failed to load.",
        anchor: expect.objectContaining({ path: "src/lib/loader.ts", line: 7 }),
      }),
    ]);
  });

  it("dismisses a finding with a reason, or with a note telling the agent why it's wrong", async () => {
    const user = userEvent.setup();
    renderPage();
    const first = await card("Read errors lose the path");
    await user.click(within(first).getByRole("button", { name: "Dismiss" }));
    await user.click(await screen.findByRole("menuitem", { name: /Out of scope for this PR/ }));
    await waitFor(() =>
      expect(screen.queryByRole("article", { name: "Agent finding: Read errors lose the path" })).toBeNull(),
    );
    expect(commands.reviewSetFindingState).toHaveBeenCalledWith(
      repoDir,
      expect.stringMatching(/-1$/),
      "dismissed",
      "outOfScope",
      null,
    );

    const second = await card("Synchronous read on startup");
    await user.click(within(second).getByRole("button", { name: "Dismiss" }));
    await user.click(await screen.findByRole("menuitem", { name: /Wrong, tell the agent why/ }));
    await user.type(await screen.findByRole("textbox", { name: "Why it's wrong" }), "It runs once, before the window");
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Dismiss" }));
    await waitFor(() =>
      expect(commands.reviewSetFindingState).toHaveBeenCalledWith(
        repoDir,
        expect.stringMatching(/-3$/),
        "dismissed",
        "wrong",
        "It runs once, before the window",
      ),
    );
  });

  it("saves 'Skip this kind of finding' as a repository review rule after I confirm", async () => {
    const user = userEvent.setup();
    renderPage();
    const finding = await card("Synchronous read on startup");
    await user.click(within(finding).getByRole("button", { name: "Dismiss" }));
    await user.click(await screen.findByRole("menuitem", { name: /Skip this kind of finding in codex-custom/ }));
    const rule = await screen.findByRole("textbox", { name: "Rule" });
    expect(rule).toHaveValue("Synchronous read on startup");
    await user.clear(rule);
    await user.type(rule, "Sync file reads at startup");
    await user.click(screen.getByRole("button", { name: "Save rule" }));
    await waitFor(async () =>
      expect((await commands.reviewRules(repoDir)).map((entry) => entry.rule)).toEqual(["Sync file reads at startup"]),
    );
  });
});
