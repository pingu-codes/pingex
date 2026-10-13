/**
 * Token budgets as people type and read them: `500k`, `2m`, `250000`.
 */

/** Presets the budget dialog offers, in tokens. */
export const BUDGET_PRESETS = [250_000, 500_000, 1_000_000, 2_000_000] as const;

/**
 * Parse a budget the user typed. Accepts a plain integer or a `k`/`m`
 * suffix (case-insensitive, thousands separators allowed). Returns null for
 * anything that is not a positive whole number of tokens.
 */
export function parseTokenBudget(text: string): number | null {
  const match = /^\s*(\d[\d,_]*(?:\.\d+)?)\s*([kKmM])?\s*$/.exec(text);
  if (!match) return null;
  const digits = Number(match[1].replace(/[,_]/g, ""));
  if (!Number.isFinite(digits) || digits <= 0) return null;
  const scale = match[2] ? (match[2].toLowerCase() === "k" ? 1_000 : 1_000_000) : 1;
  const tokens = Math.round(digits * scale);
  return tokens > 0 ? tokens : null;
}
