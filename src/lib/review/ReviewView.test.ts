import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import ReviewView from "$lib/review/ReviewView.svelte";

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

  it("asks an agent about selected lines with a prompt naming them", async () => {
    const user = userEvent.setup();
    const onAskCodex = vi.fn();
    render(ReviewView, { repoDir: "/repo", repoName: "repo", initialPr: 128, onBack: vi.fn(), onAskCodex });

    const loader = within(await screen.findByRole("region", { name: "src/lib/loader.ts" }));
    await user.click(loader.getByRole("button", { name: "Select line 4" }));
    await user.click(
      within(screen.getByRole("toolbar", { name: "Line 4" })).getByRole("button", { name: "Ask agent" }),
    );

    expect(onAskCodex).toHaveBeenCalledOnce();
    const [cwd, prompt] = onAskCodex.mock.calls[0];
    expect(cwd).toBe("/repo");
    expect(prompt).toContain("src/lib/loader.ts:4");
    expect(prompt).toContain("if (!path) {");
  });

  it("goes back from a PR to the picker", async () => {
    const user = userEvent.setup();
    render(ReviewView, { repoDir: "/repo", repoName: "repo", initialPr: 128, onBack: vi.fn(), onAskCodex: vi.fn() });

    await screen.findByRole("heading", { name: "Add pull-request review view" });
    await user.click(screen.getByRole("button", { name: "Pull requests" }));
    await waitFor(() => expect(screen.getByText(/Native git worktree service/)).toBeInTheDocument());
  });
});
