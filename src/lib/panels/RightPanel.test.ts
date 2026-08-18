import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import RightPanel from "$lib/panels/RightPanel.svelte";
import type { FileUpdateChange, SideQuestion } from "$lib/types";

const sideQuestions: SideQuestion[] = [
  { sideThreadId: "side-1", parentThreadId: "parent-1", title: "Why trailing edge?", createdAt: 10 },
  { sideThreadId: "side-2", parentThreadId: "other-parent", title: "Unrelated", createdAt: 20 },
];

function setup(
  view: any,
  parentThreadId = "parent-1",
  changes: FileUpdateChange[] = [],
  extra: Record<string, unknown> = {},
) {
  const onClose = vi.fn();
  const onDataChanged = vi.fn();
  render(RightPanel, {
    view,
    parentThreadId,
    sideQuestions,
    changes,
    cwd: "/project",
    onClose,
    onDataChanged,
    ...extra,
  });
  return { onClose, onDataChanged };
}

describe("RightPanel", () => {
  it("renders the plan view as markdown and closes", async () => {
    const user = userEvent.setup();
    const { onClose } = setup({ kind: "plan", text: "## The plan\n- step one" });

    expect(screen.getByText("The plan")).toBeInTheDocument();
    expect(screen.getByText("step one")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Close panel" }));
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("shows an implement button under the plan when a handler is provided", async () => {
    const user = userEvent.setup();
    const onImplementPlan = vi.fn();
    setup({ kind: "plan", text: "## The plan" }, "parent-1", [], { onImplementPlan });

    await user.click(screen.getByRole("button", { name: "Implement plan" }));
    expect(onImplementPlan).toHaveBeenCalledOnce();
  });

  it("disables the implement button while a turn is running", () => {
    setup({ kind: "plan", text: "## The plan" }, "parent-1", [], { onImplementPlan: vi.fn(), implementDisabled: true });

    expect(screen.getByRole("button", { name: "Implement plan" })).toBeDisabled();
  });

  it("hides the implement button when no handler is provided", () => {
    setup({ kind: "plan", text: "## The plan" });

    expect(screen.queryByRole("button", { name: "Implement plan" })).not.toBeInTheDocument();
  });

  it("renders the sources view with each query", () => {
    setup({ kind: "sources", queries: ["debounce typescript", "svelte runes"] });

    expect(screen.getByText("debounce typescript")).toBeInTheDocument();
    expect(screen.getByText("svelte runes")).toBeInTheDocument();
  });

  it("renders the diffs view with one diff block per changed file", () => {
    setup({ kind: "diffs" }, "parent-1", [
      { path: "src/lib/api.ts", kind: { type: "update" }, diff: "+added line\n-removed line" },
      { path: "README.md", kind: { type: "add" }, diff: "+# Title" },
    ]);

    expect(screen.getByText("Outputs")).toBeInTheDocument();
    expect(screen.getByText("src/lib/api.ts")).toBeInTheDocument();
    expect(screen.getByText("README.md")).toBeInTheDocument();
  });

  it("shows an empty state in the diffs view when nothing changed", () => {
    setup({ kind: "diffs" });

    expect(screen.getByText("No file changes in this thread.")).toBeInTheDocument();
  });

  it("renders the files view as an expandable tree", async () => {
    const user = userEvent.setup();
    setup({ kind: "files" });

    // Preview file listing resolves asynchronously; README.md is at the root.
    expect(await screen.findByText("README.md")).toBeInTheDocument();
    expect(screen.queryByText("api.ts")).not.toBeInTheDocument();
    await user.click(screen.getByText("src"));
    await user.click(screen.getByText("lib"));
    expect(screen.getByText("api.ts")).toBeInTheDocument();
  });

  it("lists only this thread's side questions", () => {
    setup({ kind: "side" });

    expect(screen.getByText("Why trailing edge?")).toBeInTheDocument();
    expect(screen.queryByText("Unrelated")).not.toBeInTheDocument();
  });

  it("creates a side question via fork on first ask", async () => {
    const user = userEvent.setup();
    const { onDataChanged } = setup({ kind: "side" });

    await user.type(screen.getByPlaceholderText("Ask a side question…"), "What about edge cases?{Enter}");

    expect(onDataChanged).toHaveBeenCalled();
    expect(screen.getByText("What about edge cases?")).toBeInTheDocument();
  });

  it("shows a stop button while a side answer streams and interrupts on click", async () => {
    const user = userEvent.setup();
    setup({ kind: "side" });

    await user.type(screen.getByPlaceholderText("Ask a side question…"), "Stream please{Enter}");

    const stopButton = await screen.findByRole("button", { name: "Stop side question" });
    await user.click(stopButton);
    expect(await screen.findByRole("button", { name: "Ask side question" })).toBeInTheDocument();
  });

  it("resets the open side question when the thread changes", async () => {
    const user = userEvent.setup();
    const { rerender } = render(RightPanel, {
      view: { kind: "side" },
      parentThreadId: "parent-1",
      sideQuestions,
      changes: [],
      cwd: "/project",
      onClose: vi.fn(),
      onDataChanged: vi.fn(),
    });

    await user.type(screen.getByPlaceholderText("Ask a side question…"), "First question{Enter}");
    expect(screen.getByText("First question")).toBeInTheDocument();
    expect(screen.getByPlaceholderText("Follow up…")).toBeInTheDocument();

    await rerender({ parentThreadId: "other-parent" });

    // Back to the new thread's list; the composer starts a fresh side question.
    expect(screen.queryByText("First question")).not.toBeInTheDocument();
    expect(screen.getByPlaceholderText("Ask a side question…")).toBeInTheDocument();
    expect(screen.getByText("Unrelated")).toBeInTheDocument();
  });
});
