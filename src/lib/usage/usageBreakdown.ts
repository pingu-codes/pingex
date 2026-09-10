/**
 * The vocabulary of the usage views: which categories exist, in what order,
 * which are exact and which estimated, and how a breakdown becomes bar
 * segments and a cost line. Rendering lives in the components; everything
 * here is plain data so it can be tested without them.
 */
import { estimateGroupCost } from "$lib/thread/usageCost";
import type { CategoryTokens, ModelUsage, PromptPart } from "$lib/types";

export type UsageCategory = keyof CategoryTokens;

export interface CategoryMeta {
  id: UsageCategory;
  label: string;
  /** Tailwind background class for the bar segment and legend swatch. */
  color: string;
  /** Whether the figure is derived from message sizes rather than reported. */
  estimated: boolean;
}

/** Display order: the prompt first, then what the model added. */
export const USAGE_CATEGORIES: CategoryMeta[] = [
  { id: "system", label: "System prompt", color: "bg-surface-400-600", estimated: true },
  { id: "skills", label: "Skills", color: "bg-tertiary-500", estimated: true },
  { id: "user", label: "Your messages", color: "bg-primary-500", estimated: true },
  { id: "tool", label: "Tool use", color: "bg-warning-500", estimated: true },
  { id: "output", label: "Replies", color: "bg-success-500", estimated: false },
  { id: "reasoning", label: "Reasoning", color: "bg-secondary-500", estimated: false },
  { id: "unattributed", label: "Not attributed", color: "bg-surface-200-800", estimated: false },
];

export interface UsageSegment extends CategoryMeta {
  tokens: number;
  /** 0–100, of the categories' sum. */
  percent: number;
}

/** Non-empty categories in display order, sized as shares of their sum.
 *  `exact` marks every segment as reported rather than estimated (a
 *  harness-reported context composition). */
export function segmentsFor(categories: CategoryTokens, options: { exact?: boolean } = {}): UsageSegment[] {
  const total = USAGE_CATEGORIES.reduce((sum, meta) => sum + Math.max(categories[meta.id], 0), 0);
  if (total === 0) return [];
  return USAGE_CATEGORIES.filter((meta) => categories[meta.id] > 0).map((meta) => ({
    ...meta,
    estimated: options.exact ? false : meta.estimated,
    tokens: categories[meta.id],
    percent: (categories[meta.id] / total) * 100,
  }));
}

export interface CostSummary {
  /** Everything priced, reported plus estimated. */
  usd: number;
  /** Whether any of it was estimated from a price table. */
  estimated: boolean;
}

/** Total spend for a set of model groups: what the harness reported where it
 *  did, the API list price for the rest. Null when there is nothing to price. */
export function costFor(byModel: ModelUsage[], reportedCostUsd: number | null | undefined): CostSummary | null {
  if (byModel.length === 0 && reportedCostUsd == null) return null;
  let estimated = false;
  let usd = 0;
  for (const group of byModel) {
    if (typeof group.costUsd === "number") {
      usd += group.costUsd;
      continue;
    }
    if (group.tokens.totalTokens === 0) continue;
    estimated = true;
    usd += estimateGroupCost(
      group.model,
      group.tokens.inputTokens,
      group.tokens.cachedInputTokens,
      group.tokens.outputTokens,
    );
  }
  if (byModel.length === 0 && typeof reportedCostUsd === "number") usd = reportedCostUsd;
  return { usd, estimated };
}

/** `$1.20`, `≈ $1.20` for an estimate, `<$0.01` for a rounding-to-nothing. */
export function costLabel(cost: CostSummary | null): string | null {
  if (!cost) return null;
  const amount = cost.usd > 0 && cost.usd < 0.01 ? "<$0.01" : `$${cost.usd.toFixed(2)}`;
  return cost.estimated ? `≈ ${amount}` : amount;
}

export interface PromptPartRow {
  key: string;
  label: string;
  /** Where the part came from when one kind has several (an AGENTS.md directory). */
  detail: string | null;
  tokens: number;
  /** 0–100, of the whole context. */
  percentOfContext: number;
  /** 0–100, of the system prompt. */
  percentOfPrompt: number;
  estimated: boolean;
}

/** The Prompt parts of a composition as rows: by size, the remainder
 *  ("tools" that nothing names) last. `contextTotal` is the whole context the
 *  percentages are of; the prompt share is of the parts' own sum. */
export function promptPartRows(parts: PromptPart[], contextTotal: number): PromptPartRow[] {
  const live = parts.filter((part) => part.tokens > 0);
  const promptTotal = live.reduce((sum, part) => sum + part.tokens, 0);
  const share = (tokens: number, of: number) => (of > 0 ? (tokens / of) * 100 : 0);
  const named = live.filter((part) => part.kind !== "tools").sort((a, b) => b.tokens - a.tokens);
  const tools = live.filter((part) => part.kind === "tools").sort((a, b) => b.tokens - a.tokens);
  return [...named, ...tools].map((part, index) => ({
    key: `${part.kind}:${part.detail ?? index}`,
    label: part.label,
    detail: part.detail && part.detail !== part.label ? part.detail : null,
    tokens: part.tokens,
    percentOfContext: share(part.tokens, contextTotal),
    percentOfPrompt: share(part.tokens, promptTotal),
    estimated: part.source === "estimated",
  }));
}

/** A token figure, prefixed when it is an estimate. */
export function approx(label: string, estimated: boolean): string {
  return estimated ? `≈ ${label}` : label;
}

export type UsageRange = "all" | "30d" | "7d";

export const USAGE_RANGES: { id: UsageRange; label: string }[] = [
  { id: "all", label: "All time" },
  { id: "30d", label: "30 days" },
  { id: "7d", label: "7 days" },
];

/** The unix time (seconds) a range starts at, or null for all time. */
export function rangeSince(range: UsageRange, now: number = Date.now()): number | null {
  const day = 24 * 60 * 60;
  const nowSeconds = Math.floor(now / 1000);
  if (range === "30d") return nowSeconds - 30 * day;
  if (range === "7d") return nowSeconds - 7 * day;
  return null;
}
