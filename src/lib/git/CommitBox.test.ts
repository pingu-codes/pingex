import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import CommitBox from "./CommitBox.svelte";

describe("CommitBox", () => {
  it("only commits with staged files and a message", async () => {
    const onCommit = vi.fn(async () => true);
    const { rerender } = render(CommitBox, { stagedCount: 0, identityName: "Test", identityEmail: "t@x", onCommit });
    const button = () => screen.getByRole("button", { name: /^Commit/ });
    expect(button()).toBeDisabled();

    await rerender({ stagedCount: 2, identityName: "Test", identityEmail: "t@x", onCommit });
    expect(button()).toBeDisabled();
    await userEvent.type(screen.getByRole("textbox", { name: "Commit message" }), "feat: thing");
    expect(button()).toBeEnabled();
    await userEvent.click(button());
    expect(onCommit).toHaveBeenCalledWith("feat: thing");
    expect((screen.getByRole("textbox", { name: "Commit message" }) as HTMLTextAreaElement).value).toBe("");
  });

  it("shows the author, the last commit and its hook output", async () => {
    render(CommitBox, {
      stagedCount: 0,
      identityName: "Test",
      identityEmail: "t@x",
      lastCommit: {
        hash: "abc",
        shortHash: "abc1234",
        subject: "did it",
        authorName: "Test",
        authorEmail: "t@x",
        hookOutput: "pre-commit: ok",
      },
      onCommit: vi.fn(async () => true),
    });
    expect(screen.getByText(/Committing as/)).toBeTruthy();
    expect(screen.getByText("abc1234")).toBeTruthy();
    await userEvent.click(screen.getByRole("button", { name: /Hook output/ }));
    expect(screen.getByText("pre-commit: ok")).toBeTruthy();
  });

  it("warns when no identity is configured and surfaces hook errors", () => {
    render(CommitBox, {
      stagedCount: 1,
      error: { kind: "hookRejected", message: "A Git hook rejected the operation.", detail: "lint failed" },
      onCommit: vi.fn(async () => false),
    });
    expect(screen.getByText(/No git identity configured/)).toBeTruthy();
    expect(screen.getByText("A Git hook rejected the operation.")).toBeTruthy();
    expect(screen.getByText("lint failed")).toBeTruthy();
  });
});
