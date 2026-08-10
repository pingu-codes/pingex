import type { RemoteConnection } from "$lib/types";

/**
 * Per-device health derived purely from `lastSeen`. The relay does not report a
 * per-client online flag, so "online" means seen very recently, "recent" means
 * seen today, "offline" means seen longer ago, and "unknown" means never seen
 * this session (e.g. a device recorded at pairing claim before it surfaced in
 * the relay's client list).
 */
export type ConnectionHealth = "online" | "recent" | "offline" | "unknown";

const ONLINE_WINDOW_SECS = 3 * 60;
const RECENT_WINDOW_SECS = 24 * 60 * 60;

export function connectionHealth(lastSeen: number | null | undefined, now = Date.now() / 1000): ConnectionHealth {
  if (lastSeen == null) return "unknown";
  const age = now - lastSeen;
  if (age <= ONLINE_WINDOW_SECS) return "online";
  if (age <= RECENT_WINDOW_SECS) return "recent";
  return "offline";
}

export function healthLabel(health: ConnectionHealth): string {
  switch (health) {
    case "online":
      return "Online";
    case "recent":
      return "Recently active";
    case "offline":
      return "Offline";
    default:
      return "Unknown";
  }
}

/** Tailwind classes for the small status dot next to a device name. */
export function healthDotClass(health: ConnectionHealth): string {
  switch (health) {
    case "online":
      return "bg-success-500";
    case "recent":
      return "bg-warning-500";
    case "offline":
      return "bg-surface-400-600";
    default:
      return "bg-surface-300-700";
  }
}

export function lastSeenLabel(lastSeen: number | null | undefined, now = Date.now() / 1000): string {
  if (lastSeen == null) return "Never seen this session";
  const age = Math.max(0, now - lastSeen);
  if (age < 60) return "Active now";
  if (age < 3600) return `Seen ${Math.floor(age / 60)}m ago`;
  if (age < 86400) return `Seen ${Math.floor(age / 3600)}h ago`;
  return `Seen ${Math.floor(age / 86400)}d ago`;
}

export function platformLabel(platform: string | null | undefined): string {
  if (!platform) return "Device";
  const known: Record<string, string> = {
    ios: "iPhone",
    ipados: "iPad",
    android: "Android",
    macos: "Mac",
    windows: "Windows",
    linux: "Linux",
    web: "Browser",
  };
  return known[platform.toLowerCase()] ?? platform;
}

export function scopeLabel(scope: string | null | undefined): string {
  if (!scope) return "Standard access";
  const known: Record<string, string> = {
    full: "Full control",
    read: "Read only",
    readonly: "Read only",
    limited: "Limited access",
  };
  return known[scope.toLowerCase()] ?? scope;
}

/**
 * Revoke confirmation gating: the destructive action only unlocks once the user
 * types the device's name exactly (case-insensitive, trimmed). Blank names
 * cannot be confirmed by an empty input.
 */
export function canRevoke(typed: string, deviceName: string): boolean {
  const target = deviceName.trim().toLowerCase();
  if (!target) return false;
  return typed.trim().toLowerCase() === target;
}

/** Whether the persistent app-shell indicator should light up. */
export function hasActiveConnection(connections: RemoteConnection[], now = Date.now() / 1000): boolean {
  return connections.some((connection) => {
    const health = connectionHealth(connection.lastSeen, now);
    return health === "online" || health === "recent";
  });
}

export function activeConnectionCount(connections: RemoteConnection[], now = Date.now() / 1000): number {
  return connections.filter((connection) => connectionHealth(connection.lastSeen, now) === "online").length;
}
