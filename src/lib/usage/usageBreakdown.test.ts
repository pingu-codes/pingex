import { describe, expect, it } from "vitest";
import type { CategoryTokens, ModelUsage, PromptPart } from "$lib/types";
import {
  approx,
  costFor,
  costLabel,
  promptPartRows,
  rangeSince,
  segmentsFor,
  USAGE_CATEGORIES,
} from "./usageBreakdown";

const categories: CategoryTokens = {
  system: 500,
  skills: 0,
  user: 250,
  tool: 0,
  output: 200,
  reasoning: 50,
  unattributed: 0,
};

const tokens = (input: number, cached: number, output: number) => ({
  inputTokens: input,
  cachedInputTokens: cached,
  cacheWriteInputTokens: 0,
  outputTokens: output,
  reasoningOutputTokens: 0,
  totalTokens: input + output,
});

describe("segmentsFor", () => {
  it("keeps display order, drops empty categories, and shares out the sum", () => {
    const segments = segmentsFor(categories);
    expect(segments.map((s) => s.id)).toEqual(["system", "user", "output", "reasoning"]);
    expect(segments.map((s) => Math.round(s.percent))).toEqual([50, 25, 20, 5]);
    expect(segments.reduce((sum, s) => sum + s.percent, 0)).toBeCloseTo(100);
  });

  it("marks prompt categories estimated and reported ones exact", () => {
    const byId = Object.fromEntries(segmentsFor(categories).map((s) => [s.id, s.estimated]));
    expect(byId).toEqual({ system: true, user: true, output: false, reasoning: false });
    expect(segmentsFor(categories, { exact: true }).every((s) => !s.estimated)).toBe(true);
  });

  it("is empty when nothing was used", () => {
    const empty = Object.fromEntries(USAGE_CATEGORIES.map((meta) => [meta.id, 0])) as unknown as CategoryTokens;
    expect(segmentsFor(empty)).toEqual([]);
  });
});

describe("costFor", () => {
  it("adds reported costs and prices the rest from the table", () => {
    const groups: ModelUsage[] = [
      { model: "claude-haiku-4-5", harness: "claude", tokens: tokens(1000, 0, 100), costUsd: 0.5, turns: 1 },
      { model: "gpt-5", harness: "codex", tokens: tokens(1_000_000, 0, 0), costUsd: null, turns: 1 },
    ];
    const cost = costFor(groups, 0.5);
    expect(cost).toEqual({ usd: 0.5 + 1.25, estimated: true });
    expect(costLabel(cost)).toBe("≈ $1.75");
  });

  it("is exact when every group was priced by its harness", () => {
    const groups: ModelUsage[] = [
      { model: "claude-opus-5", harness: "claude", tokens: tokens(10, 0, 10), costUsd: 0.2, turns: 1 },
    ];
    expect(costLabel(costFor(groups, 0.2))).toBe("$0.20");
  });

  it("has nothing to say without groups or a reported figure", () => {
    expect(costFor([], null)).toBeNull();
    expect(costLabel(null)).toBeNull();
    expect(costLabel({ usd: 0.001, estimated: false })).toBe("<$0.01");
  });
});

describe("ranges and labels", () => {
  it("turns a range into a unix start time", () => {
    const now = 1_700_000_000_000;
    expect(rangeSince("all", now)).toBeNull();
    expect(rangeSince("7d", now)).toBe(1_700_000_000 - 7 * 86_400);
    expect(rangeSince("30d", now)).toBe(1_700_000_000 - 30 * 86_400);
  });

  it("prefixes estimates", () => {
    expect(approx("1,200", true)).toBe("≈ 1,200");
    expect(approx("1,200", false)).toBe("1,200");
  });
});

describe("promptPartRows", () => {
  const parts: PromptPart[] = [
    { kind: "tools", label: "Tool definitions and other", tokens: 2_000, source: "estimated", detail: null },
    { kind: "agentsMd", label: "AGENTS.md", tokens: 1_000, source: "estimated", detail: "/repo" },
    { kind: "baseInstructions", label: "Base instructions", tokens: 5_000, source: "estimated", detail: null },
    { kind: "environment", label: "Environment context", tokens: 0, source: "estimated", detail: null },
  ];

  it("orders by size with the tool remainder last and drops empty parts", () => {
    const rows = promptPartRows(parts, 40_000);
    expect(rows.map((row) => row.label)).toEqual(["Base instructions", "AGENTS.md", "Tool definitions and other"]);
    expect(rows[0].percentOfContext).toBeCloseTo(12.5);
    expect(rows[0].percentOfPrompt).toBeCloseTo(62.5);
    expect(rows[1].detail).toBe("/repo");
    expect(rows.every((row) => row.estimated)).toBe(true);
  });

  it("hides a detail that only repeats the label and marks exact parts", () => {
    const rows = promptPartRows(
      [{ kind: "tools", label: "System tools", tokens: 10, source: "exact", detail: "System tools" }],
      100,
    );
    expect(rows[0].detail).toBeNull();
    expect(rows[0].estimated).toBe(false);
    expect(promptPartRows([], 0)).toEqual([]);
  });
});
