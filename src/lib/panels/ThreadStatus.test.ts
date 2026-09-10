import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { beforeEach, describe, expect, it, vi } from "vitest";
import ThreadStatus from "$lib/panels/ThreadStatus.svelte";
import type { ContextStats } from "$lib/thread/contextUsage";
import type { ContextComposition, UsageBreakdown } from "$lib/types";

const { readContextBreakdown, readUsageBreakdown, readThreadUsage } = vi.hoisted(() => ({
  readContextBreakdown: vi.fn(),
  readUsageBreakdown: vi.fn(),
  readThreadUsage: vi.fn(),
}));

vi.mock("$lib/services/api", () => ({
  readContextBreakdown,
  readUsageBreakdown,
  readThreadUsage,
  isContextBreakdownUnsupported: (cause: unknown) =>
    String(cause instanceof Error ? cause.message : cause).startsWith("harness-unsupported:context_breakdown"),
}));

vi.mock("$lib/services/accountUsage.svelte", () => ({
  accountUsage: { snapshot: null, byLimitId: {}, error: null },
}));

const stats: ContextStats = {
  contextWindow: 200_000,
  usedTokens: 42_000,
  usedFraction: 0.16,
  percentUsed: 16,
  percentRemaining: 84,
  sessionTotalTokens: 120_600,
  sessionInputTokens: 114_400,
  sessionCachedInputTokens: 96_000,
  sessionOutputTokens: 6_200,
  sessionReasoningTokens: 1_100,
};

const categories = { system: 12_000, skills: 0, user: 1_000, tool: 3_000, output: 500, reasoning: 0, unattributed: 0 };

const breakdown: UsageBreakdown = {
  tokens: {
    inputTokens: 16_000,
    cachedInputTokens: 9_000,
    cacheWriteInputTokens: 0,
    outputTokens: 700,
    reasoningOutputTokens: 200,
    totalTokens: 16_700,
  },
  categories: { ...categories, reasoning: 200 },
  reportedCostUsd: null,
  unpricedTokens: true,
  turns: 2,
  byModel: [
    {
      model: "gpt-5",
      harness: "codex",
      tokens: {
        inputTokens: 16_000,
        cachedInputTokens: 9_000,
        cacheWriteInputTokens: 0,
        outputTokens: 700,
        reasoningOutputTokens: 200,
        totalTokens: 16_700,
      },
      costUsd: null,
      turns: 2,
    },
  ],
  byThread: [],
  context: {
    categories,
    totalTokens: 16_500,
    contextWindow: 200_000,
    source: "estimate",
    parts: null,
    partsScaled: false,
  },
};

beforeEach(() => {
  vi.clearAllMocks();
  readThreadUsage.mockResolvedValue(null);
  readUsageBreakdown.mockResolvedValue(breakdown);
});

describe("ThreadStatus", () => {
  it("falls back to the estimated composition when the harness cannot report one", async () => {
    readContextBreakdown.mockRejectedValue(new Error("harness-unsupported:context_breakdown: Codex thread"));
    render(ThreadStatus, { stats, threadId: "t1", model: "gpt-5", costUsd: 0.05 });

    const composition = await screen.findByTestId("context-composition");
    expect(composition).toHaveTextContent("≈ estimated");
    expect(composition).toHaveTextContent("System prompt");
    expect(composition).toHaveTextContent("≈ 12,000");
    expect(composition).toHaveTextContent("Tool use");
    expect(readUsageBreakdown).toHaveBeenCalledWith({ kind: "thread", threadId: "t1" });

    const spend = await screen.findByTestId("spend-by-category");
    await waitFor(() => expect(spend).toHaveTextContent("Spend by category"));
    expect(spend).toHaveTextContent("Reasoning");
    expect(spend).toHaveTextContent("Estimated cost");
  });

  it("shows the harness's own composition as reported", async () => {
    const reported: ContextComposition = {
      categories: { ...categories, unattributed: 0 },
      totalTokens: 16_500,
      contextWindow: 200_000,
      source: "harness",
      parts: [
        { kind: "tools", label: "System tools", tokens: 9_000, source: "exact", detail: "System tools" },
        { kind: "baseInstructions", label: "System prompt", tokens: 3_000, source: "exact", detail: "System prompt" },
      ],
      partsScaled: false,
    };
    readContextBreakdown.mockResolvedValue(reported);
    render(ThreadStatus, { stats, threadId: "t1" });

    const composition = await screen.findByTestId("context-composition");
    expect(composition).toHaveTextContent("reported");
    expect(composition).not.toHaveTextContent("≈");

    const parts = screen.getByTestId("prompt-parts");
    await fireEvent.click(screen.getByRole("button", { name: /What is in the system prompt/ }));
    await waitFor(() => expect(parts).toHaveTextContent("System tools"));
    expect(parts).toHaveTextContent("9,000");
    expect(parts).not.toHaveTextContent("≈");
  });

  it("lists the estimated parts of the system prompt with the tool remainder last", async () => {
    const estimated: ContextComposition = {
      categories,
      totalTokens: 16_500,
      contextWindow: 200_000,
      source: "estimate",
      parts: [
        { kind: "tools", label: "Tool definitions and other", tokens: 6_000, source: "estimated", detail: null },
        { kind: "agentsMd", label: "AGENTS.md", tokens: 1_000, source: "estimated", detail: "/repo" },
        { kind: "baseInstructions", label: "Base instructions", tokens: 5_000, source: "estimated", detail: null },
      ],
      partsScaled: true,
    };
    readContextBreakdown.mockResolvedValue(estimated);
    render(ThreadStatus, { stats, threadId: "t1" });

    const parts = await screen.findByTestId("prompt-parts");
    await fireEvent.click(screen.getByRole("button", { name: /What is in the system prompt/ }));
    await waitFor(() => expect(parts).toHaveTextContent("Base instructions"));
    const text = parts.textContent ?? "";
    expect(text.indexOf("Base instructions")).toBeLessThan(text.indexOf("AGENTS.md"));
    expect(text.indexOf("AGENTS.md")).toBeLessThan(text.indexOf("Tool definitions and other"));
    expect(parts).toHaveTextContent("/repo");
    expect(parts).toHaveTextContent("≈ 5,000");
    expect(parts).toHaveTextContent("Scaled to fit");
    expect(parts).toHaveTextContent("what remains");
  });

  it("shows no parts list when the harness cannot name them", async () => {
    readContextBreakdown.mockRejectedValue(new Error("harness-unsupported:context_breakdown: Codex thread"));
    render(ThreadStatus, { stats, threadId: "t1" });
    await screen.findByTestId("context-composition");
    expect(screen.queryByTestId("prompt-parts")).not.toBeInTheDocument();
  });

  it("leaves the composition out when a real error occurs", async () => {
    readContextBreakdown.mockRejectedValue(new Error("claude exited"));
    render(ThreadStatus, { stats, threadId: "t1" });

    await screen.findByText("In context now");
    expect(screen.queryByTestId("context-composition")).not.toBeInTheDocument();
    expect(readUsageBreakdown).toHaveBeenCalledTimes(1);
  });
});
