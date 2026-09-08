import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import WorkItem from "$lib/thread/WorkItem.svelte";
import type { SubagentDetail, ThreadItem } from "$lib/types";

const openSubagent = vi.fn();
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
    const item: ThreadItem = { type: "subAgentActivity", id: "a2", kind: "started", agentThreadId: "child-9", agentPath: "/root/x" };
    render(WorkItem, { item, subagents: [agent] });
    expect(screen.getByText("Spawned agent")).toBeInTheDocument();
    expect(screen.getByText("/root/x")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Open thread" })).toBeNull();
  });

  it("does not show a live status once the agent has finished", () => {
    const item: ThreadItem = { type: "subAgentActivity", id: "a3", kind: "completed", agentThreadId: "child-1", agentPath: "/root/x" };
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
