import { describe, expect, it } from "vitest";
import {
  activeConnectionCount,
  canRevoke,
  connectionHealth,
  hasActiveConnection,
  lastSeenLabel,
  platformLabel,
  scopeLabel,
} from "$lib/layout/connectionState";
import type { RemoteConnection } from "$lib/types";

const now = 1_000_000;

function connection(overrides: Partial<RemoteConnection>): RemoteConnection {
  return {
    clientId: "c",
    name: "Device",
    platform: "iOS",
    deviceModel: null,
    appVersion: null,
    pairedAt: now - 1000,
    lastSeen: now,
    scope: "full",
    source: "protocol",
    ...overrides,
  };
}

describe("connectionHealth", () => {
  it("maps recency to a health state", () => {
    expect(connectionHealth(now - 30, now)).toBe("online");
    expect(connectionHealth(now - 60 * 60, now)).toBe("recent");
    expect(connectionHealth(now - 3 * 86400, now)).toBe("offline");
    expect(connectionHealth(null, now)).toBe("unknown");
    expect(connectionHealth(undefined, now)).toBe("unknown");
  });

  it("treats the boundaries inclusively", () => {
    expect(connectionHealth(now - 3 * 60, now)).toBe("online");
    expect(connectionHealth(now - 3 * 60 - 1, now)).toBe("recent");
    expect(connectionHealth(now - 24 * 60 * 60, now)).toBe("recent");
    expect(connectionHealth(now - 24 * 60 * 60 - 1, now)).toBe("offline");
  });
});

describe("lastSeenLabel", () => {
  it("phrases the age in the largest unit", () => {
    expect(lastSeenLabel(now - 10, now)).toBe("Active now");
    expect(lastSeenLabel(now - 5 * 60, now)).toBe("Seen 5m ago");
    expect(lastSeenLabel(now - 3 * 3600, now)).toBe("Seen 3h ago");
    expect(lastSeenLabel(now - 2 * 86400, now)).toBe("Seen 2d ago");
    expect(lastSeenLabel(null, now)).toBe("Never seen this session");
  });
});

describe("platformLabel / scopeLabel", () => {
  it("humanizes known values and passes through the rest", () => {
    expect(platformLabel("ios")).toBe("iPhone");
    expect(platformLabel("android")).toBe("Android");
    expect(platformLabel("BeOS")).toBe("BeOS");
    expect(platformLabel(null)).toBe("Device");
    expect(scopeLabel("full")).toBe("Full control");
    expect(scopeLabel(null)).toBe("Standard access");
  });
});

describe("canRevoke (confirmation gating)", () => {
  it("only unlocks on an exact name match", () => {
    expect(canRevoke("Ciaran's iPhone", "Ciaran's iPhone")).toBe(true);
    expect(canRevoke("  ciaran's iphone  ", "Ciaran's iPhone")).toBe(true);
    expect(canRevoke("wrong", "Ciaran's iPhone")).toBe(false);
    expect(canRevoke("", "Ciaran's iPhone")).toBe(false);
  });

  it("never confirms a blank device name with blank input", () => {
    expect(canRevoke("", "")).toBe(false);
    expect(canRevoke("   ", "   ")).toBe(false);
  });
});

describe("hasActiveConnection / activeConnectionCount", () => {
  it("lights up when any device is online or recent", () => {
    const list = [
      connection({ clientId: "a", lastSeen: now - 30 }),
      connection({ clientId: "b", lastSeen: now - 5 * 86400 }),
    ];
    expect(hasActiveConnection(list, now)).toBe(true);
    expect(activeConnectionCount(list, now)).toBe(1);
  });

  it("stays dark when every device is offline or unknown", () => {
    const list = [
      connection({ clientId: "a", lastSeen: now - 5 * 86400 }),
      connection({ clientId: "b", lastSeen: null }),
    ];
    expect(hasActiveConnection(list, now)).toBe(false);
    expect(activeConnectionCount(list, now)).toBe(0);
  });
});
