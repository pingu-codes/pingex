import { fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import ReviewPage from "$lib/review/ReviewPage.svelte";
import { commands } from "$lib/services/homeRouting";
import { DESK_REPOS, deskInbox, resetDeskReviewState, THREADED_PR } from "$lib/services/preview/desk";

vi.mock("$lib/services/homeRouting", async (importOriginal) => {
  const actual = await importOriginal<typeof import("$lib/services/homeRouting")>();
  return {
    ...actual,
    commands: {
      ...actual.commands,
      reviewReply: vi.fn(actual.commands.reviewReply),
      reviewSubmitPendingReview: vi.fn(actual.commands.reviewSubmitPendingReview),
      reviewPrDetail: vi.fn(actual.commands.reviewPrDetail),
    },
  };
});

const repoDir = DESK_REPOS[0].path as string;

function renderPage(props: Partial<Parameters<typeof render<typeof ReviewPage>>[1]> = {}) {
  return render(ReviewPage, { repoDir, number: THREADED_PR, onBack: vi.fn(), ...props });
}

async function opened() {
  await screen.findByRole("heading", { name: "Retry the app-server handshake" });
  await screen.findByText(/Viewed \d of 2/);
}

const yourReview = () => screen.getByRole("complementary", { name: "Your review" });

beforeEach(() => {
  resetDeskReviewState();
  vi.mocked(commands.reviewReply).mockClear();
  vi.mocked(commands.reviewSubmitPendingReview).mockClear();
  vi.mocked(commands.reviewPrDetail).mockClear();
});

describe("ReviewPage", () => {
  it("a remount shows the PR from the cache at once, without reading it again", async () => {
    const first = renderPage();
    await opened();
    first.unmount();

    renderPage();
    expect(screen.getByRole("heading", { name: "Retry the app-server handshake" })).toBeInTheDocument();
    await screen.findByText(/Viewed \d of 2/);
    expect(commands.reviewPrDetail).toHaveBeenCalledOnce();
  });

  it("re-reads the PR after a reply, through the write's invalidation", async () => {
    const user = userEvent.setup();
    renderPage();
    await opened();
    vi.mocked(commands.reviewPrDetail).mockClear();
    const thread = screen.getByRole("group", { name: "Thread on src/lib/loader.ts:4" });
    await user.type(within(thread).getByRole("textbox", { name: "Reply" }), "Thanks");
    await user.click(within(thread).getByRole("button", { name: "Reply now" }));
    await waitFor(() => expect(commands.reviewReply).toHaveBeenCalledOnce());
    await waitFor(() => expect(commands.reviewPrDetail).toHaveBeenCalledOnce());
  });

  it("collapses a viewed file and keeps it viewed across a remount", async () => {
    const user = userEvent.setup();
    const first = renderPage();
    await opened();
    expect(screen.getByText("Viewed 0 of 2")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Toggle src/lib/loader.ts" })).toHaveAttribute("aria-expanded", "true");

    await user.click(screen.getByRole("checkbox", { name: "Viewed src/lib/loader.ts" }));
    expect(screen.getByText("Viewed 1 of 2")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Toggle src/lib/loader.ts" })).toHaveAttribute("aria-expanded", "false");
    first.unmount();

    renderPage();
    await opened();
    await waitFor(() => expect(screen.getByText("Viewed 1 of 2")).toBeInTheDocument());
    expect(screen.getByRole("checkbox", { name: "Viewed src/lib/loader.ts" })).toBeChecked();
    expect(screen.getByRole("button", { name: "Toggle src/lib/loader.ts" })).toHaveAttribute("aria-expanded", "false");
    expect(screen.getByRole("button", { name: "Toggle docs/review.md" })).toHaveAttribute("aria-expanded", "true");
  });

  it("offers Comment, Suggest change and Ask agent for a selected range", async () => {
    const user = userEvent.setup();
    const onAskAgent = vi.fn();
    renderPage({ onAskAgent });
    await opened();

    const loader = within(screen.getByRole("region", { name: "src/lib/loader.ts" }));
    await user.click(loader.getByRole("button", { name: "Select line 3" }));
    await user.keyboard("{Shift>}");
    await user.click(loader.getByRole("button", { name: "Select line 5" }));
    await user.keyboard("{/Shift}");

    const toolbar = screen.getByRole("toolbar", { name: "Lines 3–5" });
    await user.click(within(toolbar).getByRole("button", { name: "Ask agent" }));
    expect(onAskAgent).toHaveBeenCalledWith(
      expect.objectContaining({ path: "src/lib/loader.ts", side: "RIGHT", startLine: 3, line: 5 }),
    );

    await user.click(within(toolbar).getByRole("button", { name: /^Comment/ }));
    const composer = within(screen.getByRole("group", { name: "New comment on lines 3–5" }));
    await user.type(composer.getByRole("textbox", { name: "Comment on lines 3–5" }), "Guard clause reads backwards");
    await user.click(composer.getByRole("button", { name: "Add to review" }));

    expect(within(yourReview()).getByText("loader.ts:3–5")).toBeInTheDocument();
    expect(within(yourReview()).getByRole("button", { name: "Submit review with 1 comment" })).toBeInTheDocument();
  });

  it("hovering a line offers + to comment on it, and a drag down the numbers selects a range", async () => {
    const user = userEvent.setup();
    renderPage();
    await opened();

    const loader = within(screen.getByRole("region", { name: "src/lib/loader.ts" }));
    await user.hover(loader.getByRole("button", { name: "Select line 4" }));
    await user.click(loader.getByRole("button", { name: "Comment on line 4" }));
    expect(screen.getByRole("group", { name: "New comment on line 4" })).toBeInTheDocument();

    await fireEvent.pointerDown(loader.getByRole("button", { name: "Select line 3" }), { button: 0 });
    await fireEvent.pointerOver(loader.getByRole("button", { name: "Select line 5" }));
    await fireEvent.pointerUp(window);
    await fireEvent.pointerOver(loader.getByRole("button", { name: "Select line 7" }));
    expect(screen.getByRole("toolbar", { name: "Lines 3–5" })).toBeInTheDocument();
  });

  it("suggests a change prefilled with the selected lines", async () => {
    const user = userEvent.setup();
    renderPage();
    await opened();

    const loader = within(screen.getByRole("region", { name: "src/lib/loader.ts" }));
    await user.click(loader.getByRole("button", { name: "Select line 7" }));
    await user.click(
      within(screen.getByRole("toolbar", { name: "Line 7" })).getByRole("button", { name: "Suggest change" }),
    );
    const replacement = screen.getByRole("textbox", { name: "Suggested change for line 7" });
    expect(replacement).toHaveValue("  return readFileSync(path);");
    await user.clear(replacement);
    await user.type(replacement, "  return readFileSync(path, 'utf8');");
    await user.click(
      within(screen.getByRole("group", { name: "New suggestion on line 7" })).getByRole("button", {
        name: "Add to review",
      }),
    );

    expect(within(yourReview()).getByText("Suggestion")).toBeInTheDocument();
  });

  it("posts Reply now at once and holds Add to review for the submission", async () => {
    const user = userEvent.setup();
    renderPage();
    await opened();

    const held = screen.getByRole("group", { name: "Thread on src/lib/loader.ts:6" });
    await user.type(within(held).getByRole("textbox", { name: "Reply" }), "Rethrow with the cause");
    await user.click(within(held).getByRole("button", { name: "Add to review" }));
    expect(commands.reviewReply).not.toHaveBeenCalled();
    expect(within(yourReview()).getByText("Rethrow with the cause")).toBeInTheDocument();

    const now = screen.getByRole("group", { name: "Thread on src/lib/loader.ts:4" });
    await user.type(within(now).getByRole("textbox", { name: "Reply" }), "Fine as is");
    await user.click(within(now).getByRole("button", { name: "Reply now" }));
    await waitFor(() => expect(commands.reviewReply).toHaveBeenCalledWith(repoDir, THREADED_PR, 10, "Fine as is"));
    expect(within(yourReview()).queryByText("Fine as is")).not.toBeInTheDocument();

    await user.click(within(yourReview()).getByRole("button", { name: "Submit review with 1 comment" }));
    await waitFor(() => expect(commands.reviewSubmitPendingReview).toHaveBeenCalledWith(repoDir, THREADED_PR));
    await waitFor(() => expect(within(yourReview()).queryByText("Rethrow with the cause")).not.toBeInTheDocument());
  });

  it("keeps the pending review across a remount", async () => {
    const user = userEvent.setup();
    const first = renderPage();
    await opened();
    await user.type(within(yourReview()).getByRole("textbox", { name: "Summary" }), "Nearly there");
    await fireEvent.blur(within(yourReview()).getByRole("textbox", { name: "Summary" }));
    await waitFor(async () =>
      expect((await commands.reviewPendingReview(repoDir, THREADED_PR))?.summary).toBe("Nearly there"),
    );
    first.unmount();

    renderPage();
    await opened();
    await waitFor(() =>
      expect(within(yourReview()).getByRole("textbox", { name: "Summary" })).toHaveValue("Nearly there"),
    );
  });

  it("moves between files with ] and marks the current one viewed with v", async () => {
    const user = userEvent.setup();
    renderPage();
    await opened();

    await user.keyboard("]");
    await user.keyboard("v");
    expect(screen.getByRole("checkbox", { name: "Viewed docs/review.md" })).toBeChecked();
    expect(screen.getByRole("checkbox", { name: "Viewed src/lib/loader.ts" })).not.toBeChecked();
    expect(screen.getByText("Viewed 1 of 2")).toBeInTheDocument();
  });

  it("marks where new conversation comments start", async () => {
    const user = userEvent.setup();
    renderPage({ seenAt: "2026-11-04T12:00:00Z" });
    await opened();

    await user.click(screen.getByRole("radio", { name: /^Conversation/ }));
    expect(screen.getByRole("separator")).toHaveTextContent(/1 new since/);
  });

  it("shows reviews in the conversation, each with the threads it started", async () => {
    const user = userEvent.setup();
    renderPage();
    await opened();

    // tomasz's review, ciaran's inline-only review, and mira-k's comment.
    await user.click(screen.getByRole("radio", { name: "Conversation 3" }));
    const conversation = screen.getByRole("region", { name: "Conversation 3" });
    expect(within(conversation).getByText("requested changes")).toBeInTheDocument();
    expect(
      within(conversation).getByText("Requesting changes for the error handling; the rest looks good."),
    ).toBeInTheDocument();
    expect(within(conversation).getByRole("group", { name: "Thread on src/lib/loader.ts:4" })).toBeInTheDocument();
    expect(within(conversation).getByRole("group", { name: "Thread on src/lib/loader.ts:6" })).toBeInTheDocument();
    expect(within(conversation).getByRole("group", { name: "Thread on docs/review.md:1" })).toBeInTheDocument();
    // A reply shows inside its thread, not as a review of its own.
    expect(within(conversation).getAllByText("reviewed")).toHaveLength(1);

    // A thread's location opens its file in the diff.
    await user.click(within(conversation).getByRole("button", { name: "docs/review.md:1" }));
    expect(screen.getByRole("radio", { name: "Files 2" })).toBeChecked();
  });

  it("renders comments as markdown, under the lines each thread is about", async () => {
    const user = userEvent.setup();
    renderPage();
    await opened();
    await user.click(screen.getByRole("radio", { name: /^Conversation/ }));
    const conversation = screen.getByRole("region", { name: /^Conversation/ });

    const thread = within(conversation).getByRole("group", { name: "Thread on src/lib/loader.ts:4" });
    expect(within(thread).getByText("arbitrary").tagName).toBe("STRONG");
    expect(within(thread).getByText("config").tagName).toBe("EM");

    // A single-line thread shows its line and the ones before it, marked.
    const lines = within(conversation).getByRole("figure", { name: "Lines src/lib/loader.ts:4" });
    expect(lines).toHaveTextContent("if (!path) {");
    expect(lines.querySelector("[data-commented=true]")).toHaveTextContent("if (!path) {");
    // A multi-line one shows its range; an outdated one says so.
    expect(within(conversation).getByRole("figure", { name: "Lines src/lib/loader.ts:4–6" })).toHaveTextContent(
      "throw new Error",
    );
    expect(within(conversation).getAllByText("Outdated")).toHaveLength(1);
  });

  it("replies from a thread in the conversation as from the diff", async () => {
    const user = userEvent.setup();
    renderPage();
    await opened();
    await user.click(screen.getByRole("radio", { name: /^Conversation/ }));
    const thread = within(screen.getByRole("region", { name: /^Conversation/ })).getByRole("group", {
      name: "Thread on src/lib/loader.ts:4",
    });
    await user.type(within(thread).getByRole("textbox", { name: "Reply" }), "Thanks");
    await user.click(within(thread).getByRole("button", { name: "Reply now" }));
    await waitFor(() => expect(commands.reviewReply).toHaveBeenCalledOnce());
  });

  it("lists the next review requests and steps back through them with k", async () => {
    const user = userEvent.setup();
    const onOpenPr = vi.fn();
    renderPage({ inbox: deskInbox(repoDir), onOpenPr });
    await opened();

    expect(within(yourReview()).getByRole("button", { name: /#403 Trap focus in dialogs/ })).toBeInTheDocument();
    await user.keyboard("k");
    expect(onOpenPr).toHaveBeenCalledWith(403);
  });

  it("offers only a comment on the viewer's own PR, which GitHub takes from its author", async () => {
    renderPage({ inbox: { ...deskInbox(repoDir), viewer: "mira-k" } });
    await opened();

    expect(within(yourReview()).queryByRole("radiogroup", { name: "Verdict" })).toBeNull();
    expect(within(yourReview()).getByRole("button", { name: "Submit review" })).toBeDisabled();
  });

  it("offers every verdict on someone else's PR", async () => {
    renderPage({ inbox: deskInbox(repoDir) });
    await opened();

    expect(within(yourReview()).getByRole("radiogroup", { name: "Verdict" })).toBeInTheDocument();
    expect(within(yourReview()).getByRole("radio", { name: "Request changes" })).toBeInTheDocument();
  });
});

describe("the header's behind-the-base badge", () => {
  it("names the PR below when the PR no longer sits on its head", async () => {
    renderPage({ number: 418, inbox: deskInbox(repoDir) });
    expect(await screen.findByText("Behind #415")).toBeInTheDocument();
  });

  it("counts the commits on main a single PR lacks", async () => {
    renderPage({ number: 128, inbox: deskInbox(repoDir) });
    expect(await screen.findByText("Behind main by 2")).toBeInTheDocument();
  });

  it("is absent when the PR is on its base's tip", async () => {
    renderPage({ inbox: deskInbox(repoDir) });
    await opened();
    expect(screen.queryByText(/^Behind /)).not.toBeInTheDocument();
  });
});
