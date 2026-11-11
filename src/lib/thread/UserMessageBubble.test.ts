import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import UserMessageBubble from "$lib/thread/UserMessageBubble.svelte";
import { withWorktreeContext } from "$lib/thread/worktreeContext";
import type { ThreadItem } from "$lib/types";

const CONTEXT = 'This directory is the worktree of the stack "billing".';

function message(content: ThreadItem["content"]): ThreadItem {
  return { id: "item-1", type: "userMessage", content } as ThreadItem;
}

describe("UserMessageBubble worktree context", () => {
  it("folds the context away under a chip and shows only what the user typed", async () => {
    const user = userEvent.setup();
    const sent = withWorktreeContext([{ type: "text", text: "Fix the tax rounding" }], CONTEXT);
    render(UserMessageBubble, { item: message(sent), cwd: "/wt/billing", editable: false, onSubmitEdit: vi.fn() });

    expect(screen.getByText("Fix the tax rounding")).toBeVisible();
    expect(screen.queryByText(CONTEXT)).not.toBeInTheDocument();
    const chip = screen.getByRole("button", { name: /Worktree context/ });
    expect(chip).toHaveAttribute("aria-expanded", "false");

    await user.click(chip);

    expect(chip).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByText(CONTEXT)).toBeVisible();
  });

  it("shows no chip on a message sent without context", () => {
    render(UserMessageBubble, {
      item: message([{ type: "text", text: "Fix the tax rounding" }]),
      cwd: "/wt/billing",
      editable: false,
      onSubmitEdit: vi.fn(),
    });

    expect(screen.queryByRole("button", { name: /Worktree context/ })).not.toBeInTheDocument();
  });
});
