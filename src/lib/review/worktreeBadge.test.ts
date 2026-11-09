import { render, screen } from "@testing-library/svelte";
import { describe, expect, it } from "vitest";
import type { PrCheckout } from "$lib/types";
import WorktreeBadge from "./WorktreeBadge.svelte";

const checkout: PrCheckout = {
  number: 12,
  step: "worktree",
  path: "/worktrees/repo/feat-x",
  headSha: "abc",
  gitBranch: "feat/x",
  setupPending: null,
  ahead: 0,
  behind: 0,
  dirty: false,
  managed: false,
  onPrBranch: true,
  mainCheckout: false,
};

describe("WorktreeBadge", () => {
  it("draws nothing until the read has answered", () => {
    render(WorktreeBadge, { checkout: null, known: false, variant: "chip" });
    expect(screen.queryByRole("img")).toBeNull();
    expect(screen.queryByText("No local worktree")).toBeNull();
  });

  it("says there is no worktree once it knows", () => {
    render(WorktreeBadge, { checkout: null, known: true, variant: "chip" });
    expect(screen.getByRole("img", { name: /^No local worktree/ })).toHaveTextContent("No local worktree");
  });

  it("shows the folder's name on a page and the whole path to assistive tech", () => {
    render(WorktreeBadge, { checkout, known: true, variant: "chip" });
    const badge = screen.getByRole("img", { name: /\/worktrees\/repo\/feat-x/ });
    expect(badge).toHaveTextContent("feat-x");
    expect(badge).toHaveAttribute("data-worktree-state", "here");
  });

  it("is only an icon on a row, with the state in its label", () => {
    render(WorktreeBadge, { checkout, known: true, variant: "icon" });
    const badge = screen.getByRole("img", { name: /Worktree feat-x/ });
    expect(badge).toHaveTextContent("");
  });
});
