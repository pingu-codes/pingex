import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { copyText } from "$lib/services/api";
import WorkItem from "$lib/thread/WorkItem.svelte";
import type { SubagentDetail, ThreadItem } from "$lib/types";

const openSubagent = vi.fn();
vi.mock("$lib/services/api", async (orig) => ({ ...(await orig<object>()), copyText: vi.fn(() => Promise.resolve()) }));
vi.mock("$lib/app/actions.svelte", () => ({ openSubagent: (agent: SubagentDetail) => openSubagent(agent) }));

const agent: SubagentDetail = {
  id: "child-1",
  parentThreadId: "t",
  title: "inspect_scaffold",
  cwd: "/repo",
  status: "running",
  agentNickname: "Scout",
  agentRole: "explorer",
  model: null,
  reasoningEffort: null,
};

describe("WorkItem — Codex subagent activity", () => {
  it("names the agent from the listing, shows its status and opens its thread", async () => {
    const user = userEvent.setup();
    const item: ThreadItem = {
      type: "subAgentActivity",
      id: "a1",
      kind: "interacted",
      agentThreadId: "child-1",
      agentPath: "/root/inspect_scaffold",
    };
    render(WorkItem, { item, subagents: [agent] });
    expect(screen.getByText("Messaged agent")).toBeInTheDocument();
    expect(screen.getByText("Scout")).toBeInTheDocument();
    expect(screen.getByText("running")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Open thread" }));
    expect(openSubagent).toHaveBeenCalledWith(expect.objectContaining({ id: "child-1" }));
  });

  it("falls back to the agent path before the listing knows the agent", () => {
    const item: ThreadItem = {
      type: "subAgentActivity",
      id: "a2",
      kind: "started",
      agentThreadId: "child-9",
      agentPath: "/root/x",
    };
    render(WorkItem, { item, subagents: [agent] });
    expect(screen.getByText("Spawned agent")).toBeInTheDocument();
    expect(screen.getByText("/root/x")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Open thread" })).toBeNull();
  });

  it("does not show a live status once the agent has finished", () => {
    const item: ThreadItem = {
      type: "subAgentActivity",
      id: "a3",
      kind: "completed",
      agentThreadId: "child-1",
      agentPath: "/root/x",
    };
    render(WorkItem, { item, subagents: [agent] });
    expect(screen.getByText("Agent finished")).toBeInTheDocument();
    expect(screen.queryByText("running")).toBeNull();
  });
});

describe("WorkItem — Codex collab tool calls", () => {
  const collab = (tool: string, receivers: string[]): ThreadItem => ({
    type: "collabAgentToolCall",
    id: `c-${tool}`,
    tool,
    receiverThreadIds: receivers,
  });

  it.each([
    ["spawnAgent", ["a", "b"], "Spawned 2 subagents"],
    ["sendInput", ["a"], "Sent input to a subagent"],
    ["wait", ["a", "b", "c"], "Waited for 3 subagents"],
    ["closeAgent", ["a"], "Closed a subagent"],
    ["listAgents", [], "Listed agents"],
    ["somethingNew", ["a"], "Agent call a subagent"],
  ])("labels a %s call", (tool, receivers, label) => {
    render(WorkItem, { item: collab(tool, receivers) });
    expect(screen.getByText(label)).toBeInTheDocument();
  });
});

describe("WorkItem — command execution", () => {
  const command = "rg -n 'export function' src/lib/utils.ts src/lib/services/api.ts src/lib/thread/ThreadView.svelte";
  const item: ThreadItem = {
    type: "commandExecution",
    id: "cmd-1",
    command,
    cwd: "/repo",
    status: "completed",
    exitCode: 0,
    durationMs: 84,
    aggregatedOutput: "12:export function a() {}\n",
  };

  it("shows the full command on hover and when expanded", async () => {
    const user = userEvent.setup();
    render(WorkItem, { item });
    expect(screen.getByTitle(command)).toBeInTheDocument();
    await user.click(screen.getByText(command, { selector: "code" }));
    expect(screen.getByText(command, { selector: "pre" })).toBeInTheDocument();
    expect(screen.getByText("/repo")).toBeInTheDocument();
  });

  it("copies the command verbatim", async () => {
    const user = userEvent.setup();
    render(WorkItem, { item });
    await user.click(screen.getByRole("button", { name: "Copy command" }));
    expect(copyText).toHaveBeenCalledWith(command);
  });
});

describe("async messages", () => {
  it("labels an async message and retains its copy controls", async () => {
    render(WorkItem, { item: { type: "agentMessage", id: "async", delivery: "async", text: "A question" } });
    expect(screen.getByText("Async")).toBeVisible();
    expect(screen.getByText("A question")).toBeVisible();
    await userEvent.setup().click(screen.getByRole("button", { name: "Copy message" }));
    expect(copyText).toHaveBeenCalledWith("A question");
  });
  it.each([undefined, null, "sync"])("does not label delivery %s as async", (delivery) => {
    render(WorkItem, { item: { type: "agentMessage", id: "normal", delivery, text: "An update" } });
    expect(screen.queryByText("Async")).toBeNull();
  });
});
