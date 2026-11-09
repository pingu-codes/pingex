import { fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import ReviewPage from "$lib/review/ReviewPage.svelte";
import { prPlace } from "$lib/review/reviewPage";
import { commands } from "$lib/services/homeRouting";
import { DESK_REPOS, deskInbox, resetDeskReviewState, THREADED_PR } from "$lib/services/preview/desk";
import { invalidate } from "$lib/services/queryCache.svelte";
import type { PrDetail } from "$lib/types";

vi.mock("$lib/services/homeRouting", async (importOriginal) => {
  const actual = await importOriginal<typeof import("$lib/services/homeRouting")>();
  return {
    ...actual,
    commands: {
      ...actual.commands,
      reviewReply: vi.fn(actual.commands.reviewReply),
      reviewSubmitPendingReview: vi.fn(actual.commands.reviewSubmitPendingReview),
      reviewPrDetail: vi.fn(actual.commands.reviewPrDetail),
      reviewChangesSince: vi.fn(actual.commands.reviewChangesSince),
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
  localStorage.removeItem("pingex-review-panel-collapsed");
  resetDeskReviewState();
  vi.mocked(commands.reviewReply).mockClear();
  vi.mocked(commands.reviewSubmitPendingReview).mockClear();
  vi.mocked(commands.reviewPrDetail).mockClear();
  vi.mocked(commands.reviewChangesSince).mockClear();
});

describe("ReviewPage", () => {
  it("comes back as it was left: the tab, the filter, the folds and the scroll", async () => {
    const user = userEvent.setup();
    const first = renderPage();
    await opened();
    await user.click(screen.getByRole("button", { name: /^Unresolved/ }));
    const pane = document.querySelector<HTMLElement>("[data-diff-pane]") as HTMLElement;
    pane.scrollTop = 300;
    fireEvent.scroll(pane);
    await user.click(screen.getByRole("radio", { name: /^Checks/ }));
    await user.click(screen.getByRole("radio", { name: /^Files/ }));
    pane.scrollTop = 300;
    fireEvent.scroll(pane);
    first.unmount();

    renderPage();
    await opened();

    expect(screen.getByRole("button", { name: /^Unresolved/ })).toHaveAttribute("aria-pressed", "true");
    const again = document.querySelector<HTMLElement>("[data-diff-pane]") as HTMLElement;
    await waitFor(() => expect(again.scrollTop).toBe(300));
  });

  it("comes back on the tab it was left on", async () => {
    const user = userEvent.setup();
    const first = renderPage();
    await opened();
    await user.click(screen.getByRole("radio", { name: /^Conversation/ }));
    first.unmount();

    renderPage();
    await screen.findByRole("heading", { name: "Retry the app-server handshake" });

    expect(screen.getByRole("radio", { name: /^Conversation/ })).toBeChecked();
  });

  it("starts a PR it hasn't shown at the top, on Files", async () => {
    renderPage();
    await opened();

    expect(screen.getByRole("radio", { name: /^Files/ })).toBeChecked();
    expect((document.querySelector("[data-diff-pane]") as HTMLElement).scrollTop).toBe(0);
  });

  it("a collapsed Your review stays collapsed on the next PR, and still counts its entries", async () => {
    const user = userEvent.setup();
    const first = renderPage();
    await opened();
    const thread = screen.getByRole("group", { name: "Thread on src/lib/loader.ts:4" });
    await user.type(within(thread).getByRole("textbox", { name: "Reply" }), "Thanks");
    await user.click(within(thread).getByRole("button", { name: "Add to review" }));
    await user.click(screen.getByRole("button", { name: "Collapse Your review" }));
    // The strip says a pending review is still there.
    expect(screen.getByTitle("Entries in your pending review")).toHaveTextContent("1");
    first.unmount();

    renderPage();
    await opened();
    await user.click(screen.getByRole("button", { name: "Expand Your review" }));
    expect(screen.queryByRole("button", { name: "Expand Your review" })).not.toBeInTheDocument();
    expect(localStorage.getItem("pingex-review-panel-collapsed")).toBe("false");
  });

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

  describe("find on this tab", () => {
    const findInput = () => screen.getByRole("textbox", { name: "Find on this tab" });
    async function search(user: ReturnType<typeof userEvent.setup>, query: string) {
      await user.keyboard("{Meta>}f{/Meta}");
      await user.clear(findInput());
      await user.type(findInput(), query);
    }

    it("opens with ⌘F or Ctrl+F, focused, and leaves ⌘⇧F to the sidebar", async () => {
      const user = userEvent.setup();
      renderPage();
      await opened();

      await user.keyboard("{Meta>}{Shift>}f{/Shift}{/Meta}");
      expect(screen.queryByRole("textbox", { name: "Find on this tab" })).not.toBeInTheDocument();

      await user.keyboard("{Control>}f{/Control}");
      expect(findInput()).toHaveFocus();
    });

    it("counts matches in code and comments, and steps through them, wrapping", async () => {
      const user = userEvent.setup();
      renderPage();
      await opened();

      // The removed and added lines in the code, and THREAD_2's rendered comment.
      await search(user, "path is required");
      await screen.findByText("1 of 2");
      await user.keyboard("{Enter}");
      await screen.findByText("2 of 2");
      await user.keyboard("{Enter}");
      await screen.findByText("1 of 2");
      await user.keyboard("{Shift>}{Enter}{/Shift}");
      await screen.findByText("2 of 2");
    });

    it("searches what a comment shows, not its markdown", async () => {
      const user = userEvent.setup();
      renderPage();
      await opened();

      await search(user, "feels arbitrary");
      await screen.findByText("1 of 1");
      await search(user, "**arbitrary**");
      await screen.findByText("No results");
    });

    it("marks the current match's row, and ⌘G moves it", async () => {
      const user = userEvent.setup();
      renderPage();
      await opened();

      await search(user, "readFileSync");
      await screen.findByText(/^1 of \d+$/);
      await waitFor(() => expect(document.querySelector("[data-find-current]")).toHaveTextContent("import"));
      await user.keyboard("{Meta>}g{/Meta}");
      await screen.findByText(/^2 of \d+$/);
      await waitFor(() => expect(document.querySelector("[data-find-current]")).toHaveTextContent("return"));
    });

    it("reaching a match opens its collapsed file without unmarking it viewed", async () => {
      const user = userEvent.setup();
      renderPage();
      await opened();
      await user.keyboard("]v");
      expect(screen.queryByText("How to review a pull request.")).not.toBeInTheDocument();

      await search(user, "How to review");
      await screen.findByText("1 of 1");
      expect(await screen.findByText("How to review a pull request.")).toBeInTheDocument();
      expect(screen.getByRole("checkbox", { name: "Viewed docs/review.md" })).toBeChecked();
    });

    it("a match on a collapsed file's path leaves it collapsed", async () => {
      const user = userEvent.setup();
      renderPage();
      await opened();
      await user.keyboard("]v");

      await search(user, "docs/review");
      await screen.findByText("1 of 1");
      await new Promise((resolve) => requestAnimationFrame(resolve));
      expect(screen.getByRole("button", { name: "Toggle docs/review.md" })).toHaveAttribute("aria-expanded", "false");
    });

    it("follows the comment filter, and re-runs on another tab", async () => {
      const user = userEvent.setup();
      renderPage();
      await opened();

      await search(user, "Typo in the heading");
      await screen.findByText("1 of 1");
      await user.click(screen.getByRole("button", { name: /^Unresolved/ }));
      await screen.findByText("No results");
      await user.click(screen.getByRole("button", { name: /^All comments/ }));
      await screen.findByText("1 of 1");

      await user.click(screen.getByRole("radio", { name: /^Conversation/ }));
      await waitFor(() => expect(findInput()).toHaveValue("Typo in the heading"));
      await screen.findByText("1 of 1");
      // Only the conversation's own text: nothing from the other tab's rows.
      await search(user, "How to review");
      await screen.findByText("No results");
    });

    it("finds a phrase across a bold word on the conversation tab", async () => {
      const user = userEvent.setup();
      renderPage();
      await opened();
      await user.click(screen.getByRole("radio", { name: /^Conversation/ }));

      await search(user, "feels arbitrary. Can");
      await screen.findByText("1 of 1");
    });

    it("Esc closes it and hands focus back; ⌘F keeps the last query", async () => {
      const user = userEvent.setup();
      renderPage();
      await opened();
      const viewed = screen.getByRole("checkbox", { name: "Viewed src/lib/loader.ts" });
      viewed.focus();

      await search(user, "readFileSync");
      await screen.findByText(/^1 of \d+$/);
      await user.keyboard("{Escape}");
      expect(screen.queryByRole("textbox", { name: "Find on this tab" })).not.toBeInTheDocument();
      expect(viewed).toHaveFocus();
      expect(document.querySelector("[data-find-current]")).toBeNull();

      await user.keyboard("{Meta>}f{/Meta}");
      expect(findInput()).toHaveValue("readFileSync");
    });

    it("Enter in the bar moves, and with ⌘ it does not submit the review", async () => {
      const user = userEvent.setup();
      renderPage();
      await opened();

      await search(user, "readFileSync");
      await screen.findByText(/^1 of \d+$/);
      await user.keyboard("{Meta>}{Enter}{/Meta}");
      await screen.findByText(/^2 of \d+$/);
      expect(commands.reviewSubmitPendingReview).not.toHaveBeenCalled();

      // Out of the bar, the same keys still submit.
      await user.keyboard("{Escape}");
      await user.keyboard("{Meta>}{Enter}{/Meta}");
      await waitFor(() => expect(commands.reviewSubmitPendingReview).toHaveBeenCalled());
    });

    it("typing in the bar doesn't run the page's letter shortcuts", async () => {
      const user = userEvent.setup();
      renderPage();
      await opened();

      await search(user, "v]");
      expect(screen.getByRole("checkbox", { name: "Viewed src/lib/loader.ts" })).not.toBeChecked();
      expect(screen.getByText("Viewed 0 of 2")).toBeInTheDocument();
    });
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
    expect(onOpenPr).toHaveBeenCalledWith(403, "walk");
  });

  it("offers only a comment on the viewer's own PR, which GitHub takes from its author", async () => {
    renderPage({ inbox: { ...deskInbox(repoDir), viewer: "mira-k" } });
    await opened();

    expect(within(yourReview()).queryByRole("radiogroup", { name: "Verdict" })).toBeNull();
    expect(within(yourReview()).getByRole("button", { name: "Submit review" })).toBeDisabled();
  });

  it("submits a bare approval, with no comment or summary", async () => {
    const user = userEvent.setup();
    renderPage({ inbox: deskInbox(repoDir) });
    await opened();

    await user.click(within(yourReview()).getByRole("radio", { name: "Approve" }));
    await user.click(within(yourReview()).getByRole("button", { name: "Submit review" }));

    await waitFor(() => expect(commands.reviewSubmitPendingReview).toHaveBeenCalledTimes(1));
    expect(await commands.reviewPendingReview(repoDir, THREADED_PR)).toBeNull();
    expect(within(yourReview()).queryByText(/no pending review/)).toBeNull();
  });

  describe("when the PR moves to a new head", () => {
    /** The next read of the PR answers with a new head, `path` changed in it. */
    async function pushTo(path: string) {
      const original = vi.mocked(commands.reviewPrDetail).getMockImplementation();
      vi.mocked(commands.reviewPrDetail).mockImplementation(async (...args) => {
        const read = (await original?.(...args)) as PrDetail;
        return {
          ...read,
          headSha: "pushedhead",
          files: read.files.map((file) => (file.path === path ? { ...file, blobSha: "changed" } : file)),
        };
      });
      invalidate({ names: ["reviewPrDetail"] });
      return () => vi.mocked(commands.reviewPrDetail).mockImplementation(original as never);
    }

    it("unmarks and unfolds a viewed file that changed, and keeps one that didn't", async () => {
      const user = userEvent.setup();
      renderPage();
      await opened();
      await user.click(screen.getByRole("checkbox", { name: "Viewed src/lib/loader.ts" }));
      await user.click(screen.getByRole("checkbox", { name: "Viewed docs/review.md" }));
      expect(screen.getByText("Viewed 2 of 2")).toBeInTheDocument();

      const restore = await pushTo("src/lib/loader.ts");
      await waitFor(() => expect(screen.getByText("Viewed 1 of 2")).toBeInTheDocument());
      expect(screen.getByRole("checkbox", { name: "Viewed src/lib/loader.ts" })).not.toBeChecked();
      expect(screen.getByRole("checkbox", { name: "Viewed docs/review.md" })).toBeChecked();
      expect(screen.getByRole("button", { name: /src\/lib\/loader\.ts/ })).toHaveAttribute("aria-expanded", "true");
      restore();
    });

    it("re-reads the changes since my review at the new head", async () => {
      prPlace(repoDir, THREADED_PR).changes = "since";
      renderPage();
      await waitFor(() => expect(commands.reviewChangesSince).toHaveBeenCalledTimes(1));

      const restore = await pushTo("src/lib/loader.ts");
      await waitFor(() =>
        expect(commands.reviewChangesSince).toHaveBeenLastCalledWith(repoDir, THREADED_PR, expect.anything(), "pushedhead"),
      );
      restore();
    });
  });

  it("opens on the changes since my review when the inbox asks", async () => {
    prPlace(repoDir, THREADED_PR).changes = "since";
    renderPage();
    await waitFor(() => expect(commands.reviewChangesSince).toHaveBeenCalledTimes(1));
    expect(await screen.findByRole("radio", { name: /since/i, checked: true })).toBeInTheDocument();
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
