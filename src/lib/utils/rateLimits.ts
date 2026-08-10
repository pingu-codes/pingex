import type { RateLimitSnapshot, RateLimitWindow } from "$lib/types";

/** A rate-limit window shaped for display. */
export interface UsageWindow {
  /** "5h", "Weekly", "Monthly", … derived from the window length. */
  label: string;
  usedPercent: number;
  remainingPercent: number;
  /** Unix seconds when the window resets, when Codex reported one. */
  resetsAt: number | null;
  windowDurationMins: number | null;
}

const MINUTES_PER_HOUR = 60;
const MINUTES_PER_DAY = 60 * 24;

/** Names a window by its length, matching how Codex describes its limits. */
export function windowLabel(minutes: number | null | undefined): string {
  if (!minutes || minutes <= 0) return "Usage";
  if (minutes < MINUTES_PER_DAY) {
    const hours = Math.round(minutes / MINUTES_PER_HOUR);
    return hours <= 1 ? "Hourly" : `${hours}h`;
  }
  const days = Math.round(minutes / MINUTES_PER_DAY);
  if (days === 1) return "Daily";
  if (days === 7) return "Weekly";
  if (days >= 28 && days <= 31) return "Monthly";
  return `${days}d`;
}

function toUsageWindow(window: RateLimitWindow | null | undefined): UsageWindow | null {
  if (!window) return null;
  const usedPercent = Math.min(Math.max(Math.round(window.usedPercent), 0), 100);
  return {
    label: windowLabel(window.windowDurationMins),
    usedPercent,
    remainingPercent: 100 - usedPercent,
    resetsAt: window.resetsAt ?? null,
    windowDurationMins: window.windowDurationMins ?? null,
  };
}

/** Both windows of a snapshot, shortest first, dropping the ones Codex omitted. */
export function usageWindows(snapshot: RateLimitSnapshot | null): UsageWindow[] {
  if (!snapshot) return [];
  const windows = [toUsageWindow(snapshot.primary), toUsageWindow(snapshot.secondary)].filter(
    (window): window is UsageWindow => window !== null,
  );
  return windows.sort((a, b) => (a.windowDurationMins ?? 0) - (b.windowDurationMins ?? 0));
}

/**
 * The window the "remaining usage" headline should track: the weekly one when
 * Codex reports it, otherwise the longest window available.
 */
export function primaryUsageWindow(snapshot: RateLimitSnapshot | null): UsageWindow | null {
  const windows = usageWindows(snapshot);
  if (windows.length === 0) return null;
  const weekly = windows.find((window) => window.label === "Weekly");
  return weekly ?? windows[windows.length - 1];
}

/** Tailwind class for a usage bar, warning as the window fills up. */
export function usageToneClass(usedPercent: number): string {
  if (usedPercent >= 90) return "bg-error-500";
  if (usedPercent >= 75) return "bg-warning-500";
  return "bg-primary-500";
}

/**
 * Human reset countdown, e.g. `resets in 3d 4h`. `now` is injectable so the
 * caller controls the clock (and tests stay deterministic).
 */
export function resetLabel(resetsAt: number | null, now: number = Date.now()): string | null {
  if (!resetsAt) return null;
  const seconds = Math.round(resetsAt - now / 1000);
  if (seconds <= 0) return "resets now";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `resets in ${Math.max(minutes, 1)}m`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) {
    const restMinutes = minutes % 60;
    return restMinutes > 0 ? `resets in ${hours}h ${restMinutes}m` : `resets in ${hours}h`;
  }
  const days = Math.floor(hours / 24);
  const restHours = hours % 24;
  return restHours > 0 ? `resets in ${days}d ${restHours}h` : `resets in ${days}d`;
}

/**
 * Merge a sparse `account/rateLimits/updated` payload into the last known
 * snapshot. Codex documents rolling updates as partial: a missing field means
 * "unchanged", not "cleared".
 */
export function mergeSnapshot(previous: RateLimitSnapshot | null, update: RateLimitSnapshot): RateLimitSnapshot {
  if (!previous) return update;
  return {
    ...previous,
    ...Object.fromEntries(Object.entries(update).filter(([, value]) => value !== null && value !== undefined)),
  };
}
