import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import ReviewView from "$lib/review/ReviewView.svelte";
import { commands } from "$lib/services/homeRouting";

vi.mock("$lib/services/homeRouting", async (importOriginal) => {
  const actual = await importOriginal<typeof import("$lib/services/homeRouting")>();
  return {
    ...actual,
    commands: {
      ...actual.commands,
      startTurn: vi.fn(async () => ({ id: "turn-1", items: [], status: "inProgress" })),
    },
  };
});

// In browser mode (isTauri() === false) the review API serves preview fixtures,
// so the whole view renders without Tauri.
describe("ReviewView", () => {
  it("lists open PRs and opens one onto the review page", async () => {
    const user = userEvent.setup();
    render(ReviewView, { repoDir: "/repo", repoName: "repo", onBack: vi.fn(), onAskCodex: vi.fn() });

    await user.click(await screen.findByText(/Add pull-request review view/));

    await screen.findByRole("heading", { name: "Add pull-request review view" });
    expect(screen.getByRole("region", { name: "src/lib/loader.ts" })).toBeInTheDocument();
    expect(screen.getByRole("complementary", { name: "Your review" })).toBeInTheDocument();
  });

  it("asks an agent about selected lines in a thread filed under the PR", async () => {
    const user = userEvent.setup();
    const onAskCodex = vi.fn();
    render(ReviewView, { repoDir: "/repo", repoName: "repo", initialPr: 128, onBack: vi.fn(), onAskCodex });

    const loader = within(await screen.findByRole("region", { name: "src/lib/loader.ts" }));
    await user.click(loader.getByRole("button", { name: "Select line 4" }));
    await user.click(
      within(screen.getByRole("toolbar", { name: "Line 4" })).getByRole("button", { name: "Ask agent" }),
    );

    await waitFor(() => expect(commands.startTurn).toHaveBeenCalledOnce());
    expect(onAskCodex).not.toHaveBeenCalled();
    const [threadId, input, options] = vi.mocked(commands.startTurn).mock.calls[0];
    const prompt = (input as { text: string }[])[0].text;
    expect(prompt).toContain("src/lib/loader.ts:4");
    expect(prompt).toContain("if (!path) {");
    // Filed under the PR, and read-only at No checkout.
    expect((await commands.reviewPrLinks()).find((link) => link.threadId === threadId)?.prNumber).toBe(128);
    expect(options).toMatchObject({ sandboxMode: "read-only" });
  });

  it("goes back from a PR to the picker", async () => {
    const user = userEvent.setup();
    render(ReviewView, { repoDir: "/repo", repoName: "repo", initialPr: 128, onBack: vi.fn(), onAskCodex: vi.fn() });

    await screen.findByRole("heading", { name: "Add pull-request review view" });
    await user.click(screen.getByRole("button", { name: "Pull requests" }));
    await waitFor(() => expect(screen.getByText(/Native git worktree service/)).toBeInTheDocument());
  });
});
