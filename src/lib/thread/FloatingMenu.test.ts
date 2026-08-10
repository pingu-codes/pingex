import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import FloatingMenu from "$lib/thread/FloatingMenu.svelte";
import type { FileUpdateChange } from "$lib/types";

const change = (path: string, kind = "update"): FileUpdateChange => ({
  path,
  kind: { type: kind },
  diff: `+${path}`,
});

function setup(overrides: Partial<Record<string, unknown>> = {}) {
  const handlers = {
    onOpenFinder: vi.fn(),
    onOpenZed: vi.fn(),
    onShowPlan: vi.fn(),
    onShowSources: vi.fn(),
    onShowSideQuestions: vi.fn(),
    onShowDiff: vi.fn(),
    onShowFiles: vi.fn(),
    onShowMessageLog: vi.fn(),
    onOpenSubagent: vi.fn(),
  };
  render(FloatingMenu, {
    plan: "## Ship the feature\n1. do it",
    outputs: [change("src/lib/api.ts"), change("README.md", "add")],
    sources: ["how to debounce"],
    sideQuestionCount: 2,
    ...handlers,
    ...overrides,
  });
  return handlers;
}

describe("FloatingMenu", () => {
  it("opens the project in Finder or Zed from the Open in menu", async () => {
    const user = userEvent.setup();
    const { onOpenFinder, onOpenZed } = setup();

    await user.click(screen.getByRole("button", { name: "Open in" }));
    await user.click(screen.getByRole("menuitem", { name: "Finder" }));
    expect(onOpenFinder).toHaveBeenCalledOnce();

    await user.click(screen.getByRole("button", { name: "Open in" }));
    await user.click(screen.getByRole("menuitem", { name: "Zed" }));
    expect(onOpenZed).toHaveBeenCalledOnce();
  });

  it("shows plan, outputs, sources, and side question sections in the default-open overview panel", async () => {
    const user = userEvent.setup();
    const { onShowPlan, onShowSideQuestions, onShowDiff } = setup();

    expect(screen.getByText("Ship the feature")).toBeInTheDocument();
    expect(screen.getByText("api.ts")).toBeInTheDocument();
    expect(screen.getByText("Web search")).toBeInTheDocument();
    // Both the side question count and the Outputs count read "2" here.
    expect(screen.getByRole("button", { name: /Side questions 2/ })).toBeInTheDocument();

    await user.click(screen.getByText("api.ts"));
    expect(onShowDiff).toHaveBeenCalledWith("src/lib/api.ts");

    await user.click(screen.getByText("Ship the feature"));
    expect(onShowPlan).toHaveBeenCalledOnce();

    // The panel stays open after actions, so side questions are reachable
    // without reopening it.
    await user.click(screen.getByText("Side questions"));
    expect(onShowSideQuestions).toHaveBeenCalledOnce();
    expect(screen.getByText("Ship the feature")).toBeInTheDocument();
  });

  it("toggles the overview panel closed and open again", async () => {
    const user = userEvent.setup();
    setup();

    expect(screen.getByText("Ship the feature")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Thread overview" }));
    expect(screen.queryByText("Ship the feature")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Thread overview" }));
    expect(screen.getByText("Ship the feature")).toBeInTheDocument();
  });

  it("lists nested subagents with resolved model, effort, state, and navigation", async () => {
    const user = userEvent.setup();
    const { onOpenSubagent } = setup({
      subagents: [
        {
          id: "agent-1",
          parentThreadId: "parent-1",
          title: "Research",
          cwd: "/project",
          status: "running",
          agentNickname: "Scout",
          agentRole: "researcher",
          model: "gpt-5.6-terra",
          reasoningEffort: "high",
        },
        {
          id: "agent-2",
          parentThreadId: "agent-1",
          title: "Review",
          cwd: "/project",
          status: "completed",
          agentNickname: null,
          agentRole: "reviewer",
          model: "gpt-5.6-sol",
          reasoningEffort: "xhigh",
        },
      ],
    });

    expect(screen.getByText("Scout")).toBeInTheDocument();
    expect(screen.getByText("gpt-5.6-terra")).toBeInTheDocument();
    expect(screen.getByText("Active")).toBeInTheDocument();
    expect(screen.getByText("Finished")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Open subagent Scout" }));
    expect(onOpenSubagent).toHaveBeenCalledWith(expect.objectContaining({ id: "agent-1" }));
  });

  it("opens the file tree from the Files button below side questions", async () => {
    const user = userEvent.setup();
    const { onShowFiles } = setup();

    await user.click(screen.getByText("Files"));
    expect(onShowFiles).toHaveBeenCalledOnce();
  });

  it("labels created and edited files alike, rather than listing paths bare", () => {
    setup();

    expect(screen.getByText("api.ts")).toBeInTheDocument();
    expect(screen.getByText("Edited")).toBeInTheDocument();
    expect(screen.getByText("README.md")).toBeInTheDocument();
    expect(screen.getByText("New")).toBeInTheDocument();
  });

  it("lists every touched file and still offers the all-changed-files shortcut", async () => {
    const user = userEvent.setup();
    const { onShowDiff } = setup({
      outputs: ["a.ts", "b.ts", "c.ts", "d.ts", "e.ts", "f.ts"]
        .map((path) => change(path))
        .concat(change("late-edit.ts")),
    });

    // The seventh entry used to fall off the end of the list, which is what
    // made a late edit look like it was never made.
    expect(screen.getByText("late-edit.ts")).toBeInTheDocument();
    await user.click(screen.getByText("All 7 changed files"));
    expect(onShowDiff).toHaveBeenCalledWith(null);
  });

  it("shows empty states when the thread has no plan or outputs", () => {
    setup({ plan: null, outputs: [], sources: [], sideQuestionCount: 0 });

    expect(screen.getByText("No plan in this thread.")).toBeInTheDocument();
    expect(screen.getByText("No files changed yet.")).toBeInTheDocument();
    expect(screen.getByText("No web searches.")).toBeInTheDocument();
    expect(screen.getByText("No commands run yet.")).toBeInTheDocument();
  });

  describe("processes", () => {
    const process = (overrides: Partial<Record<string, unknown>> = {}) => ({
      key: "t:c1",
      threadId: "t",
      turnId: "turn-1",
      itemId: "c1",
      command: "sleep 60",
      cwd: "/repo",
      status: "running",
      startedAt: Date.now(),
      finishedAt: null,
      output: "",
      exitCode: null,
      ...overrides,
    });

    it("lists a running process and opens it on click", async () => {
      const user = userEvent.setup();
      const onOpenProcess = vi.fn();
      setup({ processes: [process()], currentThreadId: "t", onOpenProcess });

      const row = screen.getByRole("button", { name: "Open process sleep 60" });
      expect(row).toHaveTextContent("sleep 60");
      expect(row).toHaveTextContent("Active");
      await user.click(row);
      expect(onOpenProcess).toHaveBeenCalledWith(expect.objectContaining({ key: "t:c1" }));
    });

    it("marks a process from another thread and shows its exit code once finished", async () => {
      const user = userEvent.setup();
      setup({
        processes: [process({ key: "o:c2", threadId: "o", status: "completed", exitCode: 0 })],
        currentThreadId: "t",
      });

      // Finished processes are hidden until the Finished chip is toggled on.
      expect(screen.getByText("No matching processes.")).toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: "Finished" }));

      const row = screen.getByRole("button", { name: "Open process sleep 60" });
      expect(row).toHaveTextContent("Finished");
      expect(row).toHaveTextContent("exit 0");
      expect(row).toHaveTextContent("other thread");
    });

    it("filters processes with the state chips, showing only active by default", async () => {
      const user = userEvent.setup();
      setup({
        processes: [
          process(),
          process({ key: "t:c2", itemId: "c2", command: "npm test", status: "completed", exitCode: 0 }),
          process({ key: "t:c3", itemId: "c3", command: "bad cmd", status: "failed", exitCode: 1 }),
        ],
        currentThreadId: "t",
      });

      expect(screen.getByRole("button", { name: "Open process sleep 60" })).toBeInTheDocument();
      expect(screen.queryByRole("button", { name: "Open process npm test" })).not.toBeInTheDocument();
      expect(screen.queryByRole("button", { name: "Open process bad cmd" })).not.toBeInTheDocument();

      await user.click(screen.getByRole("button", { name: "Failed" }));
      expect(screen.getByRole("button", { name: "Open process bad cmd" })).toBeInTheDocument();
      expect(screen.queryByRole("button", { name: "Open process npm test" })).not.toBeInTheDocument();

      // Toggling Active off hides the running process again.
      await user.click(screen.getByRole("button", { name: "Active" }));
      expect(screen.queryByRole("button", { name: "Open process sleep 60" })).not.toBeInTheDocument();
    });
  });
});
