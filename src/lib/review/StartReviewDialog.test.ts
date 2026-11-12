import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import StartReviewDialog from "$lib/review/StartReviewDialog.svelte";
import { inboxPr } from "$lib/services/preview/factories";
import type { CheckoutTarget, HarnessKind } from "$lib/types";

const pr = inboxPr({
  number: 399,
  title: "Readable theme focus rings",
  author: "jo-r",
  headRef: "jo-r/focus-rings",
  headSha: "7e2c1f0".padEnd(40, "0"),
  additions: 38,
  deletions: 12,
});

const target: CheckoutTarget = {
  number: pr.number,
  headRef: pr.headRef,
  headSha: pr.headSha,
  mine: false,
  stack: null,
  headRepo: null,
  stackBranches: [],
};

function open(harnesses: HarnessKind[] = ["codex", "claude"]) {
  const close = vi.fn();
  render(StartReviewDialog, {
    props: { pr, target, worktreePath: "/home/me/.pingex/worktrees/pingex/pr-399", harnesses, close },
  });
  return close;
}

describe("Start review", () => {
  it("shows the PR and the worktree the agent works in, and starts a review with drafts by default", async () => {
    const close = open();
    expect(screen.getByText("Review #399")).toBeInTheDocument();
    expect(screen.getByText(/jo-r · Readable theme focus rings · \+38 −12/)).toBeInTheDocument();
    expect(screen.getByText("pr-399")).toHaveAttribute("title", "/home/me/.pingex/worktrees/pingex/pr-399");
    expect(screen.queryByRole("radio", { name: /No checkout/ })).toBeNull();
    expect(screen.getByRole("switch", { name: "Keep findings as drafts" })).toBeChecked();
    expect(screen.getByRole("switch", { name: "Re-run when new commits land" })).not.toBeChecked();

    await userEvent.click(screen.getByRole("button", { name: "Start review" }));
    expect(close).toHaveBeenCalledWith({
      kind: "start",
      request: {
        targets: [expect.objectContaining({ number: 399, headRef: "jo-r/focus-rings", mine: false })],
        harness: "codex",
        focus: ["correctness", "tests"],
        keepDrafts: true,
        rerunOnNewCommits: false,
      },
    });
  });

  it("runs tests and re-runs on new commits when asked, whatever the harness", async () => {
    const close = open(["codex", "maki"]);
    await userEvent.click(screen.getByRole("radio", { name: "maki" }));
    await userEvent.click(screen.getByRole("button", { name: "Run tests" }));
    expect(screen.getByRole("button", { name: "Run tests" })).toHaveAttribute("aria-pressed", "true");
    await userEvent.click(screen.getByRole("switch", { name: "Re-run when new commits land" }));
    await userEvent.click(screen.getByRole("button", { name: "Start review" }));
    expect(close).toHaveBeenCalledWith({
      kind: "start",
      request: expect.objectContaining({
        harness: "maki",
        focus: ["correctness", "tests", "runTests"],
        rerunOnNewCommits: true,
      }),
    });
  });

  it("opens the PR without an agent", async () => {
    const close = open();
    await userEvent.click(screen.getByRole("button", { name: "Open without agent" }));
    expect(close).toHaveBeenCalledWith({ kind: "open" });
  });
});
