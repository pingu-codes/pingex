/** One decimal below 100, whole numbers above, and never a trailing `.0`. */
function compact(value: number): string {
  return value.toFixed(Math.abs(value) >= 100 ? 0 : 1).replace(/\.0$/, "");
}

/** `512 B`, `2 KB`, `12.3 KB`, `3.4 MB`. */
export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${compact(bytes / 1024)} KB`;
  return `${compact(bytes / (1024 * 1024))} MB`;
}

/** Thousands-separated token count, e.g. `128,000`. */
export function formatTokens(tokens: number): string {
  return Math.round(tokens).toLocaleString("en-US");
}

/**
 * Compact token count for tight spaces: `840`, `1.2k`, `128k`, `2.5M`. The
 * goal-budget dialog reads these back, so they stay `parseTokenBudget` input.
 */
export function formatTokensShort(tokens: number): string {
  const value = Math.round(tokens);
  if (Math.abs(value) >= 1_000_000) return `${compact(value / 1_000_000)}M`;
  if (Math.abs(value) >= 1_000) return `${compact(value / 1_000)}k`;
  return String(value);
}

export interface RelativeTimeOptions {
  /** Appended to a count: `" ago"` turns `3d` into `3d ago`. */
  suffix?: string;
  /** `"day"`: Today / Yesterday / `3d`. `"minute"`: just now / `5m` / `2h` / `3d`. */
  granularity?: "day" | "minute";
  /** The present, in Unix seconds like the timestamp. */
  now?: number;
}

/** How long ago a Unix-seconds timestamp was. Future timestamps read as now. */
export function relativeTime(
  timestamp: number,
  { suffix = "", granularity = "day", now = Date.now() / 1000 }: RelativeTimeOptions = {},
): string {
  const elapsed = Math.max(0, now - timestamp);
  const days = Math.floor(elapsed / 86400);
  if (granularity === "day") {
    if (days === 0) return "Today";
    if (days === 1) return "Yesterday";
  } else {
    if (elapsed < 60) return "just now";
    if (elapsed < 3600) return `${Math.floor(elapsed / 60)}m${suffix}`;
    if (elapsed < 86400) return `${Math.floor(elapsed / 3600)}h${suffix}`;
  }
  return `${days}d${suffix}`;
}
