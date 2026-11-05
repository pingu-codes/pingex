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
});

describe("ReviewPage", () => {
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

  it("lists the next review requests and steps back through them with k", async () => {
    const user = userEvent.setup();
    const onOpenPr = vi.fn();
    renderPage({ inbox: deskInbox(repoDir), onOpenPr });
    await opened();

    expect(within(yourReview()).getByRole("button", { name: /#403 Trap focus in dialogs/ })).toBeInTheDocument();
    await user.keyboard("k");
    expect(onOpenPr).toHaveBeenCalledWith(403);
  });
});
