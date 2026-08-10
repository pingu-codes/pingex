import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import ReviewPanel from "$lib/review/ReviewPanel.svelte";
import type { PrComment } from "$lib/types";

function comment(overrides: Partial<PrComment> = {}): PrComment {
  return {
    id: 1,
    author: "dev",
    body: "hi",
    createdAt: "t",
    path: null,
    line: null,
    side: null,
    threadId: null,
    isResolved: false,
    ...overrides,
  };
}

function handlers() {
  return {
    onStartReview: vi.fn(),
    onSubmit: vi.fn(),
    onReply: vi.fn(),
    onResolve: vi.fn(),
    onRemovePending: vi.fn(),
    onAskCodex: vi.fn(),
  };
}

describe("ReviewPanel actions", () => {
  it("starts a review, then submits with the chosen event and body", async () => {
    const user = userEvent.setup();
    const h = handlers();
    const { rerender } = render(ReviewPanel, { comments: [], reviewStarted: false, ...h });

    await user.click(screen.getByRole("button", { name: "Start review" }));
    expect(h.onStartReview).toHaveBeenCalledOnce();

    // Host flips reviewStarted after start.
    await rerender({ comments: [], reviewStarted: true, ...h });

    await user.type(screen.getByLabelText("Review summary"), "Looks good");
    await user.selectOptions(screen.getByLabelText("Review action"), "approve");
    await user.click(screen.getByRole("button", { name: /Submit review/ }));
    expect(h.onSubmit).toHaveBeenCalledWith("approve", "Looks good");
  });

  it("blocks a comment review with an empty body but allows an empty approval", async () => {
    const user = userEvent.setup();
    const h = handlers();
    const { rerender } = render(ReviewPanel, { comments: [], reviewStarted: true, ...h });

    // Default event is "comment": submit is disabled with no body.
    expect(screen.getByRole("button", { name: /Submit review/ })).toBeDisabled();

    await user.selectOptions(screen.getByLabelText("Review action"), "approve");
    const submit = screen.getByRole("button", { name: /Submit review/ });
    expect(submit).toBeEnabled();
    await user.click(submit);
    expect(h.onSubmit).toHaveBeenCalledWith("approve", "");
    // Keep referencing rerender so the binding is retained for clarity.
    expect(rerender).toBeTypeOf("function");
  });

  it("replies to the newest comment in a thread", async () => {
    const user = userEvent.setup();
    const h = handlers();
    const comments = [
      comment({ id: 10, path: "a.ts", line: 4, side: "RIGHT", threadId: "T1" }),
      comment({ id: 11, path: "a.ts", line: 4, side: "RIGHT", threadId: "T1" }),
    ];
    render(ReviewPanel, { comments, reviewStarted: false, ...h });

    await user.click(screen.getByRole("button", { name: /Reply/ }));
    await user.type(screen.getByLabelText("Reply"), "thanks");
    await user.click(screen.getByRole("button", { name: /Send reply/ }));
    // Reply targets the last comment id in the thread.
    expect(h.onReply).toHaveBeenCalledWith(11, "thanks");
  });

  it("resolves a thread that has a real node id", async () => {
    const user = userEvent.setup();
    const h = handlers();
    const comments = [comment({ id: 20, path: "a.ts", line: 2, side: "RIGHT", threadId: "PRRT_node" })];
    render(ReviewPanel, { comments, reviewStarted: false, ...h });

    await user.click(screen.getByRole("button", { name: /Resolve/ }));
    expect(h.onResolve).toHaveBeenCalledWith("PRRT_node");
  });

  it("does not offer resolve for a synthesized path:line thread", () => {
    const h = handlers();
    const comments = [comment({ id: 30, path: "a.ts", line: 2, side: "RIGHT", threadId: null })];
    render(ReviewPanel, { comments, reviewStarted: false, ...h });
    expect(screen.queryByRole("button", { name: /Resolve/ })).not.toBeInTheDocument();
  });

  it("removes a pending comment and asks Codex to review", async () => {
    const user = userEvent.setup();
    const h = handlers();
    const pending = [{ path: "a.ts", line: 5, side: "RIGHT", body: "nit" }];
    render(ReviewPanel, { comments: [], pending, reviewStarted: false, ...h });

    await user.click(screen.getByRole("button", { name: "Ask Codex" }));
    expect(h.onAskCodex).toHaveBeenCalledOnce();

    await user.click(screen.getByRole("button", { name: "Remove pending comment" }));
    expect(h.onRemovePending).toHaveBeenCalledWith(0);
  });
});
