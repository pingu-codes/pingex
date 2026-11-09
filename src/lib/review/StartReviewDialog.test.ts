import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { checkoutTarget } from "$lib/review/checkoutTarget";
import StartReviewDialog from "$lib/review/StartReviewDialog.svelte";
import { inboxPr } from "$lib/services/preview/factories";
import type { HarnessKind } from "$lib/types";

const pr = inboxPr({
  number: 399,
  title: "Readable theme focus rings",
  author: "jo-r",
  headRef: "jo-r/focus-rings",
  headSha: "7e2c1f0".padEnd(40, "0"),
  additions: 38,
  deletions: 12,
});

function open(harnesses: HarnessKind[] = ["codex", "claude"], refusedCheckout: string | null = null) {
  const close = vi.fn();
  render(StartReviewDialog, {
    props: {
      refusedCheckout,
      pr,
      target: checkoutTarget(pr, pr.author === "ciaran"),
      worktreePath: "~/.pingex/pr-worktrees/pingex/pr-399",
      setupCommand: "bun install",
      harnesses,
      close,
    },
  });
  return close;
}

const code = (name: RegExp) => screen.getByRole("radio", { name });

describe("Start review", () => {
  it("shows the PR and starts a diff review with drafts by default", async () => {
    const close = open();
    expect(screen.getByText("Review #399")).toBeInTheDocument();
    expect(screen.getByText(/jo-r · Readable theme focus rings · \+38 −12/)).toBeInTheDocument();
    expect(code(/^No checkout/)).toBeChecked();
    expect(screen.getByText("~/.pingex/pr-worktrees/pingex/pr-399")).toBeInTheDocument();
    expect(screen.getByRole("switch", { name: "Keep findings as drafts" })).toBeChecked();
    expect(screen.getByRole("switch", { name: "Re-run when new commits land" })).not.toBeChecked();

    await userEvent.click(screen.getByRole("button", { name: "Start review" }));
    expect(close).toHaveBeenCalledWith({
      kind: "start",
      request: expect.objectContaining({
        targets: [expect.objectContaining({ number: 399, headRef: "jo-r/focus-rings", mine: false })],
        harness: "codex",
        step: "noCheckout",
        focus: ["correctness", "tests"],
        keepDrafts: true,
        rerunOnNewCommits: false,
      }),
    });
  });

  it("switches the code step to a worktree when Run tests is picked", async () => {
    const close = open();
    await userEvent.click(screen.getByRole("button", { name: "Run tests" }));
    expect(code(/^Worktree/)).toBeChecked();
    expect(screen.getByRole("button", { name: "Run tests" })).toHaveAttribute("aria-pressed", "true");

    // Back to No checkout: the dialog drops what it can no longer do.
    await userEvent.click(code(/^No checkout/));
    expect(screen.getByRole("button", { name: "Run tests" })).toHaveAttribute("aria-pressed", "false");

    await userEvent.click(screen.getByRole("button", { name: "Run tests" }));
    await userEvent.click(screen.getByRole("switch", { name: "Re-run when new commits land" }));
    await userEvent.click(screen.getByRole("button", { name: "Start review" }));
    expect(close).toHaveBeenCalledWith({
      kind: "start",
      request: expect.objectContaining({
        step: "worktree",
        focus: ["correctness", "tests", "runTests"],
        rerunOnNewCommits: true,
      }),
    });
  });

  it("runs a harness without a read-only sandbox in a worktree, tests included", async () => {
    const close = open(["codex", "maki"]);
    await userEvent.click(screen.getByRole("radio", { name: "maki" }));
    expect(code(/^No checkout/)).toBeDisabled();
    expect(code(/^Worktree/)).toBeChecked();
    await userEvent.click(screen.getByRole("button", { name: "Run tests" }));
    await userEvent.click(screen.getByRole("button", { name: "Start review" }));
    expect(close).toHaveBeenCalledWith({
      kind: "start",
      request: expect.objectContaining({
        harness: "maki",
        step: "worktree",
        focus: ["correctness", "tests", "runTests"],
      }),
    });
  });

  it("offers no worktree or tests when the repository turned PR worktrees off", () => {
    open(["codex"], "PR worktrees are turned off for this repository");
    expect(code(/^Worktree/)).toBeDisabled();
    expect(screen.getByRole("button", { name: "Run tests" })).toBeDisabled();
  });

  it("opens the PR without an agent", async () => {
    const close = open();
    await userEvent.click(screen.getByRole("button", { name: "Open without agent" }));
    expect(close).toHaveBeenCalledWith({ kind: "open" });
  });
});
