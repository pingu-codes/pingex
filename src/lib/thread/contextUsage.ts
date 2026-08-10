import type { ThreadTokenUsage } from "$lib/types";

/**
 * Tokens the system prompt, tool definitions and instructions occupy before any
 * conversation happens. Codex excludes them from both ends of the ratio so the
 * meter reads "how much of the space I can actually use is gone".
 */
export const BASELINE_TOKENS = 12_000;

export interface ContextStats {
  /** Model context window in tokens, when Codex has reported one. */
  contextWindow: number | null;
  /** Tokens the most recent request occupied — the live context size. */
  usedTokens: number;
  /** 0–1 share of the usable window consumed; null while the window is unknown. */
  usedFraction: number | null;
  /** Rounded percentages matching the Codex TUI status line. */
  percentUsed: number | null;
  percentRemaining: number | null;
  /** Cumulative usage across every request in the thread. */
  sessionTotalTokens: number;
  sessionInputTokens: number;
  sessionCachedInputTokens: number;
  sessionOutputTokens: number;
  sessionReasoningTokens: number;
}

/**
 * Derives meter-ready numbers from a `thread/tokenUsage/updated` payload.
 * Mirrors `TokenUsage::percent_of_context_window_remaining` in the Codex TUI:
 * the live context size is the *last* request's total, while the accumulated
 * `total` only describes what the thread has spent overall.
 */
export function contextStats(usage: ThreadTokenUsage): ContextStats {
  const contextWindow = usage.modelContextWindow ?? null;
  const usedTokens = Math.max(usage.last.totalTokens, 0);
  const stats: ContextStats = {
    contextWindow,
    usedTokens,
    usedFraction: null,
    percentUsed: null,
    percentRemaining: null,
    sessionTotalTokens: usage.total.totalTokens,
    sessionInputTokens: usage.total.inputTokens,
    sessionCachedInputTokens: usage.total.cachedInputTokens,
    sessionOutputTokens: usage.total.outputTokens,
    sessionReasoningTokens: usage.total.reasoningOutputTokens,
  };
  if (contextWindow === null) return stats;
  if (contextWindow <= BASELINE_TOKENS) {
    stats.usedFraction = 1;
    stats.percentUsed = 100;
    stats.percentRemaining = 0;
    return stats;
  }
  const usableWindow = contextWindow - BASELINE_TOKENS;
  const usedInWindow = Math.max(usedTokens - BASELINE_TOKENS, 0);
  stats.usedFraction = Math.min(usedInWindow / usableWindow, 1);
  stats.percentRemaining = Math.round((1 - stats.usedFraction) * 100);
  stats.percentUsed = 100 - stats.percentRemaining;
  return stats;
}

/** Thousands-separated token count, e.g. `128,000`. */
export function formatTokens(tokens: number): string {
  return Math.round(tokens).toLocaleString("en-US");
}

/** Compact token count for tight spaces, e.g. `1.2M`, `128K`, `840`. */
export function formatTokensShort(tokens: number): string {
  const value = Math.round(tokens);
  if (Math.abs(value) >= 1_000_000) return `${trimZero(value / 1_000_000)}M`;
  if (Math.abs(value) >= 1_000) return `${trimZero(value / 1_000)}K`;
  return String(value);
}

function trimZero(value: number): string {
  return value.toFixed(1).replace(/\.0$/, "");
}
