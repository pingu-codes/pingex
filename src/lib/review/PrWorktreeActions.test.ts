import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import PrWorktreeActions from "$lib/review/PrWorktreeActions.svelte";
import type { PrCheckout } from "$lib/types";

const worktree = (overrides: Partial<PrCheckout> = {}): PrCheckout => ({
  number: 12,
  step: "worktree",
  path: "/wt/pr-12",
  headSha: "abc",
  gitBranch: "pr/12",
  setupPending: null,
  ahead: 0,
  behind: 0,
  dirty: false,
  managed: true,
  onPrBranch: true,
  mainCheckout: false,
  ...overrides,
});

describe("a PR's worktree actions", () => {
  it("says nothing about a level, clean worktree but still offers work", () => {
    render(PrWorktreeActions, { checkout: worktree(), mine: false, onWork: vi.fn(), onPush: vi.fn() });
    expect(screen.getByRole("button", { name: "Work on this PR" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Push to PR" })).not.toBeInTheDocument();
    expect(screen.queryByText(/local commit|Uncommitted|Behind/)).not.toBeInTheDocument();
  });

  it("shows what the worktree holds that the PR doesn't, and pushes it", async () => {
    const onPush = vi.fn();
    render(PrWorktreeActions, {
      checkout: worktree({ ahead: 2, behind: 1, dirty: true }),
      mine: false,
      onWork: vi.fn(),
      onPush,
    });
    expect(screen.getByText("2 local commits")).toBeInTheDocument();
    expect(screen.getByText("Uncommitted changes")).toBeInTheDocument();
    expect(screen.getByText("Behind PR by 1")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Push to PR" }));
    expect(onPush).toHaveBeenCalledOnce();
  });

  it("makes work the primary action on my own PR", () => {
    render(PrWorktreeActions, { checkout: null, mine: true, onWork: vi.fn() });
    expect(screen.getByRole("button", { name: "Work on this PR" }).className).toContain("preset-filled-primary-500");
  });
});
