import { render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import ReviewView from "$lib/review/ReviewView.svelte";
import { reviewLoadDraft, reviewPrDetail } from "$lib/services/api";

vi.mock("$lib/services/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("$lib/services/api")>();
  return {
    ...actual,
    reviewPrDetail: vi.fn(actual.reviewPrDetail),
    reviewLoadDraft: vi.fn(actual.reviewLoadDraft),
  };
});

// In browser mode (isTauri() === false) the review API serves preview fixtures,
// so the whole view renders without Tauri.
describe("ReviewView", () => {
  it("lists open PRs and opens one into the three-pane view", async () => {
    const user = userEvent.setup();
    render(ReviewView, { repoDir: "/repo", repoName: "repo", onBack: vi.fn(), onAskCodex: vi.fn() });

    // Picker lists the fixture PRs.
    await waitFor(() => expect(screen.getByText(/Add pull-request review view/)).toBeInTheDocument());

    await user.click(screen.getByText(/Add pull-request review view/));

    // Three-pane view: the changed file and the review panel are present.
    await waitFor(() => expect(screen.getAllByText("src/lib/loader.ts").length).toBeGreaterThan(0));
    expect(screen.getByRole("button", { name: "Ask Codex" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Start review" })).toBeInTheDocument();
  });

  it("asks Codex to review with a prompt containing the diff", async () => {
    const user = userEvent.setup();
    const onAskCodex = vi.fn();
    render(ReviewView, { repoDir: "/repo", repoName: "repo", onBack: vi.fn(), onAskCodex });

    await waitFor(() => expect(screen.getByText(/Add pull-request review view/)).toBeInTheDocument());
    await user.click(screen.getByText(/Add pull-request review view/));
    await waitFor(() => expect(screen.getAllByText("src/lib/loader.ts").length).toBeGreaterThan(0));

    await user.click(screen.getByRole("button", { name: "Ask Codex" }));
    expect(onAskCodex).toHaveBeenCalledOnce();
    const [cwd, prompt] = onAskCodex.mock.calls[0];
    expect(cwd).toBe("/repo");
    expect(prompt).toContain("src/lib/loader.ts");
  });

  it("ignores a PR that finishes loading after the user opened another", async () => {
    const user = userEvent.setup();
    vi.mocked(reviewLoadDraft).mockClear();
    const actual = vi.mocked(reviewPrDetail).getMockImplementation()!;
    let releaseFirst!: () => void;
    vi.mocked(reviewPrDetail).mockImplementationOnce(
      (repoDir, number) => new Promise((resolve) => (releaseFirst = () => resolve(actual(repoDir, number)))),
    );
    render(ReviewView, { repoDir: "/repo", repoName: "repo", onBack: vi.fn(), onAskCodex: vi.fn() });
    await waitFor(() => expect(screen.getByText(/Native git worktree service/)).toBeInTheDocument());

    await user.click(screen.getByText(/Add pull-request review view/));
    await user.click(screen.getByText(/Native git worktree service/));
    await waitFor(() => expect(reviewLoadDraft).toHaveBeenCalledWith("github", "repo", 126));
    releaseFirst();

    await new Promise((resolve) => setTimeout(resolve, 0));
    // The stale PR's draft was never restored onto the PR now showing.
    expect(reviewLoadDraft).not.toHaveBeenCalledWith("github", "repo", 128);
  });
});
